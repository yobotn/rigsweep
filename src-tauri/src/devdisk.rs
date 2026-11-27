//! Dev Disk Cleanup: reclaim disk space that dev tools leave behind, in two parts.
//!
//! - Projects: build output and dependency folders (node_modules, .next, target, bin/obj, .venv…)
//!   found under the scan roots. A folder only counts when the file that proves what it is sits
//!   next to it (package.json, Cargo.toml, *.csproj, pyvenv.cfg…), so a random `build` or `target`
//!   folder is never touched. Only projects nobody has edited for a while are suggested.
//! - Package caches: npm, pnpm, Yarn, Bun, pip, uv, Cargo, Go, NuGet, Gradle, Maven, Playwright,
//!   Puppeteer, Hugging Face. uv and Go are cleaned with their own commands (uv cache prune,
//!   go clean), which are stopped after a time limit. The pnpm store is emptied directly:
//!   `pnpm store prune` can hang for a long time without saying why, and projects keep working
//!   without the store because their node_modules files are hard links to the same data. Each
//!   cache can be switched off ("never clean"); the expensive-to-download ones start switched off.
//!
//! Hard links: a file that has other links (pnpm's node_modules and store) frees no space when
//! one name is deleted, so it is left out of "can free" (for the pnpm store) and of "freed".
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
use std::sync::atomic::{AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

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

#[derive(Clone)]
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

/// Number of hard links (names) the file has; 1 when it can't be read.
fn link_count(p: &Path) -> u32 {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    let w: Vec<u16> = p.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let Ok(h) = CreateFileW(
            PCWSTR(w.as_ptr()),
            FILE_READ_ATTRIBUTES.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        ) else {
            return 1;
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        let n = if GetFileInformationByHandle(h, &mut info).is_ok() { info.nNumberOfLinks.max(1) } else { 1 };
        let _ = CloseHandle(h);
        n
    }
}

/// Like `local_size`, but only files with no other hard link: what deleting `p` really frees.
fn unshared_size(p: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if is_reparse(&m) => 0,
            Ok(m) if m.is_dir() => unshared_size(&e.path()),
            Ok(m) if attrs(&m) & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) != 0 => 0,
            Ok(_) if link_count(&e.path()) > 1 => 0,
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

/// Lowers the calling thread's CPU and disk priority (Windows background mode), so a scan yields
/// to whatever the user is doing. Undone automatically when the thread ends.
fn background_thread() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN};
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN);
    }
}

/// Run `f` over `items` on a few background-priority threads (disk-bound work; order is kept).
/// Half the cores at most, so a scan never takes over the machine.
fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = (std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4) / 2).clamp(2, 4);
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::with_capacity(items.len()));
    std::thread::scope(|s| {
        for _ in 0..n.min(items.len().max(1)) {
            s.spawn(|| {
                background_thread();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= items.len() {
                        break;
                    }
                    let r = f(&items[i]);
                    out.lock().unwrap_or_else(|e| e.into_inner()).push((i, r));
                }
            });
        }
    });
    let mut v = out.into_inner().unwrap_or_else(|e| e.into_inner());
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
    /// the folder has a `.git` file: a linked worktree (or a submodule, sorted out later)
    wt: bool,
}

type Listing = (Vec<(String, &'static str, &'static str)>, Vec<PathBuf>, bool);

/// List one folder: artifacts in it, the subfolders to walk further, and whether it has a
/// `.git` file (worktrees and submodules have a file where a repo has a folder).
fn list_dir(dir: &Path, ex: &Excludes) -> Listing {
    let (mut arts, mut subs, mut wt) = (vec![], vec![], false);
    let Ok(rd) = fs::read_dir(dir) else { return (arts, subs, wt) };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if m.is_file() && e.file_name().eq_ignore_ascii_case(".git") {
            wt = true;
        }
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
    (arts, subs, wt)
}

fn walk(dir: &Path, depth: usize, ex: &Excludes, out: &mut Vec<Found>) {
    let (arts, subs, wt) = list_dir(dir, ex);
    if !arts.is_empty() || wt {
        out.push(Found { dir: dir.to_path_buf(), arts, wt });
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

fn scan_projects(cfg: &Config, hay: &[String]) -> (Vec<Project>, Vec<PathBuf>) {
    let ex = Excludes::new(&cfg.excludes);
    let mut found: Vec<Found> = vec![];
    // the first two levels are listed here, the rest is walked in parallel
    let mut tasks: Vec<PathBuf> = vec![];
    for root in expanded_roots(cfg) {
        if ex.dir(&root) {
            continue;
        }
        let (arts, subs, wt) = list_dir(&root, &ex);
        if !arts.is_empty() || wt {
            found.push(Found { dir: root.clone(), arts, wt });
        }
        for s in subs {
            let (arts, subs2, wt) = list_dir(&s, &ex);
            if !arts.is_empty() || wt {
                found.push(Found { dir: s.clone(), arts, wt });
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
    let wts: Vec<PathBuf> = found.iter().filter(|f| f.wt).map(|f| f.dir.clone()).collect();
    found.retain(|f| !f.arts.is_empty());

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
    (projects, wts)
}

// ---------- git worktrees ----------
//
// AI agents (Claude Code, Cursor, Codex) and people make linked worktrees: a second checkout of a
// repo, often with its own node_modules or target. They pile up once the task is done. A worktree
// is removed with `git worktree remove` (never by hand), which refuses one with uncommitted or
// untracked files. The branch and its commits stay in the repo.

#[derive(Serialize, Clone, Debug)]
pub struct Worktree {
    pub path: String,
    /// the main checkout of the repo
    pub repo: String,
    pub branch: String,
    /// claude | cursor | codex | windsurf | git
    pub tool: &'static str,
    pub bytes: u64,
    pub last_active: u64,
    pub idle_days: u32,
    pub busy: bool,
    /// uncommitted or untracked files: git won't remove it
    pub dirty: bool,
    /// its repo is gone or moved: git can't remove it, so it can only go to the Recycle Bin
    pub orphan: bool,
    pub stale: bool,
    /// top-level files and folders git ignores that aren't build output (`.env`…): deleted with it
    pub ignored: Vec<String>,
}

/// Where AI tools keep the worktrees they create outside the project (under the profile's
/// tool folders, which the project scan skips).
fn tool_worktree_roots() -> Vec<PathBuf> {
    let Some(home) = env_dir("USERPROFILE") else { return vec![] };
    let mut v: Vec<PathBuf> = [".cursor\\worktrees", ".codex\\worktrees", ".claude\\worktrees", ".windsurf\\worktrees"].iter().map(|r| home.join(r)).collect();
    if let Some(c) = env_dir("CODEX_HOME") {
        v.push(c.join("worktrees"));
    }
    // agents and scripts also put throwaway worktrees in %TEMP%
    if let Some(t) = temp_dir() {
        v.push(t);
    }
    v.into_iter().filter(|p| p.is_dir()).collect()
}

fn temp_dir() -> Option<PathBuf> {
    env_dir("TEMP").or_else(|| env_dir("TMP")).filter(|p| p.is_absolute() && p.components().count() > 3)
}

/// Folders with a `.git` file a few levels below `dir`.
fn find_git_files(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if !m.is_dir() || is_reparse(&m) {
            continue;
        }
        let p = e.path();
        if fs::symlink_metadata(p.join(".git")).is_ok_and(|m| m.is_file()) {
            out.push(p);
        } else if depth > 0 && !is_artifact_name(&e.file_name().to_string_lossy()) {
            find_git_files(&p, depth - 1, out);
        }
    }
}

/// The worktree's own git folder (`<repo>\.git\worktrees\<name>`) from its `.git` file, or None
/// if the file doesn't point into a `worktrees` folder (a submodule does that).
pub(crate) fn worktree_gitdir(wt: &Path) -> Option<PathBuf> {
    let s = fs::read_to_string(wt.join(".git")).ok()?;
    let g = s.lines().find_map(|l| l.trim().strip_prefix("gitdir:"))?.trim().replace('/', "\\");
    let mut p = PathBuf::from(&g);
    if p.is_relative() {
        p = wt.join(p);
    }
    let parent = p.parent()?;
    parent.file_name().is_some_and(|n| n.eq_ignore_ascii_case("worktrees")).then_some(p)
}

/// The main checkout for a worktree git folder: `<repo>\.git\worktrees\x` -> `<repo>`
/// (for a bare repo, the repo folder itself).
fn main_repo(gitdir: &Path) -> Option<PathBuf> {
    let common = gitdir.parent()?.parent()?;
    if common.file_name().is_some_and(|n| n.eq_ignore_ascii_case(".git")) {
        common.parent().map(Path::to_path_buf)
    } else {
        Some(common.to_path_buf())
    }
}

pub(crate) fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    use std::os::windows::process::CommandExt;
    let o = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).into_owned())
}

/// (dirty, ignored non-build entries) from `git status`; None if git can't read it.
fn worktree_status(wt: &Path) -> Option<(bool, Vec<String>)> {
    let s = git_out(wt, &["status", "--porcelain=v1", "--ignored=matching", "--untracked-files=normal"])?;
    let mut dirty = false;
    let mut ignored: Vec<String> = vec![];
    for l in s.lines() {
        if let Some(p) = l.strip_prefix("!! ") {
            let top = p.trim_matches('"').split('/').next().unwrap_or("").to_string();
            if !top.is_empty() && !is_artifact_name(&top) && !matches!(top.as_str(), "dist" | "out" | ".cache" | "coverage" | "next-env.d.ts") && !top.ends_with(".tsbuildinfo") && !ignored.contains(&top) {
                ignored.push(top);
            }
        } else if !l.trim().is_empty() {
            dirty = true;
        }
    }
    ignored.truncate(8);
    Some((dirty, ignored))
}

/// A worktree on a detached HEAD can hold commits no branch points to; removing it would drop the
/// only reference to them. True when HEAD is on a branch, or its commit is in a branch or tag.
fn head_on_a_branch(wt: &Path) -> bool {
    if git_out(wt, &["symbolic-ref", "-q", "HEAD"]).is_some() {
        return true;
    }
    git_out(wt, &["for-each-ref", "--count=1", "--contains", "HEAD", "refs/heads", "refs/remotes", "refs/tags"])
        .is_some_and(|s| !s.trim().is_empty())
}

fn worktree_tool(p: &Path) -> &'static str {
    let l = lower(p);
    for (m, t) in [("\\.claude\\worktrees\\", "claude"), ("\\.cursor\\worktrees\\", "cursor"), ("\\.codex\\worktrees\\", "codex"), ("\\.windsurf\\worktrees\\", "windsurf")] {
        if l.contains(m) {
            return t;
        }
    }
    if temp_dir().is_some_and(|t| within(&l, &lower(&t))) {
        return "temp";
    }
    "git"
}

fn scan_worktrees(cfg: &Config, hay: &[String], mut dirs: Vec<PathBuf>) -> Vec<Worktree> {
    for r in tool_worktree_roots() {
        find_git_files(&r, 3, &mut dirs);
    }
    let mut seen: Vec<String> = vec![];
    dirs.retain(|d| {
        let l = lower(d);
        let new = !seen.contains(&l);
        seen.push(l);
        new
    });
    let stale_secs = cfg.stale_days.max(1) as u64 * 86400;
    let t = now();
    let mut v: Vec<Worktree> = par_map(&dirs, |d| {
        let gitdir = worktree_gitdir(d)?;
        let orphan = !gitdir.is_dir();
        let repo = main_repo(&gitdir).unwrap_or_default();
        let branch = fs::read_to_string(gitdir.join("HEAD"))
            .map(|h| {
                let h = h.trim();
                h.strip_prefix("ref: refs/heads/").map(str::to_string).unwrap_or_else(|| h.chars().take(8).collect())
            })
            .unwrap_or_default();
        let git_time = ["index", "HEAD", "logs\\HEAD"]
            .iter()
            .filter_map(|f| fs::metadata(gitdir.join(f)).and_then(|m| m.modified()).ok().map(secs))
            .max()
            .unwrap_or(0);
        let last = last_active(d).max(git_time);
        let idle = t.saturating_sub(last);
        let (dirty, ignored) = if orphan { (false, vec![]) } else { worktree_status(d).unwrap_or((true, vec![])) };
        let busy = path_in_use(hay, d);
        Some(Worktree {
            path: d.to_string_lossy().into_owned(),
            repo: repo.to_string_lossy().into_owned(),
            branch,
            tool: worktree_tool(d),
            bytes: local_size(d),
            last_active: last,
            idle_days: (idle / 86400) as u32,
            busy,
            dirty,
            orphan,
            stale: idle >= stale_secs && !busy,
            ignored,
        })
    })
    .into_iter()
    .flatten()
    .collect();
    v.sort_by(|a, b| b.stale.cmp(&a.stale).then(b.bytes.cmp(&a.bytes)));
    v
}

/// How a worktree is removed: by git from its repo, or (repo gone) moved to the Recycle Bin.
enum WtRemove {
    Git(PathBuf, PathBuf),
    Recycle(PathBuf),
}

/// Re-check a worktree target from scratch.
fn verify_worktree(cfg: &Config, hay: &[String], t: &DiskTarget) -> Result<WtRemove, String> {
    let dir = PathBuf::from(&t.id);
    if !dir.is_absolute() || dir.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return Err("not a plain absolute path".into());
    }
    let l = lower(&dir);
    let allowed = expanded_roots(cfg).iter().chain(tool_worktree_roots().iter()).any(|r| within(&l, &lower(r)) && l != lower(r));
    if !allowed {
        return Err("not inside a scan folder".into());
    }
    let m = fs::symlink_metadata(&dir).map_err(|_| "no longer exists")?;
    if is_reparse(&m) || !m.is_dir() {
        return Err("is a link or not a folder".into());
    }
    let gitdir = worktree_gitdir(&dir).ok_or("no longer a git worktree")?;
    if path_in_use(hay, &dir) {
        return Err("in use by a running process".into());
    }
    if !gitdir.is_dir() {
        // git can't remove it or tell what is uncommitted: only to the Recycle Bin, and only
        // when the user picked it knowing that (the UI sends name "recycle" after a warning)
        return if t.name == "recycle" { Ok(WtRemove::Recycle(dir)) } else { Err("its repository is gone".into()) };
    }
    let repo = main_repo(&gitdir).filter(|r| r.is_dir()).ok_or("its repository is gone")?;
    if !head_on_a_branch(&dir) {
        return Err("its commits are not on any branch".into());
    }
    match worktree_status(&dir) {
        Some((false, _)) => Ok(WtRemove::Git(repo, dir)),
        Some((true, _)) => Err("has uncommitted or untracked files".into()),
        None => Err("git could not read it".into()),
    }
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
    /// `conda clean --all`, with the conda.exe that owns the package folder
    Conda,
    /// only shown with its size (models, chat history): removed in the app that owns it, never here
    Show,
    /// empty the folders, but keep artifacts the user built and installed locally (`mvn install`
    /// writes `maven-metadata-local.xml` next to them): those can't be downloaded again
    KeepLocal,
    /// delete only what nothing has touched for this many days (temp folders)
    Aged(u64),
    /// the Recycle Bin on every drive, emptied through the shell
    RecycleBin,
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
    CacheDef { id: "pnpm", eco: "node", tier: "redownload", how: How::Empty, busy: &[] },
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
    CacheDef { id: "maven", eco: "java", tier: "redownload", how: How::KeepLocal, busy: &["plexus-classworlds", "\\maven"] },
    // GPU: shader caches the driver rebuilds (games stutter for a few minutes while it does)
    CacheDef { id: "nvshader", eco: "gpu", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "amdshader", eco: "gpu", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "intelshader", eco: "gpu", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "d3dshader", eco: "gpu", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "cudajit", eco: "gpu", tier: "safe", how: How::Empty, busy: &[] },
    // machine learning
    CacheDef { id: "torch", eco: "ml", tier: "redownload", how: How::Empty, busy: &["python"] },
    CacheDef { id: "torchext", eco: "ml", tier: "safe", how: How::Empty, busy: &["python"] },
    CacheDef { id: "triton", eco: "ml", tier: "safe", how: How::Empty, busy: &["python"] },
    CacheDef { id: "conda", eco: "ml", tier: "safe", how: How::Conda, busy: &["\\conda.exe", "\\mamba.exe", "\\micromamba.exe"] },
    CacheDef { id: "ollama", eco: "ml", tier: "shown", how: How::Show, busy: &[] },
    CacheDef { id: "lmstudio", eco: "ml", tier: "shown", how: How::Show, busy: &[] },
    // AI tools and editors: app caches (Electron), plus what they keep that only they should delete
    CacheDef { id: "cursor", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\cursor.exe"] },
    CacheDef { id: "cursorbak", eco: "ai", tier: "backup", how: How::Empty, busy: &["\\cursor.exe"] },
    // Cursor's agent checkpoints are project state ("restore to this point"): only shown
    CacheDef { id: "cursorsnap", eco: "ai", tier: "history", how: How::Show, busy: &[] },
    CacheDef { id: "cursordb", eco: "ai", tier: "history", how: How::Show, busy: &[] },
    CacheDef { id: "vscode", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\code.exe", "\\code - insiders.exe"] },
    CacheDef { id: "windsurf", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\windsurf.exe"] },
    CacheDef { id: "claude", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\anthropicclaude\\", "\\windowsapps\\claude_"] },
    CacheDef { id: "claudehist", eco: "ai", tier: "history", how: How::Show, busy: &[] },
    // the Codex CLI also runs as node.exe …\@openai\codex\bin\codex.js
    CacheDef { id: "codex", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\codex.exe", "@openai\\codex"] },
    CacheDef { id: "codexhist", eco: "ai", tier: "history", how: How::Show, busy: &[] },
    CacheDef { id: "chatgpt", eco: "ai", tier: "safe", how: How::Empty, busy: &["\\chatgpt.exe"] },
    // Windows leftovers: what Disk Cleanup / Storage Sense would clear, plus what they hide
    CacheDef { id: "wintemp", eco: "win", tier: "safe", how: How::Aged(7), busy: &[] },
    CacheDef { id: "systemp", eco: "win", tier: "safe", how: How::Aged(7), busy: &[] },
    // never while Windows is installing an update
    CacheDef { id: "wudownload", eco: "win", tier: "safe", how: How::Empty, busy: &["\\tiworker.exe", "\\trustedinstaller.exe", "\\wusa.exe"] },
    CacheDef { id: "deliveryopt", eco: "win", tier: "safe", how: How::Tool("powershell.exe", &["-NoProfile", "-NonInteractive", "-Command", "Delete-DeliveryOptimizationCache -Force"]), busy: &[] },
    CacheDef { id: "crashdumps", eco: "win", tier: "safe", how: How::Empty, busy: &["\\werfault.exe"] },
    CacheDef { id: "thumbcache", eco: "win", tier: "safe", how: How::Empty, busy: &[] },
    CacheDef { id: "recyclebin", eco: "win", tier: "bin", how: How::RecycleBin, busy: &[] },
    // the previous Windows: Settings > Storage removes it properly (and it is the way back)
    CacheDef { id: "winold", eco: "win", tier: "shown", how: How::Show, busy: &[] },
];

/// The app that owns a shown-only item (chat history, checkpoints). While it is installed only it
/// deletes them; once it is uninstalled nothing can open them any more, so they become a leftover
/// that may go, to the Recycle Bin.
fn owner_app(id: &str) -> Option<&'static str> {
    match id {
        "cursordb" | "cursorsnap" => Some("cursor"),
        _ => None,
    }
}

/// Any sign the app is still on this PC: running, its exe in a usual place, or an uninstall entry
/// named after it. A false "installed" only keeps the data, which is the safe side.
fn app_installed(app: &str, hay: &[String]) -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};
    let (exe, dirs): (&str, &[&str]) = match app {
        "cursor" => ("cursor.exe", &["cursor"]),
        _ => return true,
    };
    if hay.iter().any(|h| h.contains(&format!("\\{exe}"))) {
        return true;
    }
    let local = env_dir("LOCALAPPDATA").map(|l| l.join("Programs"));
    // Scoop: per user, global, or wherever SCOOP / SCOOP_GLOBAL point (it writes no uninstall entry)
    let scoop = [
        env_dir("SCOOP").or_else(|| env_dir("USERPROFILE").map(|h| h.join("scoop"))),
        env_dir("SCOOP_GLOBAL").or_else(|| env_dir("ProgramData").map(|p| p.join("scoop"))),
    ];
    let mut roots: Vec<PathBuf> = [local, env_dir("ProgramW6432"), env_dir("ProgramFiles"), env_dir("ProgramFiles(x86)")].into_iter().flatten().collect();
    roots.extend(scoop.into_iter().flatten().map(|s| s.join("apps")));
    if roots.iter().any(|r| dirs.iter().any(|d| r.join(d).join(exe).is_file() || r.join(d).join("current").join(exe).is_file())) {
        return true;
    }
    let mut read_any = false;
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for view in [KEY_WOW64_64KEY, KEY_WOW64_32KEY] {
            let Ok(un) = winreg::RegKey::predef(hive).open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall", KEY_READ | view) else { continue };
            read_any = true;
            for k in un.enum_keys().flatten() {
                let name: String = un.open_subkey_with_flags(&k, KEY_READ | view).and_then(|s| s.get_value("DisplayName")).unwrap_or_default();
                if name.to_lowercase().starts_with(app) || k.to_lowercase().contains(app) {
                    return true;
                }
            }
        }
    }
    // the uninstall list couldn't be read at all: that proves nothing, so it counts as installed
    !read_any
}

fn is_leftover(def: &CacheDef, hay: &[String]) -> bool {
    def.how == How::Show && owner_app(def.id).is_some_and(|a| !app_installed(a, hay))
}

/// The cache folders of an Electron app (and VS Code-style editors) inside its data folder.
/// Settings, sign-ins, extensions and chat history stay.
const ELECTRON_CACHES: &[&str] = &[
    "Cache", "Code Cache", "GPUCache", "DawnGraphiteCache", "DawnWebGPUCache", "CachedData", "CachedExtensionVSIXs",
    "CachedProfilesData", "logs", "Crashpad\\reports",
];

/// Store (MSIX) apps keep their data in %LOCALAPPDATA%\Packages\<name>_<publisher id>.
fn store_packages(prefix: &str) -> Vec<PathBuf> {
    let Some(base) = env_dir("LOCALAPPDATA").map(|l| l.join("Packages")) else { return vec![] };
    let pre = prefix.to_lowercase();
    fs::read_dir(&base)
        .map(|rd| rd.flatten().filter(|e| e.file_name().to_string_lossy().to_lowercase().starts_with(&pre)).map(|e| e.path()).collect())
        .unwrap_or_default()
}

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
    let roaming = env_dir("APPDATA");
    let r = |rel: &str| roaming.as_ref().map(|b| b.join(rel));
    let low = |rel: &str| h(&format!("AppData\\LocalLow\\{rel}"));
    let electron = |dirs: Vec<Option<PathBuf>>| -> Vec<Option<PathBuf>> {
        dirs.into_iter().flatten().flat_map(|d| ELECTRON_CACHES.iter().map(move |c| Some(d.join(c)))).collect()
    };
    let in_pkg = |prefix: &str, app: &str| -> Vec<Option<PathBuf>> {
        store_packages(prefix).into_iter().map(|p| Some(p.join("LocalCache\\Roaming").join(app))).collect()
    };
    let win = env_dir("SystemRoot").or_else(|| env_dir("windir"));
    let w = |rel: &str| win.as_ref().map(|b| b.join(rel));
    let v: Vec<Option<PathBuf>> = match id {
        "wintemp" => vec![temp_dir()],
        "systemp" => vec![w("Temp")],
        "wudownload" => vec![w("SoftwareDistribution\\Download")],
        "deliveryopt" => vec![w("ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache")],
        "crashdumps" => {
            let pd = env_dir("ProgramData");
            vec![
                l("CrashDumps"), w("Minidump"), w("MEMORY.DMP"), l("Microsoft\\Windows\\WER\\ReportArchive"), l("Microsoft\\Windows\\WER\\ReportQueue"),
                pd.as_ref().map(|p| p.join("Microsoft\\Windows\\WER\\ReportArchive")), pd.map(|p| p.join("Microsoft\\Windows\\WER\\ReportQueue")),
            ]
        }
        // Explorer's thumbnail and icon databases (the ones it has open stay)
        "thumbcache" => l("Microsoft\\Windows\\Explorer")
            .and_then(|d| fs::read_dir(d).ok())
            .map(|rd| {
                rd.flatten()
                    .filter(|e| {
                        let n = e.file_name().to_string_lossy().to_lowercase();
                        (n.starts_with("thumbcache_") || n.starts_with("iconcache_")) && n.ends_with(".db")
                    })
                    .map(|e| Some(e.path()))
                    .collect()
            })
            .unwrap_or_default(),
        "recyclebin" => recycle_roots().into_iter().map(Some).collect(),
        "winold" => std::env::var("SystemDrive").ok().map(|d| PathBuf::from(format!("{d}\\Windows.old"))).into_iter().map(Some).collect(),
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
        // NVIDIA moved DXCache under LocalLow\PerDriverVersion in recent drivers
        "nvshader" => vec![l("NVIDIA\\DXCache"), l("NVIDIA\\GLCache"), low("NVIDIA\\PerDriverVersion\\DXCache"), low("NVIDIA\\PerDriverVersion\\GLCache")],
        "amdshader" => vec![l("AMD\\DxCache"), l("AMD\\DxcCache"), l("AMD\\VkCache"), l("AMD\\GLCache"), low("AMD\\DxCache"), low("AMD\\DxcCache"), low("AMD\\VkCache")],
        "intelshader" => vec![l("Intel\\ShaderCache"), low("Intel\\ShaderCache")],
        "d3dshader" => vec![l("D3DSCache")],
        "cudajit" => vec![env_dir("CUDA_CACHE_PATH").or_else(|| r("NVIDIA\\ComputeCache"))],
        "torch" => vec![env_dir("TORCH_HOME").or_else(|| env_dir("XDG_CACHE_HOME").map(|p| p.join("torch"))).or_else(|| h(".cache\\torch"))],
        "torchext" => vec![env_dir("TORCH_EXTENSIONS_DIR").or_else(|| l("torch_extensions"))],
        "triton" => vec![env_dir("TRITON_CACHE_DIR").or_else(|| h(".triton\\cache"))],
        "conda" => {
            let mut v: Vec<Option<PathBuf>> = std::env::var("CONDA_PKGS_DIRS")
                .map(|s| s.split(';').filter(|x| !x.trim().is_empty()).map(|x| Some(PathBuf::from(x.trim()))).collect())
                .unwrap_or_default();
            v.extend(["miniconda3", "anaconda3", "miniforge3", "mambaforge", ".conda"].iter().map(|b| h(&format!("{b}\\pkgs"))));
            v.extend(["miniconda3", "anaconda3", "miniforge3"].iter().map(|b| l(&format!("{b}\\pkgs"))));
            v
        }
        "ollama" => vec![env_dir("OLLAMA_MODELS").or_else(|| h(".ollama\\models"))],
        "lmstudio" => vec![h(".lmstudio\\models"), h(".cache\\lm-studio\\models")],
        "cursor" => electron(vec![r("Cursor")]),
        "cursorbak" => vec![r("Cursor\\User\\globalStorage\\state.vscdb.backup")],
        "cursorsnap" => vec![r("Cursor\\snapshots")],
        "cursordb" => vec![r("Cursor\\User\\globalStorage\\state.vscdb")],
        "vscode" => electron(vec![r("Code"), r("Code - Insiders")]),
        "windsurf" => electron(vec![r("Windsurf")]),
        "claude" => electron([vec![r("Claude")], in_pkg("Claude_", "Claude")].concat()),
        "claudehist" => vec![h(".claude\\projects"), h(".claude\\file-history")],
        "codex" => {
            let home = env_dir("CODEX_HOME").or_else(|| h(".codex"));
            let mut v: Vec<Option<PathBuf>> = ["cache", ".tmp", "tmp", "log"].iter().map(|d| home.as_ref().map(|c| c.join(d))).collect();
            v.extend(electron(in_pkg("OpenAI.Codex_", "Codex")));
            v.extend(store_packages("OpenAI.Codex_").into_iter().map(|p| Some(p.join("LocalCache\\Local\\Codex\\Logs"))));
            v
        }
        "codexhist" => {
            let home = env_dir("CODEX_HOME").or_else(|| h(".codex"));
            vec![home.as_ref().map(|c| c.join("sessions")), home.map(|c| c.join("archived_sessions"))]
        }
        "chatgpt" => electron(in_pkg("OpenAI.ChatGPT", "ChatGPT")),
        _ => vec![],
    };
    // a cache path must never be a drive root or the profile itself; a few caches are single files
    v.into_iter()
        .flatten()
        .filter(|p| p.is_absolute() && (p.components().count() > 3 || matches!(id, "recyclebin" | "winold")))
        .filter(|p| p.is_dir() || fs::symlink_metadata(p).is_ok_and(|m| m.is_file()))
        .fold(vec![], |mut v: Vec<PathBuf>, p| {
            if !v.iter().any(|x| lower(x) == lower(&p)) {
                v.push(p);
            }
            v
        })
}

/// Size of a cache path: a folder's local size, or a single file's length.
fn path_size(p: &Path) -> u64 {
    match fs::symlink_metadata(p) {
        Ok(m) if m.is_file() => m.len(),
        Ok(_) => local_size(p),
        Err(_) => 0,
    }
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

/// Marks an artifact folder `mvn install` wrote from the user's own build.
const LOCAL_MARKER: &str = "maven-metadata-local.xml";

/// Split a Maven repository into what can go (files and folders with nothing locally built in
/// them) and whether anything locally built was found. A folder holding the marker is kept whole.
fn local_split(d: &Path) -> (Vec<PathBuf>, bool) {
    if d.join(LOCAL_MARKER).exists() {
        return (vec![], true);
    }
    let Ok(rd) = fs::read_dir(d) else { return (vec![], false) };
    let (mut out, mut any) = (vec![], false);
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if is_reparse(&m) {
            continue;
        }
        if m.is_dir() {
            let (sub, has) = local_split(&e.path());
            if has {
                any = true;
                out.extend(sub);
            } else {
                out.push(e.path());
            }
        } else {
            out.push(e.path());
        }
    }
    (out, any)
}

/// The folders whose content a cleanup would remove (for KeepNewest: the old version folders).
fn targets_of(def: &CacheDef) -> Vec<PathBuf> {
    let paths = cache_paths(def.id);
    match def.how {
        How::KeepNewest => paths.iter().flat_map(|p| old_versions(def.id, p)).collect(),
        How::KeepLocal => paths.iter().flat_map(|p| local_split(p).0).collect(),
        How::Aged(days) => paths.iter().flat_map(|p| aged_entries(p, days)).collect(),
        _ => paths,
    }
}

/// Entries of `dir` that nothing has touched for `days` days (a folder: nothing inside it either).
/// Links, git checkouts (agent worktrees are handled on their own) and RigSweep's own files stay;
/// a folder too big to check quickly stays too.
fn aged_entries(dir: &Path, days: u64) -> Vec<PathBuf> {
    let cutoff = SystemTime::now() - Duration::from_secs(days * 86_400);
    let Ok(rd) = fs::read_dir(dir) else { return vec![] };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = fs::symlink_metadata(&p).ok()?;
            let name = e.file_name().to_string_lossy().to_lowercase();
            if is_reparse(&m) || name.starts_with("rigsweep") || name.starts_with(".rigsweep") || (m.is_dir() && p.join(".git").exists()) {
                return None;
            }
            let mut budget = 20_000u32;
            (!touched_since(&p, &m, cutoff, &mut budget)).then_some(p)
        })
        .collect()
}

fn touched_since(p: &Path, m: &fs::Metadata, cutoff: SystemTime, budget: &mut u32) -> bool {
    if m.modified().map_or(true, |t| t > cutoff) || m.created().is_ok_and(|t| t > cutoff) {
        return true;
    }
    if !m.is_dir() {
        return false;
    }
    let Ok(rd) = fs::read_dir(p) else { return true };
    for e in rd.flatten() {
        if *budget == 0 {
            return true;
        }
        *budget -= 1;
        let Ok(cm) = fs::symlink_metadata(e.path()) else { continue };
        if is_reparse(&cm) || touched_since(&e.path(), &cm, cutoff, budget) {
            return true;
        }
    }
    false
}

/// `X:\$Recycle.Bin` on every fixed drive that has one, the system drive first (the space
/// meter counts the whole bin there).
fn recycle_roots() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = sysinfo::Disks::new_with_refreshed_list()
        .map(|d| d.iter().filter(|x| !x.is_removable()).map(|x| x.mount_point().join("$Recycle.Bin")).collect())
        .unwrap_or_default();
    v.retain(|p| p.is_dir());
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_lowercase();
    v.sort_by_key(|p| !p.to_string_lossy().to_lowercase().starts_with(&sys));
    v
}

/// Bytes in the Recycle Bin on all drives.
fn recycle_size() -> u64 {
    use windows::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, ..Default::default() };
    // SAFETY: a null root means every drive; `info` outlives the call
    unsafe { SHQueryRecycleBinW(windows::core::PCWSTR::null(), &mut info) }.map(|_| info.i64Size.max(0) as u64).unwrap_or(0)
}

fn empty_recycle_bin() {
    use windows::Win32::UI::Shell::{SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND};
    // SAFETY: no window, null root = every drive; fails harmlessly when the bin is already empty
    let _ = unsafe { SHEmptyRecycleBinW(None, windows::core::PCWSTR::null(), SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND) };
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
    /// shown with its size only; never cleaned here (models, chat history)
    pub show_only: bool,
    /// a leftover of an uninstalled app: it goes to the Recycle Bin, so nothing is freed until the bin is emptied
    pub recycle: bool,
    /// only what is older than this many days goes (0: everything)
    pub aged_days: u64,
    pub paths: Vec<String>,
}

fn tool_label(how: How) -> String {
    match how {
        How::Tool("powershell.exe", args) => args.last().map(|a| a.to_string()).unwrap_or_default(),
        How::Tool(exe, args) | How::ToolOrEmpty(exe, args) => {
            let args: Vec<&str> = args.iter().copied().filter(|a| !a.starts_with('/')).collect();
            if exe == "cmd.exe" { args.join(" ") } else { format!("{} {}", exe.trim_end_matches(".exe"), args.join(" ")) }
        }
        How::Conda => "conda clean --all".into(),
        _ => String::new(),
    }
}

/// The conda.exe that owns a package folder (`<install>\pkgs` -> `<install>\Scripts\conda.exe`),
/// else one from a usual install place, else whatever is on PATH.
fn conda_exe(pkgs: &[PathBuf]) -> String {
    let home = env_dir("USERPROFILE");
    let local = env_dir("LOCALAPPDATA");
    let mut c: Vec<PathBuf> = pkgs.iter().filter_map(|p| p.parent().map(|i| i.join("Scripts\\conda.exe"))).collect();
    for b in ["miniconda3", "anaconda3", "miniforge3", "mambaforge"] {
        c.extend(home.iter().chain(local.iter()).map(|h| h.join(b).join("Scripts\\conda.exe")));
    }
    c.into_iter().find(|p| p.is_file()).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "conda.exe".into())
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
        let leftover = is_leftover(def, hay);
        let total: u64 = if def.how == How::RecycleBin { recycle_size() } else { paths.iter().map(|p| path_size(p)).sum() };
        let bytes = if def.how == How::RecycleBin {
            total
        } else if matches!(def.how, How::Aged(_)) {
            targets_of(def).iter().map(|p| path_size(p)).sum()
        } else if def.how == How::KeepNewest {
            targets_of(def).iter().map(|p| local_size(p)).sum()
        } else if def.how == How::KeepLocal {
            targets_of(def).iter().map(|p| path_size(p)).sum()
        } else if def.id == "pnpm" || def.how == How::Conda {
            // packages still linked into a node_modules (or a conda env) stay on disk after the store goes
            paths.iter().map(|p| unshared_size(p)).sum()
        } else {
            total
        };
        Some(CacheView {
            id: def.id,
            eco: def.eco,
            tier: if leftover { "leftover" } else { def.tier },
            bytes,
            total,
            disabled: cfg.disabled.iter().any(|d| d == def.id),
            busy: cache_busy(def, hay),
            tool: tool_label(def.how),
            keep_newest: def.how == How::KeepNewest,
            show_only: def.how == How::Show && !leftover,
            recycle: leftover,
            aged_days: if let How::Aged(d) = def.how { d } else { 0 },
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
    pub worktrees: Vec<Worktree>,
    pub roots: Vec<String>,
    pub home: String,
    pub cache_total: u64,
    pub project_total: u64,
    pub stale_total: u64,
    pub worktree_total: u64,
    /// Docker Desktop and WSL virtual disks (compacted, never deleted)
    pub vdisks: Vec<VDisk>,
    pub scan_ms: u64,
    /// set when only one folder was scanned (File Explorer's right-click menu): no caches or virtual disks then
    pub scope: Option<String>,
}

pub fn report(cfg: &Config) -> DiskReport {
    report_in(cfg, None)
}

/// Only the projects and worktrees inside one folder, with the user's exclusions still applied.
/// Package caches and virtual disks live elsewhere, so they are left out.
pub fn report_folder(cfg: &Config, folder: &str) -> Result<DiskReport, String> {
    let p = Path::new(folder);
    if !p.is_absolute() || !p.is_dir() {
        return Err(format!("not a folder: {folder}"));
    }
    let cfg = Config { roots: vec![folder.to_string()], ..cfg.clone() };
    Ok(report_in(&cfg, Some(folder.to_string())))
}

fn report_in(cfg: &Config, scope: Option<String>) -> DiskReport {
    let t0 = Instant::now();
    let hay = process_hay();
    let (caches, (projects, wt_dirs)) = std::thread::scope(|s| {
        let c = s.spawn(|| if scope.is_some() { vec![] } else { scan_caches(cfg, &hay) });
        let p = scan_projects(cfg, &hay);
        (c.join().unwrap_or_default(), p)
    });
    let worktrees = scan_worktrees(cfg, &hay, wt_dirs);
    let sum = |p: &Project| p.items.iter().map(|i| i.bytes).sum::<u64>();
    DiskReport {
        cache_total: caches.iter().filter(|c| !c.disabled && !c.show_only).map(|c| c.bytes).sum(),
        project_total: projects.iter().map(sum).sum(),
        stale_total: projects.iter().filter(|p| p.stale).map(sum).sum(),
        worktree_total: worktrees.iter().map(|w| w.bytes).sum(),
        roots: expanded_roots(cfg).iter().map(|p| p.to_string_lossy().into_owned()).collect(),
        home: std::env::var("USERPROFILE").unwrap_or_default(),
        caches,
        projects,
        worktrees,
        vdisks: if scope.is_some() { vec![] } else { vdisks() },
        scan_ms: t0.elapsed().as_millis() as u64,
        scope,
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DiskTarget {
    /// "cache" | "project" | "worktree"
    pub kind: String,
    /// cache id, the project folder, or the worktree folder
    pub id: String,
    /// project only: the artifact folder name inside it
    #[serde(default)]
    pub name: String,
    /// size from the last scan, only used to show progress
    #[serde(default)]
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct DiskCleanResult {
    pub freed: u64,
    pub cleaned: Vec<String>,
    /// UI keys of the targets that are gone ("c|id" / "p|path|name"), so the list can drop them
    /// right away instead of waiting for a full rescan
    pub done: Vec<String>,
    pub skipped: Vec<String>,
    pub locked_files: u64,
    /// what was freed, by folder or cache type only (for the Impact counters)
    pub impact: Vec<crate::impact::Freed>,
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

// ---------- deleting, with progress, pause and stop ----------

const RUN: u8 = 0;
const PAUSE: u8 = 1;
const STOP: u8 = 2;
/// What the user asked the running cleanup to do.
static CTL: AtomicU8 = AtomicU8::new(RUN);
static FILES: AtomicU64 = AtomicU64::new(0);
static FREED: AtomicU64 = AtomicU64::new(0);
/// The file being deleted right now.
static CURRENT: Mutex<String> = Mutex::new(String::new());

/// Called between files: waits while paused; true means stop now.
fn hold() -> bool {
    loop {
        match CTL.load(Ordering::SeqCst) {
            RUN => return false,
            STOP => return true,
            _ => std::thread::sleep(Duration::from_millis(120)),
        }
    }
}

/// Delete `path` (a file, or a folder with everything in it). Links and junctions are left
/// alone, never followed. Returns true if a stop was requested.
fn wipe_entry(path: &Path, left: &AtomicU64) -> bool {
    let Ok(m) = fs::symlink_metadata(path) else { return false };
    if is_reparse(&m) {
        return false;
    }
    if m.is_dir() {
        if wipe(path, left) {
            return true;
        }
        let _ = fs::remove_dir(path); // fails harmlessly if something stayed
        return false;
    }
    if let Ok(mut c) = CURRENT.lock() {
        c.clear();
        c.push_str(&path.to_string_lossy());
    }
    if m.permissions().readonly() {
        let mut perm = m.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perm.set_readonly(false);
        let _ = fs::set_permissions(path, perm);
    }
    // another hard link keeps the data on disk (pnpm store <-> node_modules)
    let shared = link_count(path) > 1;
    match fs::remove_file(path) {
        Ok(()) => {
            FILES.fetch_add(1, Ordering::Relaxed);
            if !shared {
                FREED.fetch_add(m.len(), Ordering::Relaxed);
            }
        }
        Err(_) => {
            left.fetch_add(1, Ordering::Relaxed);
        }
    }
    false
}

fn wipe(dir: &Path, left: &AtomicU64) -> bool {
    let Ok(rd) = fs::read_dir(dir) else { return false };
    for e in rd.flatten() {
        if hold() || wipe_entry(&e.path(), left) {
            return true;
        }
    }
    false
}

/// Empty `dir` (keeping `dir` itself), its top-level entries spread over a few threads:
/// node_modules holds thousands of small packages. Returns (files left in place, stopped).
fn wipe_par(dir: &Path) -> (u64, bool) {
    let left = AtomicU64::new(0);
    let Ok(rd) = fs::read_dir(dir) else { return (0, false) };
    let entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    let stopped = par_map(&entries, |p| hold() || wipe_entry(p, &left)).into_iter().any(|s| s);
    (left.into_inner(), stopped)
}

/// Rename first (instant, and the project never sees a half-deleted folder); the caller then
/// empties the renamed folder. If files are locked or the cleanup is stopped, the renamed folder
/// stays and shows up as a leftover in the next scan.
fn rename_artifact(p: &Path) -> Result<PathBuf, String> {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if name.contains(LEFTOVER) {
        return Ok(p.to_path_buf());
    }
    let tmp = p.with_file_name(format!("{name}{LEFTOVER}{}", now()));
    fs::rename(p, &tmp).map_err(|e| format!("a file in it is open ({e})"))?;
    Ok(tmp)
}

/// A tool's own cleanup gets this long before it is killed.
const TOOL_LIMIT: Duration = Duration::from_secs(15 * 60);

/// Run a cleanup tool without a console window. It is killed when Stop is pressed or after
/// TOOL_LIMIT, so a tool that hangs can't hold up the whole cleanup.
fn run_tool(exe: &str, args: &[&str]) -> Result<(), String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(exe)
        .args(args)
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("failed to run {exe}: {e}"))?;
    // drain both pipes on threads so a chatty tool can't block on a full pipe
    let drain = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut s = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut s);
            }
            String::from_utf8_lossy(&s).into_owned()
        })
    };
    let out = drain(child.stdout.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|r| Box::new(r) as Box<dyn Read + Send>));
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if CTL.load(Ordering::SeqCst) == STOP => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("stopped".into());
            }
            Ok(None) if start.elapsed() > TOOL_LIMIT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("no result after {} minutes, stopped", TOOL_LIMIT.as_secs() / 60));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    };
    if status.success() {
        return Ok(());
    }
    let (o, e) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
    Err(if e.trim().is_empty() { o } else { e }.trim().chars().take(200).collect())
}

/// Returns (files left, stopped). Bytes are counted in FREED as files go; a tool's own cleanup
/// is measured before and after (and can't be paused or stopped half-way).
fn clean_cache(def: &CacheDef) -> Result<(u64, bool), String> {
    let paths = cache_paths(def.id);
    let empty = |dirs: &[PathBuf], remove: bool| {
        let mut left = 0;
        for p in dirs {
            if fs::symlink_metadata(p).is_ok_and(|m| m.is_file()) {
                // a single-file cache (Cursor's state backup)
                let l = AtomicU64::new(0);
                wipe_entry(p, &l);
                left += l.into_inner();
                if hold() {
                    return (left, true);
                }
                continue;
            }
            let (l, stopped) = wipe_par(p);
            left += l;
            if stopped {
                return (left, true);
            }
            if remove && l == 0 {
                let _ = fs::remove_dir(p);
            }
        }
        (left, false)
    };
    let by_tool = |exe: &str, args: &[&str]| -> Result<(), String> {
        let before: u64 = paths.iter().map(|p| path_size(p)).sum();
        if let Ok(mut c) = CURRENT.lock() {
            *c = tool_label(def.how);
        }
        run_tool(exe, args)?;
        let after: u64 = paths.iter().map(|p| path_size(p)).sum();
        FREED.fetch_add(before.saturating_sub(after), Ordering::Relaxed);
        Ok(())
    };
    match def.how {
        How::Empty => Ok(empty(&paths, false)),
        How::KeepNewest => Ok(empty(&targets_of(def), true)),
        How::KeepLocal | How::Aged(_) => {
            let left = AtomicU64::new(0);
            let stopped = par_map(&targets_of(def), |p| hold() || wipe_entry(p, &left)).into_iter().any(|s| s);
            Ok((left.into_inner(), stopped))
        }
        How::Tool(exe, args) => {
            by_tool(exe, args).map_err(|e| format!("{} failed: {e}", tool_label(def.how)))?;
            Ok((0, false))
        }
        How::ToolOrEmpty(exe, args) => match by_tool(exe, args) {
            Ok(()) => Ok((0, false)),
            Err(_) => Ok(empty(&paths, false)),
        },
        // conda's own cleanup knows which packages an environment still uses; never by hand
        How::Conda => {
            let exe = conda_exe(&paths);
            by_tool(&exe, &["clean", "--all", "--yes"]).map_err(|e| format!("conda clean failed: {e}"))?;
            Ok((0, false))
        }
        How::RecycleBin => {
            let before = recycle_size();
            empty_recycle_bin();
            FREED.fetch_add(before.saturating_sub(recycle_size()), Ordering::Relaxed);
            Ok((0, false))
        }
        // checked again now: the app may have been installed since the scan
        How::Show if is_leftover(def, &process_hay()) => {
            for p in paths.iter().filter(|p| p.exists()) {
                if hold() {
                    return Ok((0, true));
                }
                if let Ok(mut c) = CURRENT.lock() {
                    *c = format!("Recycle Bin: {}", p.display());
                }
                let size = path_size(p);
                crate::util::recycle(p).map_err(|e| e.to_string())?;
                FREED.fetch_add(size, Ordering::Relaxed);
            }
            Ok((0, false))
        }
        How::Show => Err("only shown here; remove it in the app that owns it".into()),
    }
}

/// Clean `targets` in order. `pending` holds what is still to do (the current target first) and
/// is handed to `save` whenever it changes, so a stopped or crashed cleanup can be continued.
/// Returns the result and what is left (empty unless stopped).
fn run(cfg: &Config, targets: Vec<DiskTarget>, save: &dyn Fn(&[DiskTarget])) -> (DiskCleanResult, Vec<DiskTarget>) {
    let hay = process_hay();
    let ex = Excludes::new(&cfg.excludes);
    let mut r = DiskCleanResult::default();
    let mut pending = targets.clone();
    for (i, t) in targets.into_iter().enumerate() {
        if hold() {
            break;
        }
        let label = match t.kind.as_str() {
            "cache" => format!("cache {}", t.id),
            "worktree" => format!("worktree {}", t.id),
            _ => format!("{}\\{}", t.id, t.name),
        };
        set_job(|j| {
            j.index = i;
            j.target = label.clone();
        });
        let before = FREED.load(Ordering::Relaxed);
        let mut stopped = false;
        if t.kind == "cache" {
            match CACHES.iter().find(|d| d.id == t.id) {
                None => {}
                Some(def) if cfg.disabled.iter().any(|d| d == def.id) => r.skipped.push(format!("{}: never clean is on", def.id)),
                Some(def) if cache_busy(def, &hay) => r.skipped.push(format!("{}: in use by a running process", def.id)),
                Some(def) => match clean_cache(def) {
                    Ok((left, st)) => {
                        r.locked_files += left;
                        stopped = st;
                        if !st {
                            let freed = FREED.load(Ordering::Relaxed) - before;
                            r.cleaned.push(format!("cache {} ({:.1} MB)", def.id, freed as f64 / 1048576.0));
                            r.done.push(format!("c|{}", def.id));
                        }
                    }
                    Err(e) => r.skipped.push(format!("{}: {e}", def.id)),
                },
            }
        } else if t.kind == "worktree" {
            match verify_worktree(cfg, &hay, &t) {
                Ok(WtRemove::Recycle(dir)) => {
                    if let Ok(mut c) = CURRENT.lock() {
                        *c = format!("Recycle Bin: {}", dir.display());
                    }
                    let size = local_size(&dir);
                    match crate::util::recycle(&dir) {
                        Ok(()) => {
                            // it still takes space in the Recycle Bin until that is emptied
                            FREED.fetch_add(size, Ordering::Relaxed);
                            r.cleaned.push(format!("worktree {} to the Recycle Bin ({:.1} MB)", t.id, size as f64 / 1048576.0));
                            r.done.push(format!("w|{}", t.id));
                        }
                        Err(e) => r.skipped.push(format!("worktree {}: {e}", t.id)),
                    }
                }
                Ok(WtRemove::Git(repo, dir)) => {
                    if let Ok(mut c) = CURRENT.lock() {
                        *c = format!("git worktree remove {}", dir.display());
                    }
                    let size = local_size(&dir);
                    let (repo_s, dir_s) = (repo.to_string_lossy().into_owned(), dir.to_string_lossy().into_owned());
                    // no --force: git refuses a worktree with changes, which is what we want
                    let res = run_tool("git", &["-C", &repo_s, "worktree", "remove", &dir_s]);
                    let left = if dir.exists() { local_size(&dir) } else { 0 };
                    FREED.fetch_add(size.saturating_sub(left), Ordering::Relaxed);
                    match res {
                        Ok(()) => {
                            let freed = FREED.load(Ordering::Relaxed) - before;
                            r.cleaned.push(format!("worktree {} ({:.1} MB)", t.id, freed as f64 / 1048576.0));
                            r.done.push(format!("w|{}", t.id));
                        }
                        Err(e) => r.skipped.push(format!("worktree {}: {e}", t.id)),
                    }
                }
                Err(e) => r.skipped.push(format!("worktree {}: {e}", t.id)),
            }
        } else {
            match verify_project(cfg, &ex, &hay, &t).and_then(|p| rename_artifact(&p)) {
                Ok(dir) => {
                    // from here on the folder has its leftover name: that is what a resume deletes
                    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                    if name != t.name {
                        pending[0].name = name;
                        save(&pending);
                    }
                    let (left, st) = wipe_par(&dir);
                    r.locked_files += left;
                    stopped = st;
                    if !st {
                        if left == 0 {
                            let _ = fs::remove_dir(&dir);
                        }
                        let freed = FREED.load(Ordering::Relaxed) - before;
                        r.cleaned.push(format!("{label} ({:.1} MB)", freed as f64 / 1048576.0));
                        r.done.push(format!("p|{}|{}", t.id, t.name));
                    }
                }
                Err(e) => r.skipped.push(format!("{label}: {e}")),
            }
        }
        let got = FREED.load(Ordering::Relaxed) - before;
        if got > 0 {
            let kind = match t.kind.as_str() {
                "cache" => CACHES.iter().find(|d| d.id == t.id).map(|d| format!("cache:{}", d.id)),
                "worktree" => Some("worktree".into()),
                _ => Some(crate::impact::dir_kind(&t.name, LEFTOVER)),
            };
            if let Some(kind) = kind {
                r.impact.push(crate::impact::Freed::new("disk", kind, got, 1));
            }
        }
        if stopped {
            pending[0].bytes = pending[0].bytes.saturating_sub(got);
            break;
        }
        pending.remove(0);
        save(&pending);
    }
    r.freed = FREED.load(Ordering::Relaxed);
    (r, pending)
}

// ---------- the cleanup job: runs in the background, the UI polls it ----------

struct Job {
    id: u64,
    running: bool,
    stopped: bool,
    total: u64,
    index: usize,
    count: usize,
    target: String,
    result: Option<DiskCleanResult>,
}

static JOB: Mutex<Option<Job>> = Mutex::new(None);

fn set_job(f: impl FnOnce(&mut Job)) {
    if let Ok(mut g) = JOB.lock() {
        if let Some(j) = g.as_mut() {
            f(j);
        }
    }
}

#[derive(Serialize, Debug, Default)]
pub struct JobView {
    pub id: u64,
    /// idle | running | paused | stopping | stopped | done
    pub state: &'static str,
    /// bytes the selected items had in the last scan
    pub total: u64,
    pub freed: u64,
    pub files: u64,
    pub index: usize,
    pub count: usize,
    pub target: String,
    pub current: String,
    /// once finished
    pub result: Option<DiskCleanResult>,
    /// what "Continue" would clean (filled by the caller from the saved list)
    pub resume: Vec<DiskTarget>,
}

pub fn job_view() -> JobView {
    let g = JOB.lock().unwrap_or_else(|e| e.into_inner());
    let Some(j) = g.as_ref() else { return JobView { state: "idle", ..Default::default() } };
    let state = if j.running {
        match CTL.load(Ordering::SeqCst) {
            STOP => "stopping",
            PAUSE => "paused",
            _ => "running",
        }
    } else if j.stopped {
        "stopped"
    } else {
        "done"
    };
    JobView {
        id: j.id,
        state,
        total: j.total,
        freed: FREED.load(Ordering::Relaxed),
        files: FILES.load(Ordering::Relaxed),
        index: j.index,
        count: j.count,
        target: j.target.clone(),
        current: if j.running { CURRENT.lock().map(|c| c.clone()).unwrap_or_default() } else { String::new() },
        result: j.result.clone(),
        resume: vec![],
    }
}

pub fn is_running() -> bool {
    JOB.lock().map(|g| g.as_ref().is_some_and(|j| j.running)).unwrap_or(false)
}

/// Start cleaning in the background. `save` keeps the to-do list (see `run`); `finish` gets the
/// result once the job ends, stopped or not.
pub fn start(
    cfg: Config,
    targets: Vec<DiskTarget>,
    save: impl Fn(&[DiskTarget]) + Send + 'static,
    finish: impl FnOnce(&DiskCleanResult) + Send + 'static,
) -> Result<(), String> {
    let mut g = JOB.lock().map_err(|_| "job lock poisoned".to_string())?;
    if g.as_ref().is_some_and(|j| j.running) {
        return Err("a cleanup is already running".into());
    }
    CTL.store(RUN, Ordering::SeqCst);
    FILES.store(0, Ordering::Relaxed);
    FREED.store(0, Ordering::Relaxed);
    if let Ok(mut c) = CURRENT.lock() {
        c.clear();
    }
    *g = Some(Job {
        id: g.as_ref().map_or(1, |j| j.id + 1),
        running: true,
        stopped: false,
        total: targets.iter().map(|t| t.bytes).sum(),
        index: 0,
        count: targets.len(),
        target: String::new(),
        result: None,
    });
    drop(g);
    save(&targets);
    std::thread::spawn(move || {
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&cfg, targets, &save)));
        let (r, left) = out.unwrap_or_else(|_| {
            let r = DiskCleanResult { freed: FREED.load(Ordering::Relaxed), skipped: vec!["the cleanup failed unexpectedly".into()], ..Default::default() };
            (r, vec![])
        });
        finish(&r);
        set_job(|j| {
            j.running = false;
            j.stopped = !left.is_empty();
            j.result = Some(r);
        });
    });
    Ok(())
}

/// "pause" | "resume" | "stop"
pub fn control(action: &str) {
    if !is_running() {
        return;
    }
    match action {
        "pause" => {
            let _ = CTL.compare_exchange(RUN, PAUSE, Ordering::SeqCst, Ordering::SeqCst);
        }
        "resume" => {
            let _ = CTL.compare_exchange(PAUSE, RUN, Ordering::SeqCst, Ordering::SeqCst);
        }
        "stop" => CTL.store(STOP, Ordering::SeqCst),
        _ => {}
    }
}

/// Forget a finished job (the panel closes). A running one is kept.
pub fn dismiss() {
    if let Ok(mut g) = JOB.lock() {
        if !g.as_ref().is_some_and(|j| j.running) {
            *g = None;
        }
    }
}

/// The whole cleanup, in this thread (tests).
#[cfg(test)]
pub fn clean(cfg: &Config, targets: &[DiskTarget]) -> DiskCleanResult {
    // the counters are global: tests that clean take turns
    static ONE: Mutex<()> = Mutex::new(());
    let _g = ONE.lock().unwrap_or_else(|e| e.into_inner());
    CTL.store(RUN, Ordering::SeqCst);
    FILES.store(0, Ordering::Relaxed);
    FREED.store(0, Ordering::Relaxed);
    run(cfg, targets.to_vec(), &|_| {}).0
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
    fn report_folder_scans_only_that_folder() {
        let root = tmp("scope");
        let app = root.join("web");
        fs::create_dir_all(app.join("node_modules").join("a")).unwrap();
        fs::write(app.join("package.json"), "{}").unwrap();
        fs::write(app.join("node_modules").join("a").join("i.js"), vec![0u8; 100]).unwrap();
        let cfg = Config { roots: vec![], excludes: vec![], disabled: vec![], stale_days: 30 };
        // the project folder itself, as when it is right-clicked
        let r = report_folder(&cfg, &app.to_string_lossy()).unwrap();
        assert_eq!(r.projects.len(), 1, "{:?}", r.projects);
        assert!(r.caches.is_empty() && r.vdisks.is_empty());
        assert_eq!(r.scope.as_deref(), Some(&*app.to_string_lossy()));
        // the folder above it
        assert_eq!(report_folder(&cfg, &root.to_string_lossy()).unwrap().projects.len(), 1);
        // the user's exclusions still apply
        let ex = Config { excludes: vec!["web".into()], ..cfg.clone() };
        assert!(report_folder(&ex, &root.to_string_lossy()).unwrap().projects.is_empty());
        assert!(report_folder(&cfg, "relative").is_err());
        assert!(report_folder(&cfg, &root.join("missing").to_string_lossy()).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn leftovers_only_from_shown_items_of_an_uninstalled_app() {
        // only shown-only items have an owner app; caches never take the Recycle Bin route
        for d in CACHES {
            if owner_app(d.id).is_some() {
                assert!(d.how == How::Show, "{} has an owner but is not shown-only", d.id);
            }
        }
        // a running Cursor always counts as installed, whatever the disk and registry say
        let running = vec![r"c:\x\|c:\users\a\appdata\local\programs\cursor\cursor.exe|cursor.exe".to_string()];
        assert!(app_installed("cursor", &running));
        let db = CACHES.iter().find(|d| d.id == "cursordb").unwrap();
        assert!(!is_leftover(db, &running));
        // an app RigSweep knows nothing about is always treated as installed
        assert!(app_installed("nope", &[]));
        let npm = CACHES.iter().find(|d| d.id == "npm").unwrap();
        assert!(!is_leftover(npm, &[]));
    }

    #[test]
    fn maven_keeps_locally_installed() {
        let root = tmp("m2");
        let mine = root.join(r"com\me\app\1.0-SNAPSHOT");
        let theirs = root.join(r"org\lib\core\2.1");
        fs::create_dir_all(&mine).unwrap();
        fs::create_dir_all(&theirs).unwrap();
        fs::write(root.join(r"com\me\app").join(LOCAL_MARKER), "x").unwrap();
        fs::write(mine.join("app.jar"), "x").unwrap();
        fs::write(theirs.join("core.jar"), "x").unwrap();
        fs::create_dir_all(root.join(r"com\other")).unwrap();
        let (go, any) = local_split(&root);
        assert!(any);
        let go: Vec<String> = go.iter().map(|p| lower(p)).collect();
        // the org tree goes whole; com\me\app stays; its sibling com\other goes
        assert!(go.contains(&lower(&root.join("org"))));
        assert!(go.contains(&lower(&root.join(r"com\other"))));
        assert!(!go.iter().any(|p| p.contains(r"\me")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn worktree_gitdir_and_repo() {
        let root = tmp("wtparse");
        let wt = root.join("wt");
        fs::create_dir_all(root.join("repo\\.git\\worktrees\\wt")).unwrap();
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}/repo/.git/worktrees/wt\n", root.to_string_lossy().replace('\\', "/"))).unwrap();
        let g = worktree_gitdir(&wt).unwrap();
        assert_eq!(lower(&main_repo(&g).unwrap()), lower(&root.join("repo")));
        // a submodule's .git file points into .git\modules: not a worktree
        fs::write(wt.join(".git"), "gitdir: ../repo/.git/modules/lib\n").unwrap();
        assert!(worktree_gitdir(&wt).is_none());
        let _ = fs::remove_dir_all(&root);
    }

    fn git(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git").arg("-C").arg(dir).args(args).output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn worktree_found_checked_and_removed() {
        let root = tmp("wtlive");
        let repo = root.join("repo");
        fs::create_dir_all(&repo).unwrap();
        if !git(&repo, &["init", "-q"]) {
            return; // no git on this machine
        }
        fs::write(repo.join("a.txt"), "a").unwrap();
        fs::write(repo.join(".gitignore"), ".env\nnode_modules/\n").unwrap();
        assert!(git(&repo, &["add", "."]));
        assert!(git(&repo, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "init"]));
        let wt = root.join("wt-feature");
        assert!(git(&repo, &["worktree", "add", "-q", "-b", "feature", &wt.to_string_lossy()]));
        fs::write(wt.join(".env"), "SECRET=1").unwrap();
        fs::create_dir_all(wt.join("node_modules\\x")).unwrap();

        let cfg = Config { roots: vec![root.to_string_lossy().into_owned()], excludes: vec![], disabled: vec![], stale_days: 30 };
        let (_, dirs) = scan_projects(&cfg, &[]);
        // the tool folders and %TEMP% on this machine may hold real worktrees: only look at ours
        let mine = lower(&root);
        let wts: Vec<Worktree> = scan_worktrees(&cfg, &[], dirs).into_iter().filter(|w| within(&lower(Path::new(&w.path)), &mine)).collect();
        assert_eq!(wts.len(), 1, "{wts:?}");
        let w = &wts[0];
        assert_eq!(w.branch, "feature");
        assert_eq!(lower(Path::new(&w.repo)), lower(&repo));
        assert!(!w.dirty && !w.orphan);
        assert_eq!(w.ignored, vec![".env".to_string()]); // node_modules is build output, not listed

        // untracked work: git refuses, so do we
        fs::write(wt.join("new.txt"), "work").unwrap();
        let t = DiskTarget { kind: "worktree".into(), id: wt.to_string_lossy().into_owned(), name: String::new(), bytes: 0 };
        let r = clean(&cfg, std::slice::from_ref(&t));
        assert!(r.done.is_empty() && wt.exists(), "{r:?}");

        fs::remove_file(wt.join("new.txt")).unwrap();
        let r = clean(&cfg, &[t]);
        assert_eq!(r.done, vec![format!("w|{}", wt.to_string_lossy())], "{r:?}");
        assert!(!wt.exists());
        assert!(git(&repo, &["rev-parse", "--verify", "-q", "feature"])); // the branch stays
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn single_file_cache_path_size() {
        let root = tmp("filecache");
        let f = root.join("state.vscdb.backup");
        fs::write(&f, vec![0u8; 1234]).unwrap();
        assert_eq!(path_size(&f), 1234);
        assert_eq!(path_size(&root), 1234);
        let _ = fs::remove_dir_all(&root);
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

    /// Tests that run a cleanup share the global pause/stop switch.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn stop_then_continue() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let root = tmp("stop");
        let app = root.join("app");
        fs::create_dir_all(app.join("node_modules").join("a")).unwrap();
        fs::write(app.join("package.json"), "{}").unwrap();
        fs::write(app.join("node_modules").join("a").join("i.js"), vec![0u8; 100]).unwrap();
        let cfg = Config { roots: vec![root.to_string_lossy().into_owned()], excludes: vec![], disabled: vec![], stale_days: 30 };
        let t = DiskTarget { kind: "project".into(), id: app.to_string_lossy().into_owned(), name: "node_modules".into(), bytes: 100 };

        // stopped before it starts: nothing touched, everything still to do
        CTL.store(STOP, Ordering::SeqCst);
        let (r, left) = run(&cfg, vec![t.clone()], &|_| {});
        assert!(r.cleaned.is_empty());
        assert_eq!(left.len(), 1);
        assert!(app.join("node_modules").is_dir());

        // a run cut short after the rename continues on the renamed folder
        let renamed = rename_artifact(&app.join("node_modules")).unwrap();
        let resume = DiskTarget { name: renamed.file_name().unwrap().to_string_lossy().into_owned(), ..t };
        let saved = Mutex::new(vec![]);
        CTL.store(RUN, Ordering::SeqCst);
        let (r, left) = run(&cfg, vec![resume], &|p| *saved.lock().unwrap() = p.to_vec());
        assert_eq!(r.cleaned.len(), 1, "{r:?}");
        assert!(left.is_empty() && saved.lock().unwrap().is_empty());
        assert_eq!(fs::read_dir(&app).unwrap().count(), 1); // only package.json
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn finds_and_deletes_with_verification() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
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
        let (projects, _) = scan_projects(&cfg, &[]);
        assert_eq!(projects.len(), 1, "{projects:?}");
        assert_eq!(projects[0].items.len(), 1);
        assert_eq!(projects[0].items[0].bytes, 300);
        assert!(!projects[0].stale); // just written

        // a target outside the roots or excluded is refused
        let bad = DiskTarget { kind: "project".into(), id: skip.to_string_lossy().into_owned(), name: "node_modules".into(), bytes: 0 };
        let up = DiskTarget { kind: "project".into(), id: app.to_string_lossy().into_owned(), name: "..".into(), bytes: 0 };
        let r = clean(&cfg, &[bad, up]);
        assert_eq!(r.cleaned.len(), 0);
        assert_eq!(r.skipped.len(), 2);
        assert!(skip.join("node_modules").is_dir());

        let ok = DiskTarget { kind: "project".into(), id: app.to_string_lossy().into_owned(), name: "node_modules".into(), bytes: 0 };
        let r = clean(&cfg, &[ok]);
        assert_eq!(r.cleaned.len(), 1, "{r:?}");
        assert_eq!(r.freed, 300);
        assert_eq!(r.impact.iter().map(|f| f.bytes).sum::<u64>(), 300); // every byte freed is counted, by type
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
        for w in &r.worktrees {
            println!("worktree {:>6} MB {:>4}d {} dirty={} orphan={} busy={} [{}] {} ignored={:?}", w.bytes / 1048576, w.idle_days, w.tool, w.dirty, w.orphan, w.busy, w.branch, w.path, w.ignored);
        }
        println!("caches {} MB, projects {} MB, stale {} MB, worktrees {} MB", r.cache_total / 1048576, r.project_total / 1048576, r.stale_total / 1048576, r.worktree_total / 1048576);
    }
}

// ---------- unattended cleanup (Settings > Dev Disk) ----------

/// What a weekly / low-disk run empties without asking: only caches that rebuild themselves
/// (tier "safe"), not marked "never clean", not in use, at least 50 MB. Never projects,
/// worktrees, the Recycle Bin or anything that has to be downloaded again.
pub fn auto_targets(r: &DiskReport) -> Vec<DiskTarget> {
    r.caches
        .iter()
        .filter(|c| c.tier == "safe" && !c.disabled && !c.busy && !c.show_only && c.bytes >= 50 << 20)
        .map(|c| DiskTarget { kind: "cache".into(), id: c.id.into(), name: String::new(), bytes: c.bytes })
        .collect()
}

/// Free bytes on the system drive.
pub fn system_free() -> Option<u64> {
    crate::hardware::volumes().ok()?.into_iter().find(|v| v.system).map(|v| v.free)
}

// ---------- Docker Desktop and WSL virtual disks ----------
//
// A .vhdx grows as Linux writes and never shrinks on its own, even after images or files inside
// are deleted. Compacting gives the empty space back to Windows: shut WSL down (and Docker
// Desktop), then let diskpart compact the file. Nothing inside the disk is deleted, except that
// Docker's build cache and dangling images can be pruned first.

#[derive(Serialize, Clone, Debug)]
pub struct VDisk {
    /// "docker" | "wsl"
    pub kind: &'static str,
    /// the WSL distribution, or "Docker Desktop"
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

/// Docker Desktop's own data disks, then every registered WSL distribution's disk.
pub fn vdisks() -> Vec<VDisk> {
    let mut v: Vec<VDisk> = vec![];
    let mut add = |kind: &'static str, name: String, p: PathBuf| {
        let Ok(m) = fs::metadata(&p) else { return };
        // under 1 GB there is nothing worth compacting (Docker's small system disk)
        if !m.is_file() || m.len() < 1 << 30 || v.iter().any(|x| x.path.eq_ignore_ascii_case(&p.to_string_lossy())) {
            return;
        }
        v.push(VDisk { kind, name, path: p.to_string_lossy().into_owned(), bytes: m.len() });
    };
    let mut docker_dirs: Vec<PathBuf> = env_dir("LOCALAPPDATA").map(|l| l.join("Docker\\wsl")).into_iter().collect();
    // a disk image moved in Docker Desktop's settings (Resources > Advanced)
    for f in ["settings-store.json", "settings.json"] {
        let Some(j) = env_dir("APPDATA").and_then(|a| fs::read_to_string(a.join("Docker").join(f)).ok()) else { continue };
        let Ok(j) = serde_json::from_str::<serde_json::Value>(&j) else { continue };
        for k in ["CustomWslDistroDir", "customWslDistroDir", "DataFolder", "dataFolder"] {
            if let Some(d) = j.get(k).and_then(|x| x.as_str()).filter(|d| !d.is_empty()) {
                docker_dirs.push(PathBuf::from(d));
            }
        }
    }
    for d in docker_dirs {
        for sub in ["", "disk", "data", "main", "distro"] {
            let dir = if sub.is_empty() { d.clone() } else { d.join(sub) };
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().to_lowercase().ends_with(".vhdx") {
                    add("docker", "Docker Desktop".into(), e.path());
                }
            }
        }
    }
    let lxss = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Lxss");
    if let Ok(lxss) = lxss {
        for id in lxss.enum_keys().flatten() {
            let Ok(k) = lxss.open_subkey(&id) else { continue };
            let name: String = k.get_value("DistributionName").unwrap_or_default();
            let base: String = k.get_value("BasePath").unwrap_or_default();
            if base.is_empty() {
                continue;
            }
            let file: String = k.get_value("VhdFileName").unwrap_or_else(|_| "ext4.vhdx".into());
            let base = base.trim_start_matches("\\\\?\\");
            let kind = if name.to_lowercase().starts_with("docker-desktop") { "docker" } else { "wsl" };
            add(kind, if kind == "docker" { "Docker Desktop".into() } else { name }, PathBuf::from(base).join(file));
        }
    }
    v.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    v
}

#[derive(Serialize, Debug)]
pub struct Compacted {
    pub before: u64,
    pub after: u64,
    pub pruned: bool,
}

static COMPACTING: Mutex<()> = Mutex::new(());

/// Run a tool hidden, giving up (and killing it) after `secs`.
fn run_for(exe: &str, args: &[&str], secs: u64) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut child = Command::new(exe)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| format!("{exe}: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(secs) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{exe} gave no result after {secs} s"));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(250)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let o = child.wait_with_output().map_err(|e| e.to_string())?;
    let text = |b: &[u8]| String::from_utf8_lossy(b).trim().to_string();
    if o.status.success() {
        Ok(text(&o.stdout))
    } else {
        let e = text(&o.stderr);
        Err(if e.is_empty() { text(&o.stdout) } else { e }.chars().take(300).collect())
    }
}

/// Shrink one virtual disk. Docker Desktop is quit and WSL is shut down first (running
/// containers and Linux shells stop); neither is started again afterwards.
pub fn compact_vdisk(path: &str, prune: bool) -> Result<Compacted, String> {
    let _one = COMPACTING.try_lock().map_err(|_| "another virtual disk is being compacted".to_string())?;
    if is_running() {
        return Err("a cleanup is running; try again when it has finished".into());
    }
    // only a disk the scan found, never an arbitrary path
    let d = vdisks().into_iter().find(|v| v.path.eq_ignore_ascii_case(path)).ok_or("not a known Docker or WSL disk")?;
    let size = || fs::metadata(&d.path).map(|m| m.len()).unwrap_or(0);
    let before = size();
    let mut pruned = false;
    if d.kind == "docker" {
        if prune && run_for("docker", &["info", "--format", "{{.ServerVersion}}"], 20).is_ok() {
            pruned = run_for("docker", &["builder", "prune", "-f"], 900).is_ok();
            pruned |= run_for("docker", &["image", "prune", "-f"], 900).is_ok();
        }
        let _ = run_for("docker", &["desktop", "stop"], 120);
        for exe in ["Docker Desktop.exe", "com.docker.backend.exe", "com.docker.build.exe"] {
            let _ = run_for("taskkill", &["/F", "/IM", exe], 30);
        }
    }
    run_for("wsl.exe", &["--shutdown"], 180).map_err(|e| format!("couldn't shut WSL down: {e}"))?;
    std::thread::sleep(Duration::from_secs(3));
    let script = std::env::temp_dir().join(format!("rigsweep-compact-{}.txt", std::process::id()));
    fs::write(&script, format!("select vdisk file=\"{}\"\r\nattach vdisk readonly\r\ncompact vdisk\r\ndetach vdisk\r\n", d.path))
        .map_err(|e| e.to_string())?;
    let r = run_for("diskpart.exe", &["/s", &script.to_string_lossy()], 3600);
    let _ = fs::remove_file(&script);
    if let Err(e) = r {
        // a failed run can leave the disk attached; detach it so WSL / Docker can start
        let _ = fs::write(&script, format!("select vdisk file=\"{}\"\r\ndetach vdisk\r\n", d.path));
        let _ = run_for("diskpart.exe", &["/s", &script.to_string_lossy()], 120);
        let _ = fs::remove_file(&script);
        return Err(format!("diskpart couldn't compact it (still in use?): {}", e.lines().last().unwrap_or("")));
    }
    Ok(Compacted { before, after: size(), pruned })
}


// ---------- "what's taking space": folder sizes, one level at a time (Find page) ----------

#[derive(Serialize, Clone, Debug)]
pub struct FolderSize {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct FolderSizes {
    pub path: String,
    pub total: u64,
    /// the biggest sub-folders, largest first
    pub items: Vec<FolderSize>,
    /// files directly in this folder, added up
    pub files: u64,
    /// sub-folders not listed (beyond the biggest 40, or empty)
    pub more: usize,
    pub ms: u64,
}

/// Results stay for 10 minutes, so going back up a level is instant.
static SIZES: Mutex<Vec<(String, Instant, FolderSizes)>> = Mutex::new(Vec::new());

/// Size of every sub-folder of `path` (on-disk bytes; links and online-only files count as zero).
/// Read-only. `fresh` skips the 10-minute memory.
pub fn folder_sizes(path: &str, fresh: bool) -> Result<FolderSizes, String> {
    let p = PathBuf::from(path.trim());
    if !p.is_absolute() || !p.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    let key = lower(&p);
    if !fresh {
        if let Ok(c) = SIZES.lock() {
            if let Some((_, _, v)) = c.iter().find(|(k, at, _)| *k == key && at.elapsed() < Duration::from_secs(600)) {
                return Ok(v.clone());
            }
        }
    }
    let t0 = Instant::now();
    let rd = fs::read_dir(&p).map_err(|e| format!("{path}: {e}"))?;
    let (mut dirs, mut files) = (vec![], 0u64);
    for e in rd.flatten() {
        let Ok(m) = e.metadata() else { continue };
        if is_reparse(&m) {
            continue;
        }
        if m.is_dir() {
            dirs.push(e.path());
        } else if attrs(&m) & (ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS) == 0 {
            files += m.len();
        }
    }
    let sizes = par_map(&dirs, |d| local_size(d));
    let mut items: Vec<FolderSize> = dirs
        .iter()
        .zip(sizes)
        .filter(|(_, b)| *b > 0)
        .map(|(d, bytes)| FolderSize { name: d.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), path: d.to_string_lossy().into_owned(), bytes })
        .collect();
    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let total = files + items.iter().map(|i| i.bytes).sum::<u64>();
    let more = dirs.len().saturating_sub(items.len().min(40));
    items.truncate(40);
    let v = FolderSizes { path: p.to_string_lossy().into_owned(), total, items, files, more, ms: t0.elapsed().as_millis() as u64 };
    if let Ok(mut c) = SIZES.lock() {
        c.retain(|(k, at, _)| *k != key && at.elapsed() < Duration::from_secs(600));
        c.push((key, Instant::now(), v.clone()));
    }
    Ok(v)
}

// ---------- move caches off the system drive (Find > Move caches) ----------
//
// Each tool reads its cache folder from an environment variable (Microsoft's Dev Drive guide
// lists the same ones). Moving = robocopy the folder to <drive>:\packages\<name>, then set the
// variable for this user. Only caches: if anything goes wrong, the tool downloads again.
// Not offered: Cargo (CARGO_HOME also holds installed tools on PATH) and the pnpm store (its hard
// links only work on the drive where the projects are).

/// (cache id, environment variable, folder name under <drive>:\packages)
const MOVABLE: &[(&str, &str, &str)] = &[
    ("npm", "npm_config_cache", "npm-cache"),
    ("pip", "PIP_CACHE_DIR", "pip"),
    ("uv", "UV_CACHE_DIR", "uv"),
    ("yarn", "YARN_CACHE_FOLDER", "yarn"),
    ("bun", "BUN_INSTALL_CACHE_DIR", "bun-cache"),
    ("nugetpkgs", "NUGET_PACKAGES", "nuget-packages"),
    ("gomod", "GOMODCACHE", "go-mod"),
    ("gobuild", "GOCACHE", "go-build"),
    ("gradle", "GRADLE_USER_HOME", "gradle"),
    ("playwright", "PLAYWRIGHT_BROWSERS_PATH", "ms-playwright"),
    ("hf", "HF_HUB_CACHE", "huggingface-hub"),
    ("torch", "TORCH_HOME", "torch"),
];

#[derive(Serialize, Clone, Debug)]
pub struct MovableCache {
    pub id: &'static str,
    pub env: &'static str,
    pub from: String,
    pub bytes: u64,
    pub busy: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct MoveTarget {
    pub drive: String,
    pub free: u64,
    /// ReFS: a Dev Drive (or another ReFS volume)
    pub dev_drive: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct MoveInfo {
    pub caches: Vec<MovableCache>,
    pub targets: Vec<MoveTarget>,
    pub system: String,
}

/// The folder a cache id lives in as a whole (npm: the folder holding _cacache; Gradle: .gradle).
fn movable_dir(id: &str) -> Option<PathBuf> {
    let p = cache_paths(id).into_iter().next()?;
    match id {
        "npm" | "gradle" => p.parent().map(Path::to_path_buf),
        _ => Some(p),
    }
}

fn user_env(name: &str) -> Option<String> {
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .ok()
        .and_then(|k| k.get_value::<String, _>(name).ok())
        .filter(|v| !v.is_empty())
}

pub fn move_info() -> MoveInfo {
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let hay = process_hay();
    let caches = MOVABLE
        .iter()
        .filter(|(_, env, _)| user_env(env).is_none() && std::env::var_os(env).is_none())
        .filter_map(|(id, env, _)| {
            let dir = movable_dir(id)?;
            if !dir.to_string_lossy().to_uppercase().starts_with(&sys) {
                return None;
            }
            let bytes = local_size(&dir);
            let def = CACHES.iter().find(|d| d.id == *id)?;
            (bytes >= 100 << 20).then(|| MovableCache { id, env, from: dir.to_string_lossy().into_owned(), bytes, busy: cache_busy(def, &hay) })
        })
        .collect();
    let refs: Vec<String> = sysinfo::Disks::new_with_refreshed_list()
        .map(|d| {
            d.iter()
                .filter(|x| x.file_system().to_string_lossy().eq_ignore_ascii_case("refs"))
                .map(|x| x.mount_point().to_string_lossy().to_uppercase().chars().take(2).collect())
                .collect()
        })
        .unwrap_or_default();
    let targets = crate::hardware::volumes()
        .unwrap_or_default()
        .into_iter()
        .filter(|v| !v.system && v.free > 5 << 30)
        .map(|v| MoveTarget { dev_drive: refs.contains(&v.drive), drive: v.drive, free: v.free })
        .collect();
    MoveInfo { caches, targets, system: sys }
}

/// Move one cache to `<drive>\packages\<name>` and point its tool there. Returns the new folder.
pub fn move_cache(id: &str, drive: &str) -> Result<String, String> {
    let info = move_info();
    let c = info.caches.iter().find(|c| c.id == id).ok_or("this cache can't be moved (already moved, or not on the system drive)")?;
    if c.busy {
        return Err("in use by a running process; close it and try again".into());
    }
    let t = info.targets.iter().find(|t| t.drive.eq_ignore_ascii_case(drive)).ok_or("not a drive it can move to")?;
    if t.free < c.bytes + (2 << 30) {
        return Err(format!("{} doesn't have enough free space", t.drive));
    }
    let (_, env, name) = MOVABLE.iter().find(|m| m.0 == id).ok_or("unknown cache")?;
    let dest = PathBuf::from(format!("{}\\packages\\{name}", t.drive));
    fs::create_dir_all(&dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    let (src, dst) = (c.from.clone(), dest.to_string_lossy().into_owned());
    // robocopy: exit codes below 8 mean success (1 = files copied)
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("robocopy.exe")
        .args([src.as_str(), dst.as_str(), "/E", "/MOVE", "/R:1", "/W:1", "/NFL", "/NDL", "/NJH", "/NJS", "/NP"])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| format!("robocopy: {e}"))?;
    if out.status.code().unwrap_or(16) >= 8 {
        return Err(format!("robocopy couldn't move everything (code {}); nothing was changed for the tool", out.status.code().unwrap_or(-1)));
    }
    let k = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey_with_flags("Environment", winreg::enums::KEY_SET_VALUE).map_err(|e| e.to_string())?;
    k.set_value(env, &dst).map_err(|e| e.to_string())?;
    broadcast_env_change();
    Ok(dst)
}

/// Tell Explorer (and apps started from it later) that the user environment changed.
fn broadcast_env_change() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
    let w: Vec<u16> = "Environment\0".encode_utf16().collect();
    // SAFETY: `w` outlives the call; the result is ignored
    unsafe {
        let _ = SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(w.as_ptr() as isize), SMTO_ABORTIFHUNG, 3000, None);
    }
}
