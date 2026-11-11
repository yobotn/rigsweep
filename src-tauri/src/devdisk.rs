//! Dev Disk Cleanup: reclaim disk space that dev tools leave behind, in two parts.
//!
//! - Projects: build output and dependency folders (node_modules, .next, target, bin/obj, .venv…)
//!   found under the scan roots. A folder only counts when the file that proves what it is sits
//!   next to it (package.json, Cargo.toml, *.csproj, pyvenv.cfg…), so a random `build` or `target`
//!   folder is never touched. Only projects nobody has edited for a while are suggested.
//! - Package caches: npm, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright,
//!   Puppeteer, Hugging Face. When a tool has its own cleanup command (pnpm store prune, uv cache
//!   prune, go clean) that is used instead of deleting files. Each cache can be switched off
//!   ("never clean"); the expensive-to-download ones start switched off.
//!
//! Exclusions: a built-in list (AppData, tool homes such as .cargo/.nvm/.vscode, .git, Windows)
//! is always skipped, because global installs and editor extensions keep their own node_modules
//! there. The user's list adds full paths or folder-name patterns on top.
//!
//! Nothing is deleted automatically. The UI sends (project path, folder name) or a cache id; every
//! target is verified again here (under a root, not excluded, no junctions on the way, manifest
//! still present, not in use) before anything is removed. A project folder is first renamed, so a
//! project is never left with a half-deleted node_modules.

use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::browserclean::empty_dir;
use crate::util;

const ATTR_REPARSE: u32 = 0x400;
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x400000;

/// How deep below a scan root projects are looked for.
const MAX_DEPTH: usize = 9;
/// Marker for a folder renamed by a cleanup that didn't finish (locked files).
const LEFTOVER: &str = ".rigsweep-del-";

// ---------- settings defaults ----------

/// The user profile (Desktop, Documents, Downloads, source, OneDrive…) plus common dev roots
/// elsewhere, if they exist on this PC.
pub fn default_roots() -> Vec<String> {
    let mut v = vec!["%USERPROFILE%".to_string()];
    for d in ["C", "D", "E"] {
        for n in ["dev", "src", "code", "projects", "repos", "work", "git"] {
            let p = format!("{d}:\\{n}");
            if Path::new(&p).is_dir() {
                v.push(p);
            }
        }
    }
    v
}

/// Caches that cost a big re-download start switched off.
pub fn default_disabled() -> Vec<String> {
    ["playwright", "puppeteer", "hf", "gradle", "maven", "nugetpkgs", "gomod"].iter().map(|s| s.to_string()).collect()
}

pub fn default_stale_days() -> u32 {
    30
}

pub struct Config {
    pub roots: Vec<String>,
    pub excludes: Vec<String>,
    pub disabled: Vec<String>,
    pub stale_days: u32,
}

// ---------- exclusions ----------

/// Folder names never descended into, anywhere.
const SKIP_NAMES: &[&str] = &[
    ".git", ".hg", ".svn", "$recycle.bin", "system volume information", "windows", "program files",
    "program files (x86)", "programdata", "recovery", "$windows.~bt", "$windows.~ws", "msocache",
    "npm-cache", "_npx", "_cacache",
];

/// Folders in the user profile that belong to tools, not projects. Their node_modules / target
/// folders are global installs, extensions or toolchains, never project output.
const SKIP_HOME: &[&str] = &[
    "AppData", ".cargo", ".rustup", ".nuget", ".gradle", ".m2", ".nvm", "nvm", ".volta", ".fnm", ".bun", ".deno",
    ".npm", ".yarn", ".pnpm-store", ".cache", ".vscode", ".vscode-insiders", ".vscode-server", ".cursor",
    ".cursor-server", ".windsurf", ".vs", ".conda", "anaconda3", "miniconda3", "miniforge3", ".pyenv", "scoop",
    "go", ".docker", ".local", ".android", ".dotnet", ".ollama", ".lmstudio", ".rye", ".pub-cache", ".claude",
    ".codex", ".gemini", ".continue", ".jdks", ".sdkman", ".espressif", ".platformio", "pipx",
];

fn lower(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// `a` is `b` or inside it (both lower-case, no trailing backslash).
fn within(a: &str, b: &str) -> bool {
    a == b || (a.len() > b.len() && a.starts_with(b) && a.as_bytes()[b.len()] == b'\\')
}

/// `*` matches any run of characters; case-insensitive (both sides already lower-case).
fn glob(pat: &str, s: &str) -> bool {
    let parts: Vec<&str> = pat.split('*').collect();
    if parts.len() == 1 {
        return pat == s;
    }
    let mut rest = s;
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            match rest.strip_prefix(part) {
                Some(r) => rest = r,
                None => return false,
            }
        } else if i == parts.len() - 1 {
            return rest.ends_with(part);
        } else if let Some(at) = rest.find(part) {
            rest = &rest[at + part.len()..];
        } else {
            return false;
        }
    }
    true
}

struct Excludes {
    /// the user profile: every dot-folder directly inside it is a tool home (~/.deployai, ~/.foo)
    home: String,
    /// absolute paths
    paths: Vec<String>,
    /// relative paths like `work\old`: matched as a run of folder names anywhere
    rels: Vec<String>,
    /// single folder-name patterns like `vendor` or `legacy-*`
    names: Vec<String>,
}

impl Excludes {
    fn new(user: &[String]) -> Excludes {
        let mut ex = Excludes { home: String::new(), paths: vec![], rels: vec![], names: SKIP_NAMES.iter().map(|s| s.to_string()).collect() };
        if let Ok(home) = std::env::var("USERPROFILE") {
            ex.home = lower(Path::new(&home));
            let home = Path::new(&home);
            ex.paths.extend(SKIP_HOME.iter().map(|n| lower(&home.join(n))));
        }
        for v in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            if let Ok(p) = std::env::var(v) {
                ex.paths.push(lower(Path::new(&p)));
            }
        }
        for e in user {
            let e = util::expand_env(e.trim()).replace('/', "\\");
            let e = e.trim_end_matches('\\').to_lowercase();
            if e.is_empty() {
                continue;
            }
            if e.contains(':') || e.starts_with("\\\\") {
                ex.paths.push(e);
            } else if e.contains('\\') {
                ex.rels.push(e.trim_start_matches('\\').to_string());
            } else {
                ex.names.push(e);
            }
        }
        ex
    }

    fn name(&self, name: &str) -> bool {
        let n = name.to_lowercase();
        self.names.iter().any(|p| glob(p, &n))
    }

    /// Checks the folder itself (by name and full path); ancestors are checked while walking.
    fn dir(&self, p: &Path) -> bool {
        let s = lower(p);
        if let Some(n) = p.file_name().map(|n| n.to_string_lossy()) {
            if self.name(&n) || (n.starts_with('.') && !self.home.is_empty() && p.parent().is_some_and(|par| lower(par) == self.home)) {
                return true;
            }
        }
        self.paths.iter().any(|x| within(&s, x)) || self.rels.iter().any(|r| s.ends_with(&format!("\\{r}")))
    }

    /// Full check of every folder from `root` down to `p` (used when verifying a target).
    fn any_on_path(&self, root: &Path, p: &Path) -> bool {
        let mut cur = p.to_path_buf();
        loop {
            if self.dir(&cur) {
                return true;
            }
            if lower(&cur) == lower(root) {
                return false;
            }
            match cur.parent() {
                Some(par) => cur = par.to_path_buf(),
                None => return false,
            }
        }
    }
}

// ---------- project artifacts ----------

fn has(dir: &Path, f: &str) -> bool {
    dir.join(f).exists()
}

fn has_ext(dir: &Path, exts: &[&str]) -> bool {
    fs::read_dir(dir).map(|rd| {
        rd.flatten().any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            exts.iter().any(|x| n.ends_with(x))
        })
    })
    .unwrap_or(false)
}

fn gradle_project(parent: &Path) -> bool {
    ["build.gradle", "build.gradle.kts", "settings.gradle", "settings.gradle.kts"].iter().any(|f| has(parent, f))
}

/// (eco, tier) for a folder `name` inside `parent`, or None if it isn't a known artifact.
/// tier: "build" = rebuilt by the next build; "deps" = comes back with the next install.
fn detect(parent: &Path, name: &str) -> Option<(&'static str, &'static str)> {
    let dir = parent.join(name);
    let n = name.to_lowercase();
    if n.contains(LEFTOVER) {
        return Some(("leftover", "build"));
    }
    match n.as_str() {
        "node_modules" if has(parent, "package.json") => Some(("node", "deps")),
        ".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular" | ".docusaurus" | ".expo"
            if has(parent, "package.json") =>
        {
            Some(("node", "build"))
        }
        "target" if has(parent, "Cargo.toml") && (has(&dir, "CACHEDIR.TAG") || has(&dir, "debug") || has(&dir, "release")) => {
            Some(("rust", "build"))
        }
        "target" if has(parent, "pom.xml") && (has(&dir, "classes") || has(&dir, "maven-status")) => Some(("java", "build")),
        "build" if gradle_project(parent) && ["tmp", "classes", "libs", "intermediates", "generated", "kotlin"].iter().any(|d| has(&dir, d)) => {
            Some(("java", "build"))
        }
        ".gradle" if gradle_project(parent) || has(parent, "gradlew") => Some(("java", "build")),
        "obj" if has_ext(parent, &[".csproj", ".fsproj", ".vbproj"]) && (has(&dir, "project.assets.json") || has(&dir, "Debug") || has(&dir, "Release")) => {
            Some(("dotnet", "build"))
        }
        "bin" if has_ext(parent, &[".csproj", ".fsproj", ".vbproj"]) && (has(&dir, "Debug") || has(&dir, "Release")) => {
            Some(("dotnet", "build"))
        }
        ".venv" | "venv" | "env" | ".env" if has(&dir, "pyvenv.cfg") => Some(("python", "deps")),
        ".pytest_cache" | ".mypy_cache" | ".ruff_cache" => Some(("python", "build")),
        ".tox" if has(parent, "tox.ini") || has(parent, "pyproject.toml") || has(parent, "setup.py") => Some(("python", "build")),
        _ => None,
    }
}

fn is_artifact_name(n: &str) -> bool {
    let n = n.to_lowercase();
    n.contains(LEFTOVER)
        || matches!(
            n.as_str(),
            "node_modules" | ".next" | ".nuxt" | ".svelte-kit" | ".turbo" | ".parcel-cache" | ".angular" | ".docusaurus"
                | ".expo" | "target" | "build" | ".gradle" | "obj" | "bin" | ".venv" | "venv" | ".pytest_cache"
                | ".mypy_cache" | ".ruff_cache" | ".tox" | "__pycache__"
        )
}

// ---------- file system helpers ----------

fn attrs(m: &fs::Metadata) -> u32 {
    m.file_attributes()
}

fn is_reparse(m: &fs::Metadata) -> bool {
    attrs(m) & ATTR_REPARSE != 0
}

/// Bytes actually stored on this disk below `p`: links/junctions aren't followed, and cloud
/// placeholders (OneDrive "online-only" files) count as zero.
fn local_size(p: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if is_reparse(&m) => 0,
            Ok(m) if m.is_dir() => local_size(&e.path()),
            Ok(m) if attrs(&m) & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) != 0 => 0,
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

fn secs(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn now() -> u64 {
    secs(SystemTime::now())
}

/// When the project was last worked on: newest file or folder time outside its build and
/// dependency folders (a few levels deep, capped), or the last git operation.
fn last_active(dir: &Path) -> u64 {
    let mut newest = [".git\\index", ".git\\HEAD", ".git\\FETCH_HEAD"]
        .iter()
        .filter_map(|f| fs::metadata(dir.join(f)).and_then(|m| m.modified()).ok().map(secs))
        .max()
        .unwrap_or(0);
    let mut stack = vec![(dir.to_path_buf(), 0usize)];
    let mut seen = 0usize;
    while let Some((d, depth)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 5000 {
                return newest;
            }
            let Ok(m) = e.metadata() else { continue };
            let name = e.file_name().to_string_lossy().to_string();
            if m.is_dir() {
                if is_reparse(&m) || is_artifact_name(&name) || name.eq_ignore_ascii_case(".git") || name.starts_with('.') && depth > 0 {
                    continue;
                }
                if depth < 4 {
                    stack.push((e.path(), depth + 1));
                }
            } else if let Ok(t) = m.modified() {
                newest = newest.max(secs(t));
            }
        }
    }
    newest
}

/// Run `f` over `items` on a few threads (disk-bound work; order is kept).
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 8);
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|s| {
        for _ in 0..n.min(items.len().max(1)) {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= items.len() {
                    break;
                }
                let r = f(&items[i]);
                out.lock().unwrap().push((i, r));
            });
        }
    });
    let mut v = out.into_inner().unwrap();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|x| x.1).collect()
}

// ---------- processes ----------

/// One lower-case haystack per running process: "cwd\|exe|cmd" with forward slashes turned
/// into backslashes. Used to tell whether a project or cache is in use.
fn process_hay() -> Vec<String> {
    let Ok(mut sys) = System::new() else { return vec![] };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_cwd(UpdateKind::Always).with_exe(UpdateKind::Always),
    );
    let me = std::process::id();
    sys.processes()
        .values()
        .filter(|p| p.pid().as_u32() != me)
        .map(|p| {
            let cwd = p.cwd().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
            let exe = p.exe().map(|c| c.to_string_lossy().into_owned()).unwrap_or_default();
            let cmd = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ");
            format!("{}\\|{exe}|{cmd}", cwd.trim_end_matches(['\\', '/'])).replace('/', "\\").to_lowercase()
        })
        .collect()
}

fn path_in_use(hay: &[String], dir: &Path) -> bool {
    let needle = format!("{}\\", lower(dir));
    hay.iter().any(|h| h.contains(&needle))
}

// ---------- project scan ----------

#[derive(Serialize, Clone, Debug)]
pub struct Artifact {
    pub name: String,
    /// node | rust | java | dotnet | python | leftover
    pub eco: &'static str,
    /// "build" | "deps"
    pub tier: &'static str,
    pub bytes: u64,
    /// node_modules installed by pnpm: its files are hard links into the pnpm store
    pub pnpm: bool,
    /// pre-ticked in the UI
    pub suggest: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Project {
    pub path: String,
    pub last_active: u64,
    pub idle_days: u32,
    pub busy: bool,
    pub onedrive: bool,
    pub stale: bool,
    pub items: Vec<Artifact>,
}

struct Found {
    dir: PathBuf,
    arts: Vec<(String, &'static str, &'static str)>,
}

/// List one folder: artifacts in it, and the subfolders to walk further.
fn list_dir(dir: &Path, ex: &Excludes) -> (Vec<(String, &'static str, &'static str)>, Vec<PathBuf>) {
    let (mut arts, mut subs) = (vec![], vec![]);
    let Ok(rd) = fs::read_dir(dir) else { return (arts, subs) };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_dir() || is_reparse(&m) {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let path = e.path();
        if ex.dir(&path) {
            continue;
        }
        if let Some((eco, tier)) = detect(dir, &name) {
            arts.push((name, eco, tier));
            continue; // never look inside an artifact
        }
        if is_artifact_name(&name) {
            continue; // e.g. a `bin` folder without a project file: not ours, and not a project tree either
        }
        subs.push(path);
    }
    (arts, subs)
}

fn walk(dir: &Path, depth: usize, ex: &Excludes, out: &mut Vec<Found>) {
    let (arts, subs) = list_dir(dir, ex);
    if !arts.is_empty() {
        out.push(Found { dir: dir.to_path_buf(), arts });
    }
    if depth < MAX_DEPTH {
        for s in subs {
            walk(&s, depth + 1, ex, out);
        }
    }
}

fn expanded_roots(cfg: &Config) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = vec![];
    for r in &cfg.roots {
        let p = PathBuf::from(util::expand_env(r.trim()).replace('/', "\\"));
        if !p.is_absolute() || !p.is_dir() {
            continue;
        }
        // drop duplicates and roots nested in another root
        let l = lower(&p);
        if roots.iter().any(|x| within(&l, &lower(x))) {
            continue;
        }
        roots.retain(|x| !within(&lower(x), &l));
        roots.push(p);
    }
    roots
}

fn onedrive_roots() -> Vec<String> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .filter(|s| !s.is_empty())
        .map(|s| lower(Path::new(&s)))
        .collect()
}

fn scan_projects(cfg: &Config, hay: &[String]) -> Vec<Project> {
    let ex = Excludes::new(&cfg.excludes);
    let mut found: Vec<Found> = vec![];
    // the first two levels are listed here, the rest is walked in parallel
    let mut tasks: Vec<PathBuf> = vec![];
    for root in expanded_roots(cfg) {
        if ex.dir(&root) {
            continue;
        }
        let (arts, subs) = list_dir(&root, &ex);
        if !arts.is_empty() {
            found.push(Found { dir: root.clone(), arts });
        }
        for s in subs {
            let (arts, subs2) = list_dir(&s, &ex);
            if !arts.is_empty() {
                found.push(Found { dir: s.clone(), arts });
            }
            tasks.extend(subs2);
        }
    }
    for mut v in par_map(&tasks, |t| {
        let mut out = vec![];
        walk(t, 2, &ex, &mut out);
        out
    }) {
        found.append(&mut v);
    }

    let od = onedrive_roots();
    let stale_secs = cfg.stale_days.max(1) as u64 * 86400;
    let t = now();
    let mut projects = par_map(&found, |f| {
        let last = last_active(&f.dir);
        let idle = t.saturating_sub(last);
        let busy = path_in_use(hay, &f.dir);
        let l = lower(&f.dir);
        let onedrive = od.iter().any(|o| within(&l, o));
        let stale = idle >= stale_secs && !busy;
        let items = f
            .arts
            .iter()
            .map(|(name, eco, tier)| {
                let dir = f.dir.join(name);
                let pnpm = *eco == "node" && *tier == "deps" && dir.join(".pnpm").is_dir();
                let bytes = local_size(&dir);
                // leftovers from an interrupted cleanup are always suggested; a venv may hold
                // packages installed by hand, and pnpm frees little until the store is pruned
                let venv = *eco == "python" && *tier == "deps";
                let suggest = !busy && (*eco == "leftover" || (stale && !onedrive && !pnpm && !venv));
                Artifact { name: name.clone(), eco, tier, bytes, pnpm, suggest: suggest && bytes > 0 }
            })
            .collect();
        Project {
            path: f.dir.to_string_lossy().into_owned(),
            last_active: last,
            idle_days: (idle / 86400) as u32,
            busy,
            onedrive,
            stale,
            items,
        }
    });
    projects.iter_mut().for_each(|p| p.items.retain(|i| i.bytes > 0));
    projects.retain(|p| !p.items.is_empty());
    let total = |p: &Project| p.items.iter().map(|i| i.bytes).sum::<u64>();
    projects.sort_by(|a, b| b.stale.cmp(&a.stale).then(total(b).cmp(&total(a))));
    projects
}

// ---------- package caches ----------

#[derive(Clone, Copy, PartialEq)]
enum How {
    /// empty the folders
    Empty,
    /// keep the newest version of each browser, remove older ones
    KeepNewest,
    /// run the tool's own cleanup; never delete by hand
    Tool(&'static str, &'static [&'static str]),
    /// run the tool's own cleanup if installed, otherwise empty the folders
    ToolOrEmpty(&'static str, &'static [&'static str]),
}

struct CacheDef {
    id: &'static str,
    eco: &'static str,
    /// "safe": rebuilt from what's on disk or a quick fetch; "redownload": big downloads come back
    tier: &'static str,
    how: How,
    /// substrings of a running process's cwd/exe/cmd that mean the cache is in use
    busy: &'static [&'static str],
}

const CACHES: &[CacheDef] = &[
    CacheDef { id: "npm", eco: "node", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "npx", eco: "node", tier: "redownload", how: How::Empty, busy: &["\\_npx\\"] },
    CacheDef { id: "pnpm", eco: "node", tier: "safe", how: How::Tool("cmd.exe", &["/d", "/c", "pnpm", "store", "prune"]), busy: &[] },
    CacheDef { id: "yarn", eco: "node", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "bun", eco: "node", tier: "safe", how: How::Empty, busy: &["\\bun.exe"] },
    CacheDef { id: "nodegyp", eco: "node", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "electron", eco: "node", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "tsserver", eco: "node", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "playwright", eco: "browser", tier: "redownload", how: How::KeepNewest, busy: &["\\ms-playwright\\"] },
    CacheDef { id: "puppeteer", eco: "browser", tier: "redownload", how: How::KeepNewest, busy: &["\\puppeteer\\"] },
    CacheDef { id: "pip", eco: "python", tier: "safe", how: How::Empty, busy: &["\\pip.exe", " -m pip"] },
    CacheDef { id: "uv", eco: "python", tier: "safe", how: How::Tool("uv.exe", &["cache", "prune"]), busy: &[] },
    CacheDef { id: "hf", eco: "python", tier: "redownload", how: How::Empty, busy: &[] },
    CacheDef { id: "cargo", eco: "rust", tier: "safe", how: How::Empty, busy: &["\\cargo.exe"] },
    CacheDef { id: "cargosrc", eco: "rust", tier: "redownload", how: How::Empty, busy: &["\\cargo.exe", "rust-analyzer"] },
    CacheDef { id: "gobuild", eco: "go", tier: "safe", how: How::ToolOrEmpty("go.exe", &["clean", "-cache"]), busy: &["\\go.exe"] },
    CacheDef { id: "gomod", eco: "go", tier: "redownload", how: How::ToolOrEmpty("go.exe", &["clean", "-modcache"]), busy: &["\\go.exe", "\\gopls.exe"] },
    CacheDef { id: "nugethttp", eco: "dotnet", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "nugetpkgs", eco: "dotnet", tier: "redownload", how: How::Empty, busy: &["\\msbuild.exe", "\\devenv.exe"] },
    CacheDef { id: "gradle", eco: "java", tier: "redownload", how: How::Empty, busy: &["gradle"] },
    CacheDef { id: "maven", eco: "java", tier: "redownload", how: How::Empty, busy: &["plexus-classworlds", "\\maven"] },
];

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn cache_paths(id: &str) -> Vec<PathBuf> {
    let local = env_dir("LOCALAPPDATA");
    let home = env_dir("USERPROFILE");
    let l = |rel: &str| local.as_ref().map(|b| b.join(rel));
    let h = |rel: &str| home.as_ref().map(|b| b.join(rel));
    let npm = env_dir("npm_config_cache").or_else(|| l("npm-cache"));
    let cargo = env_dir("CARGO_HOME").or_else(|| h(".cargo"));
    let v: Vec<Option<PathBuf>> = match id {
        "npm" => vec![npm.map(|p| p.join("_cacache"))],
        "npx" => vec![npm.map(|p| p.join("_npx"))],
        "pnpm" => vec![l("pnpm\\store")],
        "yarn" => vec![l("Yarn\\Cache"), l("Yarn\\Berry\\cache")],
        "bun" => vec![env_dir("BUN_INSTALL_CACHE_DIR").or_else(|| h(".bun\\install\\cache"))],
        "nodegyp" => vec![l("node-gyp\\Cache")],
        "electron" => vec![env_dir("ELECTRON_CACHE").or_else(|| l("electron\\Cache")), l("electron-builder\\Cache")],
        "tsserver" => vec![l("Microsoft\\TypeScript")],
        "playwright" => vec![env_dir("PLAYWRIGHT_BROWSERS_PATH").filter(|p| p.is_absolute()).or_else(|| l("ms-playwright"))],
        "puppeteer" => vec![env_dir("PUPPETEER_CACHE_DIR").or_else(|| h(".cache\\puppeteer"))],
        "pip" => vec![env_dir("PIP_CACHE_DIR").or_else(|| l("pip\\Cache"))],
        "uv" => vec![env_dir("UV_CACHE_DIR").or_else(|| l("uv\\cache"))],
        "hf" => vec![env_dir("HF_HUB_CACHE").or_else(|| env_dir("HF_HOME").map(|p| p.join("hub"))).or_else(|| h(".cache\\huggingface\\hub"))],
        "cargo" => vec![cargo.map(|c| c.join("registry\\cache"))],
        "cargosrc" => vec![cargo.as_ref().map(|c| c.join("registry\\src")), cargo.map(|c| c.join("git\\checkouts"))],
        "gobuild" => vec![env_dir("GOCACHE").or_else(|| l("go-build"))],
        "gomod" => vec![env_dir("GOMODCACHE").or_else(|| env_dir("GOPATH").map(|p| p.join("pkg\\mod"))).or_else(|| h("go\\pkg\\mod"))],
        "nugethttp" => vec![env_dir("NUGET_HTTP_CACHE_PATH").or_else(|| l("NuGet\\v3-cache")), l("NuGet\\plugins-cache")],
        "nugetpkgs" => vec![env_dir("NUGET_PACKAGES").or_else(|| h(".nuget\\packages"))],
        "gradle" => vec![env_dir("GRADLE_USER_HOME").or_else(|| h(".gradle")).map(|p| p.join("caches"))],
        "maven" => vec![h(".m2\\repository")],
        _ => vec![],
    };
    // a cache path must never be a drive root or the profile itself
    v.into_iter().flatten().filter(|p| p.is_absolute() && p.components().count() > 3 && p.is_dir()).collect()
}

/// "1.2.3" / "1148" -> comparable numbers
fn version_key(s: &str) -> Vec<u64> {
    s.split(['.', '_']).map(|x| x.parse().unwrap_or(0)).collect()
}

/// Folders that hold an older version of a browser that also exists in a newer version.
fn old_versions(id: &str, root: &Path) -> Vec<PathBuf> {
    // returns (group, version, path) for each version folder
    let mut entries: Vec<(String, Vec<u64>, PathBuf)> = vec![];
    let dirs = |p: &Path| -> Vec<(String, PathBuf)> {
        fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .filter(|e| e.metadata().is_ok_and(|m| m.is_dir() && !is_reparse(&m)))
                    .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
                    .filter(|(n, _)| !n.starts_with('.'))
                    .collect()
            })
            .unwrap_or_default()
    };
    if id == "playwright" {
        // chromium-1148, chromium_headless_shell-1148, firefox-1466, ffmpeg-1010
        for (n, p) in dirs(root) {
            if let Some((g, v)) = n.rsplit_once('-') {
                if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) {
                    entries.push((g.to_string(), version_key(v), p));
                }
            }
        }
    } else {
        // <browser>\<platform>-<version>, e.g. chrome\win64-131.0.6778.85
        for (b, bp) in dirs(root) {
            for (n, p) in dirs(&bp) {
                if let Some((plat, v)) = n.split_once('-') {
                    entries.push((format!("{b}/{plat}"), version_key(v), p));
                }
            }
        }
    }
    let mut old = vec![];
    for (g, v, p) in &entries {
        if entries.iter().any(|(g2, v2, _)| g2 == g && v2 > v) {
            old.push(p.clone());
        }
    }
    old
}

/// The folders whose content a cleanup would remove (for KeepNewest: the old version folders).
fn targets_of(def: &CacheDef) -> Vec<PathBuf> {
    let paths = cache_paths(def.id);
    if def.how == How::KeepNewest {
        paths.iter().flat_map(|p| old_versions(def.id, p)).collect()
    } else {
        paths
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct CacheView {
    pub id: &'static str,
    pub eco: &'static str,
    pub tier: &'static str,
    /// what a cleanup would free (for Playwright / Puppeteer: the older versions only)
    pub bytes: u64,
    /// whole size on disk, shown when it differs from `bytes`
    pub total: u64,
    pub disabled: bool,
    pub busy: bool,
    /// the tool command used instead of deleting, e.g. "pnpm store prune"
    pub tool: String,
    pub keep_newest: bool,
    pub paths: Vec<String>,
}

fn tool_label(how: How) -> String {
    match how {
        How::Tool(exe, args) | How::ToolOrEmpty(exe, args) => {
            let args: Vec<&str> = args.iter().copied().filter(|a| !a.starts_with('/')).collect();
            if exe == "cmd.exe" { args.join(" ") } else { format!("{} {}", exe.trim_end_matches(".exe"), args.join(" ")) }
        }
        _ => String::new(),
    }
}

fn cache_busy(def: &CacheDef, hay: &[String]) -> bool {
    def.busy.iter().any(|m| hay.iter().any(|h| h.contains(m)))
}

fn scan_caches(cfg: &Config, hay: &[String]) -> Vec<CacheView> {
    let defs: Vec<&CacheDef> = CACHES.iter().collect();
    let mut v: Vec<CacheView> = par_map(&defs, |def| {
        let paths = cache_paths(def.id);
        if paths.is_empty() {
            return None;
        }
        let total: u64 = paths.iter().map(|p| local_size(p)).sum();
        let bytes = if def.how == How::KeepNewest { targets_of(def).iter().map(|p| local_size(p)).sum() } else { total };
        Some(CacheView {
            id: def.id,
            eco: def.eco,
            tier: def.tier,
            bytes,
            total,
            disabled: cfg.disabled.iter().any(|d| d == def.id),
            busy: cache_busy(def, hay),
            tool: tool_label(def.how),
            keep_newest: def.how == How::KeepNewest,
            paths: paths.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        })
    })
    .into_iter()
    .flatten()
    .filter(|c| c.total > 0)
    .collect();
    v.sort_by(|a, b| a.tier.cmp(b.tier).reverse().then(b.bytes.cmp(&a.bytes)));
    v
}

// ---------- public API ----------

#[derive(Serialize, Debug, Default)]
pub struct DiskReport {
    pub caches: Vec<CacheView>,
    pub projects: Vec<Project>,
    pub roots: Vec<String>,
    pub home: String,
    pub cache_total: u64,
    pub project_total: u64,
    pub stale_total: u64,
    pub scan_ms: u64,
}

pub fn report(cfg: &Config) -> DiskReport {
    let t0 = Instant::now();
    let hay = process_hay();
    let (caches, projects) = std::thread::scope(|s| {
        let c = s.spawn(|| scan_caches(cfg, &hay));
        let p = scan_projects(cfg, &hay);
        (c.join().unwrap_or_default(), p)
    });
    let sum = |p: &Project| p.items.iter().map(|i| i.bytes).sum::<u64>();
    DiskReport {
        cache_total: caches.iter().filter(|c| !c.disabled).map(|c| c.bytes).sum(),
        project_total: projects.iter().map(sum).sum(),
        stale_total: projects.iter().filter(|p| p.stale).map(sum).sum(),
        roots: expanded_roots(cfg).iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        home: std::env::var("USERPROFILE").unwrap_or_default(),
        caches,
        projects,
        scan_ms: t0.elapsed().as_millis() as u64,
    }
}

#[derive(Deserialize, Debug)]
pub struct DiskTarget {
    /// "cache" | "project"
    pub kind: String,
    /// cache id, or the project folder
    pub id: String,
    /// project only: the artifact folder name inside it
    #[serde(default)]
    pub name: String,
}

#[derive(Serialize, Debug, Default)]
pub struct DiskCleanResult {
    pub freed: u64,
    pub cleaned: Vec<String>,
    pub skipped: Vec<String>,
    pub locked_files: u64,
}

fn plain_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['\\', '/', ':'])
}

/// Re-check a project target from scratch; returns the artifact folder to delete.
fn verify_project(cfg: &Config, ex: &Excludes, hay: &[String], t: &DiskTarget) -> Result<PathBuf, String> {
    let dir = PathBuf::from(&t.id);
    if !dir.is_absolute() || dir.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err("not a plain absolute path".into());
    }
    if !plain_segment(&t.name) {
        return Err("bad folder name".into());
    }
    let l = lower(&dir);
    let root = expanded_roots(cfg).into_iter().find(|r| within(&l, &lower(r))).ok_or("not inside a scan folder")?;
    if ex.any_on_path(&root, &dir.join(&t.name)) {
        return Err("excluded".into());
    }
    // no junctions or links between the root and the target
    let mut cur = dir.join(&t.name);
    loop {
        let m = fs::symlink_metadata(&cur).map_err(|_| "no longer exists")?;
        if is_reparse(&m) || !m.is_dir() {
            return Err("is a link or not a folder".into());
        }
        if lower(&cur) == lower(&root) {
            break;
        }
        cur = cur.parent().ok_or("outside the scan folder")?.to_path_buf();
    }
    if detect(&dir, &t.name).is_none() {
        return Err("no longer looks like a build or dependency folder".into());
    }
    if path_in_use(hay, &dir) {
        return Err("in use by a running process".into());
    }
    Ok(dir.join(&t.name))
}

/// Rename first (instant, and the project never sees a half-deleted folder), then delete.
/// If files are locked, the renamed folder stays and shows up as a leftover in the next scan.
fn delete_artifact(p: &Path) -> Result<(u64, u64), String> {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = if name.contains(LEFTOVER) {
        p.to_path_buf()
    } else {
        let tmp = p.with_file_name(format!("{name}{LEFTOVER}{}", now()));
        fs::rename(p, &tmp).map_err(|e| format!("a file in it is open ({e})"))?;
        tmp
    };
    let (freed, left) = empty_dir(&target);
    if left == 0 {
        let _ = fs::remove_dir(&target);
    }
    Ok((freed, left))
}

fn run_tool(exe: &str, args: &[&str]) -> Result<(), String> {
    match util::run(exe, args) {
        Ok(o) if o.ok => Ok(()),
        Ok(o) => Err(if o.stderr.trim().is_empty() { o.stdout } else { o.stderr }.trim().chars().take(200).collect()),
        Err(e) => Err(e.to_string()),
    }
}

fn clean_cache(def: &CacheDef) -> Result<(u64, u64), String> {
    let paths = cache_paths(def.id);
    let before: u64 = paths.iter().map(|p| local_size(p)).sum();
    let measured = |left| Ok((before.saturating_sub(paths.iter().map(|p| local_size(p)).sum()), left));
    match def.how {
        How::Empty => {
            let left = paths.iter().map(|p| empty_dir(p).1).sum();
            measured(left)
        }
        How::KeepNewest => {
            let mut left = 0;
            for p in targets_of(def) {
                let (_, l) = empty_dir(&p);
                left += l;
                if l == 0 {
                    let _ = fs::remove_dir(&p);
                }
            }
            measured(left)
        }
        How::Tool(exe, args) => {
            run_tool(exe, args).map_err(|e| format!("{} failed: {e}", tool_label(def.how)))?;
            measured(0)
        }
        How::ToolOrEmpty(exe, args) => {
            if run_tool(exe, args).is_err() {
                let left = paths.iter().map(|p| empty_dir(p).1).sum();
                return measured(left);
            }
            measured(0)
        }
    }
}

pub fn clean(cfg: &Config, targets: &[DiskTarget]) -> DiskCleanResult {
    let hay = process_hay();
    let ex = Excludes::new(&cfg.excludes);
    let mut r = DiskCleanResult::default();
    for t in targets {
        if t.kind == "cache" {
            let Some(def) = CACHES.iter().find(|d| d.id == t.id) else { continue };
            if cfg.disabled.iter().any(|d| d == def.id) {
                r.skipped.push(format!("{}: never clean is on", def.id));
                continue;
            }
            if cache_busy(def, &hay) {
                r.skipped.push(format!("{}: in use by a running process", def.id));
                continue;
            }
            match clean_cache(def) {
                Ok((freed, left)) => {
                    r.freed += freed;
                    r.locked_files += left;
                    r.cleaned.push(format!("cache {} ({:.1} MB)", def.id, freed as f64 / 1048576.0));
                }
                Err(e) => r.skipped.push(format!("{}: {e}", def.id)),
            }
        } else {
            let label = format!("{}\\{}", t.id, t.name);
            match verify_project(cfg, &ex, &hay, t).and_then(|p| delete_artifact(&p)) {
                Ok((freed, left)) => {
                    r.freed += freed;
                    r.locked_files += left;
                    r.cleaned.push(format!("{label} ({:.1} MB)", freed as f64 / 1048576.0));
                }
                Err(e) => r.skipped.push(format!("{label}: {e}")),
            }
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        // not %TEMP%: that is under AppData, which the built-in exclusions skip
        let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("rigsweep-dd-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn globbing() {
        assert!(glob("vendor", "vendor"));
        assert!(!glob("vendor", "vendors"));
        assert!(glob("legacy-*", "legacy-app"));
        assert!(glob("*-old", "site-old"));
        assert!(glob("a*b*c", "axxbyyc"));
        assert!(!glob("a*b*c", "axxbyy"));
    }

    #[test]
    fn within_is_segment_aware() {
        assert!(within("c:\\a\\b", "c:\\a"));
        assert!(within("c:\\a", "c:\\a"));
        assert!(!within("c:\\ab", "c:\\a"));
    }

    #[test]
    fn needs_a_manifest() {
        let root = tmp("detect");
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::create_dir_all(root.join("target").join("debug")).unwrap();
        fs::create_dir_all(root.join("build").join("tmp")).unwrap();
        assert_eq!(detect(&root, "node_modules"), None);
        assert_eq!(detect(&root, "target"), None);
        assert_eq!(detect(&root, "build"), None);
        fs::write(root.join("package.json"), "{}").unwrap();
        fs::write(root.join("Cargo.toml"), "").unwrap();
        assert_eq!(detect(&root, "node_modules"), Some(("node", "deps")));
        assert_eq!(detect(&root, "target"), Some(("rust", "build")));
        assert_eq!(detect(&root, "build"), None); // no gradle file
        fs::create_dir_all(root.join(".venv")).unwrap();
        assert_eq!(detect(&root, ".venv"), None);
        fs::write(root.join(".venv").join("pyvenv.cfg"), "").unwrap();
        assert_eq!(detect(&root, ".venv"), Some(("python", "deps")));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn excludes() {
        let ex = Excludes::new(&["legacy-*".into(), "D:\\keep".into(), "clients\\acme".into()]);
        assert!(ex.dir(Path::new("C:\\x\\legacy-app")));
        assert!(ex.dir(Path::new("d:\\keep\\sub")));
        assert!(!ex.dir(Path::new("d:\\keeper")));
        assert!(ex.dir(Path::new("C:\\work\\clients\\acme")));
        assert!(ex.dir(Path::new("C:\\x\\.git")));
        assert!(ex.any_on_path(Path::new("C:\\x"), Path::new("C:\\x\\legacy-app\\web\\node_modules")));
        assert!(!ex.any_on_path(Path::new("C:\\x"), Path::new("C:\\x\\app\\node_modules")));
    }

    #[test]
    fn finds_and_deletes_with_verification() {
        let root = tmp("scan");
        let app = root.join("app");
        fs::create_dir_all(app.join("node_modules").join("left-pad")).unwrap();
        fs::write(app.join("package.json"), "{}").unwrap();
        fs::write(app.join("node_modules").join("left-pad").join("index.js"), vec![0u8; 300]).unwrap();
        // nested node_modules is never reported separately
        fs::create_dir_all(app.join("node_modules").join("x").join("node_modules")).unwrap();
        // a `target` without Cargo.toml is not ours
        fs::create_dir_all(root.join("game").join("target").join("debug")).unwrap();
        // excluded project
        let skip = root.join("legacy-site");
        fs::create_dir_all(skip.join("node_modules")).unwrap();
        fs::write(skip.join("package.json"), "{}").unwrap();
        fs::write(skip.join("node_modules").join("a.js"), b"x").unwrap();

        let cfg = Config {
            roots: vec![root.to_string_lossy().into_owned()],
            excludes: vec!["legacy-*".into()],
            disabled: vec![],
            stale_days: 30,
        };
        let projects = scan_projects(&cfg, &[]);
        assert_eq!(projects.len(), 1, "{projects:?}");
        assert_eq!(projects[0].items.len(), 1);
        assert_eq!(projects[0].items[0].bytes, 300);
        assert!(!projects[0].stale); // just written

        // a target outside the roots or excluded is refused
        let bad = DiskTarget { kind: "project".into(), id: skip.to_string_lossy().into_owned(), name: "node_modules".into() };
        let up = DiskTarget { kind: "project".into(), id: app.to_string_lossy().into_owned(), name: "..".into() };
        let r = clean(&cfg, &[bad, up]);
        assert_eq!(r.cleaned.len(), 0);
        assert_eq!(r.skipped.len(), 2);
        assert!(skip.join("node_modules").is_dir());

        let ok = DiskTarget { kind: "project".into(), id: app.to_string_lossy().into_owned(), name: "node_modules".into() };
        let r = clean(&cfg, &[ok]);
        assert_eq!(r.cleaned.len(), 1, "{r:?}");
        assert_eq!(r.freed, 300);
        assert!(!app.join("node_modules").exists());
        assert!(app.join("package.json").exists());
        assert_eq!(fs::read_dir(&app).unwrap().count(), 1); // no leftover folder
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn keeps_newest_browser() {
        let root = tmp("pw");
        for d in ["chromium-1140", "chromium-1148", "firefox-1466", "ffmpeg-1010", ".links"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        let old = old_versions("playwright", &root);
        assert_eq!(old, vec![root.join("chromium-1140")]);
        let pp = tmp("pp");
        for d in ["chrome\\win64-131.0.6778.85", "chrome\\win64-121.0.6167.85", "chrome-headless-shell\\win64-121.0.6167.85"] {
            fs::create_dir_all(pp.join(d)).unwrap();
        }
        assert_eq!(old_versions("puppeteer", &pp), vec![pp.join("chrome\\win64-121.0.6167.85")]);
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&pp).unwrap();
    }
}

#[cfg(test)]
mod live {
    /// Read-only: print what the scan finds on this machine. cargo test live_disk -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_disk() {
        let cfg = super::Config { roots: super::default_roots(), excludes: vec![], disabled: super::default_disabled(), stale_days: 30 };
        let r = super::report(&cfg);
        println!("scan {} ms, roots {:?}", r.scan_ms, r.roots);
        for c in &r.caches {
            println!("cache {:<11} {:>8} MB (total {} MB) off={} busy={} {}", c.id, c.bytes / 1048576, c.total / 1048576, c.disabled, c.busy, c.tool);
        }
        for p in &r.projects {
            let items: Vec<String> = p.items.iter().map(|i| format!("{}={}MB{}", i.name, i.bytes / 1048576, if i.suggest { "*" } else { "" })).collect();
            println!("{:>4}d busy={} od={} {} {}", p.idle_days, p.busy, p.onedrive, p.path, items.join(" "));
        }
        println!("caches {} MB, projects {} MB, stale {} MB", r.cache_total / 1048576, r.project_total / 1048576, r.stale_total / 1048576);
    }
}
