//! Browser Cleanup: measure and empty browser caches (Chrome, Edge, Brave, Vivaldi, Opera, Firefox),
//! per profile, without touching sign-ins. Cookies, passwords, Local/Session Storage, history and
//! settings are never on the list. Two tiers:
//! - "safe": pure caches the browser rebuilds on its own (pages, images, compiled scripts, GPU
//!   shaders, service-worker caches). No sign-outs, ever.
//! - "site": data sites stored on purpose (legacy File System API, IndexedDB). Can be huge, but a
//!   site may lose offline files, and a few web apps keep their session in IndexedDB.
//!
//! Only folder sizes are read, never site names. The UI sends (browser, profile, key) identifiers
//! and the paths are re-derived here, so nothing outside the known folders can be deleted. A browser
//! that is running is skipped, because it holds its cache files open.

use serde::{Deserialize, Serialize};
use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Clone, Copy, PartialEq)]
enum Engine {
    Chromium,
    Firefox,
}

#[derive(Clone, Copy)]
enum Base {
    Local,
    Roaming,
}

struct BrowserDef {
    id: &'static str,
    name: &'static str,
    engine: Engine,
    base: Base,
    /// Chromium: the "User Data" folder (holds Local State). Firefox: the folder with profiles.ini.
    rel: &'static str,
    exes: &'static [&'static str],
}

const BROWSERS: &[BrowserDef] = &[
    BrowserDef { id: "chrome", name: "Google Chrome", engine: Engine::Chromium, base: Base::Local, rel: r"Google\Chrome\User Data", exes: &["chrome.exe"] },
    BrowserDef { id: "edge", name: "Microsoft Edge", engine: Engine::Chromium, base: Base::Local, rel: r"Microsoft\Edge\User Data", exes: &["msedge.exe"] },
    BrowserDef { id: "brave", name: "Brave", engine: Engine::Chromium, base: Base::Local, rel: r"BraveSoftware\Brave-Browser\User Data", exes: &["brave.exe"] },
    BrowserDef { id: "vivaldi", name: "Vivaldi", engine: Engine::Chromium, base: Base::Local, rel: r"Vivaldi\User Data", exes: &["vivaldi.exe"] },
    // Opera keeps its profile in Roaming and its caches in a Local twin folder
    BrowserDef { id: "opera", name: "Opera", engine: Engine::Chromium, base: Base::Roaming, rel: r"Opera Software\Opera Stable", exes: &["opera.exe"] },
    BrowserDef { id: "operagx", name: "Opera GX", engine: Engine::Chromium, base: Base::Roaming, rel: r"Opera Software\Opera GX Stable", exes: &["opera.exe"] },
    BrowserDef { id: "firefox", name: "Mozilla Firefox", engine: Engine::Firefox, base: Base::Roaming, rel: r"Mozilla\Firefox", exes: &["firefox.exe"] },
];

/// (key, tier, folders relative to a profile folder)
type ItemDef = (&'static str, &'static str, &'static [&'static str]);

const CHROMIUM_ITEMS: &[ItemDef] = &[
    ("cache", "safe", &["Cache", "Media Cache", "Code Cache", "GPUCache", "DawnGraphiteCache", "DawnWebGPUCache", r"Shared Dictionary\cache"]),
    ("sw", "safe", &[r"Service Worker\CacheStorage", r"Service Worker\ScriptCache"]),
    ("filesystem", "site", &["File System"]),
    ("indexeddb", "site", &["IndexedDB"]),
];
/// Browser-wide GPU shader caches next to Local State.
const CHROMIUM_SHARED: &[ItemDef] = &[("gpu", "safe", &["ShaderCache", "GrShaderCache", "GraphiteDawnCache"])];
/// Firefox keeps these in the Local twin of the profile folder; cookies/logins live in Roaming.
const FIREFOX_ITEMS: &[ItemDef] = &[("cache", "safe", &["cache2", "startupCache", "thumbnails", "jumpListCache"])];

#[derive(Serialize, Clone, Debug)]
pub struct Item {
    pub key: &'static str,
    /// "safe" | "site"
    pub tier: &'static str,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProfileView {
    /// folder name ("Default", "Profile 63"); empty for the browser-wide shared caches
    pub dir: String,
    /// display name from the browser ("fts", "Work"), never the account e-mail
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Clone, Debug)]
pub struct BrowserView {
    pub id: &'static str,
    pub name: &'static str,
    pub running: bool,
    pub profiles: Vec<ProfileView>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct BrowserReport {
    pub browsers: Vec<BrowserView>,
    pub safe_total: u64,
    pub site_total: u64,
}

#[derive(Deserialize, Debug)]
pub struct CleanTarget {
    pub browser: String,
    pub profile: String,
    pub key: String,
}

#[derive(Serialize, Debug, Default)]
pub struct CleanResult {
    pub freed: u64,
    /// "Google Chrome / fts / cache"
    pub cleaned: Vec<String>,
    /// browsers that were open and therefore left alone
    pub skipped_running: Vec<String>,
    /// files that were locked or denied (left in place)
    pub locked_files: u64,
}

// ---------- discovery ----------

struct FoundItem {
    key: &'static str,
    tier: &'static str,
    paths: Vec<PathBuf>,
}

struct FoundProfile {
    dir: String,
    name: String,
    items: Vec<FoundItem>,
}

struct FoundBrowser {
    def: &'static BrowserDef,
    profiles: Vec<FoundProfile>,
}

fn base_dir(b: Base) -> Option<PathBuf> {
    let v = match b {
        Base::Local => "LOCALAPPDATA",
        Base::Roaming => "APPDATA",
    };
    std::env::var_os(v).map(PathBuf::from)
}

fn local_twin(roaming_path: &Path) -> Option<PathBuf> {
    let roaming = base_dir(Base::Roaming)?;
    let rel = roaming_path.strip_prefix(&roaming).ok()?;
    Some(base_dir(Base::Local)?.join(rel))
}

fn items_in(roots: &[PathBuf], defs: &[ItemDef]) -> Vec<FoundItem> {
    defs.iter()
        .map(|(key, tier, rels)| FoundItem {
            key,
            tier,
            paths: roots.iter().flat_map(|r| rels.iter().map(move |x| r.join(x))).filter(|p| p.is_dir()).collect(),
        })
        .filter(|i| !i.paths.is_empty())
        .collect()
}

/// A folder name from Local State / profiles.ini is only accepted as one plain path segment.
fn plain_segment(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && !s.contains(['\\', '/', ':'])
}

fn chromium_profiles(root: &Path) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = fs::read_to_string(root.join("Local State"))
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.pointer("/profile/info_cache").and_then(|c| c.as_object().cloned()))
        .map(|m| {
            m.iter()
                .filter(|(dir, _)| plain_segment(dir))
                .map(|(dir, info)| {
                    let name = info.get("name").and_then(|n| n.as_str()).unwrap_or("").trim().to_string();
                    (dir.clone(), if name.is_empty() { dir.clone() } else { name })
                })
                .collect()
        })
        .unwrap_or_default();
    if out.is_empty() && root.join("Default").is_dir() {
        out.push(("Default".into(), "Default".into()));
    }
    out.sort_by_key(|p| p.1.to_lowercase());
    out
}

fn discover_chromium(def: &'static BrowserDef, root: PathBuf) -> FoundBrowser {
    let mut roots = vec![root.clone()];
    if let Base::Roaming = def.base {
        if let Some(t) = local_twin(&root) {
            if t.is_dir() {
                roots.push(t);
            }
        }
    }
    let mut profiles: Vec<FoundProfile> = chromium_profiles(&root)
        .into_iter()
        .map(|(dir, name)| {
            let proots: Vec<PathBuf> = roots.iter().map(|r| r.join(&dir)).collect();
            FoundProfile { items: items_in(&proots, CHROMIUM_ITEMS), dir, name }
        })
        .collect();
    // older single-profile Opera layout: the caches sit directly in the root
    if profiles.is_empty() {
        profiles.push(FoundProfile { dir: ".".into(), name: def.name.into(), items: items_in(&roots, CHROMIUM_ITEMS) });
    }
    profiles.push(FoundProfile { dir: String::new(), name: String::new(), items: items_in(&roots, CHROMIUM_SHARED) });
    FoundBrowser { def, profiles }
}

/// profiles.ini -> (Name, Path, IsRelative)
fn parse_profiles_ini(s: &str) -> Vec<(String, String, bool)> {
    let mut out = vec![];
    let mut cur: Option<(String, String, bool)> = None;
    for line in s.lines().map(str::trim) {
        if let Some(section) = line.strip_prefix('[') {
            if let Some(c) = cur.take() {
                if !c.1.is_empty() {
                    out.push(c);
                }
            }
            if section.to_ascii_lowercase().starts_with("profile") {
                cur = Some((String::new(), String::new(), true));
            }
        } else if let (Some(c), Some((k, v))) = (cur.as_mut(), line.split_once('=')) {
            match k.trim() {
                "Name" => c.0 = v.trim().into(),
                "Path" => c.1 = v.trim().replace('/', "\\"),
                "IsRelative" => c.2 = v.trim() == "1",
                _ => {}
            }
        }
    }
    if let Some(c) = cur {
        if !c.1.is_empty() {
            out.push(c);
        }
    }
    out
}

fn discover_firefox(def: &'static BrowserDef, root: PathBuf) -> FoundBrowser {
    let ini = fs::read_to_string(root.join("profiles.ini")).unwrap_or_default();
    let local_root = local_twin(&root);
    let mut profiles: Vec<FoundProfile> = parse_profiles_ini(&ini)
        .into_iter()
        .filter_map(|(name, path, rel)| {
            let dir = path.rsplit('\\').next().unwrap_or("").to_string();
            if !plain_segment(&dir) {
                return None;
            }
            let cache_dir = if rel {
                // only relative paths of the form Profiles\<dir>
                if path.split('\\').filter(|s| !s.is_empty()).any(|s| !plain_segment(s)) {
                    return None;
                }
                local_root.as_ref()?.join(&path)
            } else {
                PathBuf::from(&path) // custom-location profile keeps its cache inside
            };
            Some(FoundProfile { items: items_in(&[cache_dir], FIREFOX_ITEMS), name: if name.is_empty() { dir.clone() } else { name }, dir })
        })
        .collect();
    profiles.sort_by_key(|p| p.name.to_lowercase());
    FoundBrowser { def, profiles }
}

fn discover() -> Vec<FoundBrowser> {
    BROWSERS
        .iter()
        .filter_map(|def| {
            let root = base_dir(def.base)?.join(def.rel);
            if !root.is_dir() {
                return None;
            }
            Some(match def.engine {
                Engine::Chromium => discover_chromium(def, root),
                Engine::Firefox => discover_firefox(def, root),
            })
        })
        .map(|mut b| {
            b.profiles.retain(|p| !p.items.is_empty());
            b
        })
        .filter(|b| !b.profiles.is_empty())
        .collect()
}

/// Helper processes: Chromium children get `--type=...`, Firefox children get `-contentproc`.
/// A helper alone (e.g. an orphaned utility process left after the browser closed) does not
/// hold the profile open, so it must not block cleanup.
fn is_helper_cmd(cmd: &[std::ffi::OsString]) -> bool {
    cmd.iter().skip(1).any(|a| {
        let a = a.to_string_lossy();
        a.starts_with("--type=") || a.eq_ignore_ascii_case("-contentproc")
    })
}

/// Playwright / Puppeteer / headless instances use a throwaway profile, not the user's.
fn is_automation(p: &sysinfo::Process) -> bool {
    let hay = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ").to_ascii_lowercase().replace('\\', "/");
    crate::devclean::is_automation_browser(&p.name().to_string_lossy(), &hay)
}

/// Lower-case exe names of running main (non-helper) processes; None if the process list
/// can't be read. A process whose command line can't be read counts as main (fail safe).
fn running_exes() -> Option<Vec<String>> {
    let mut sys = System::new().ok()?;
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always),
    );
    Some(
        sys.processes()
            .values()
            .filter(|p| !is_helper_cmd(p.cmd()) && !is_automation(p))
            .map(|p| p.name().to_string_lossy().to_ascii_lowercase())
            .collect(),
    )
}

/// Unknown process list counts as "running", so nothing is deleted under an open browser.
fn is_running(def: &BrowserDef, running: &Option<Vec<String>>) -> bool {
    running.as_ref().is_none_or(|r| def.exes.iter().any(|e| r.iter().any(|x| x == e)))
}

// ---------- size / delete ----------

fn is_reparse(m: &fs::Metadata) -> bool {
    m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

/// Total size of regular files below `p`, never following links or junctions.
pub fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match fs::symlink_metadata(e.path()) {
            Ok(m) if is_reparse(&m) => 0,
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Delete everything inside `p` (keeping `p` itself). Links/junctions are skipped, not followed.
/// Locked files are left in place and counted. Returns (bytes freed, files left).
pub fn empty_dir(p: &Path) -> (u64, u64) {
    let (mut freed, mut left) = (0, 0);
    let Ok(rd) = fs::read_dir(p) else { return (0, 0) };
    for e in rd.flatten() {
        let path = e.path();
        let Ok(m) = fs::symlink_metadata(&path) else { continue };
        if is_reparse(&m) {
            continue;
        }
        if m.is_dir() {
            let (f, l) = empty_dir(&path);
            freed += f;
            left += l;
            let _ = fs::remove_dir(&path); // fails harmlessly if something stayed
        } else {
            let len = m.len();
            if m.permissions().readonly() {
                let mut perm = m.permissions();
                #[allow(clippy::permissions_set_readonly_false)]
                perm.set_readonly(false);
                let _ = fs::set_permissions(&path, perm);
            }
            match fs::remove_file(&path) {
                Ok(()) => freed += len,
                Err(_) => left += 1,
            }
        }
    }
    (freed, left)
}

// ---------- public API ----------

pub fn report() -> BrowserReport {
    let running = running_exes();
    let mut r = BrowserReport::default();
    for b in discover() {
        let profiles = b
            .profiles
            .iter()
            .map(|p| ProfileView {
                dir: p.dir.clone(),
                name: p.name.clone(),
                items: p
                    .items
                    .iter()
                    .map(|i| {
                        let bytes = i.paths.iter().map(|x| dir_size(x)).sum();
                        if i.tier == "safe" {
                            r.safe_total += bytes;
                        } else {
                            r.site_total += bytes;
                        }
                        Item { key: i.key, tier: i.tier, bytes }
                    })
                    .collect(),
            })
            .collect();
        r.browsers.push(BrowserView { id: b.def.id, name: b.def.name, running: is_running(b.def, &running), profiles });
    }
    r
}

pub fn clean(targets: &[CleanTarget]) -> CleanResult {
    let running = running_exes();
    let found = discover();
    let mut res = CleanResult::default();
    for b in &found {
        let mine: Vec<&CleanTarget> = targets.iter().filter(|t| t.browser == b.def.id).collect();
        if mine.is_empty() {
            continue;
        }
        if is_running(b.def, &running) {
            res.skipped_running.push(b.def.name.to_string());
            continue;
        }
        for p in &b.profiles {
            for i in &p.items {
                if !mine.iter().any(|t| t.profile == p.dir && t.key == i.key) {
                    continue;
                }
                for path in &i.paths {
                    let (f, l) = empty_dir(path);
                    res.freed += f;
                    res.locked_files += l;
                }
                let pname = if p.dir.is_empty() { "shared" } else { p.name.as_str() };
                res.cleaned.push(format!("{} / {} / {}", b.def.name, pname, i.key));
            }
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_profiles_ini() {
        let ini = "[General]\nStartWithLastProfile=1\n\n[Profile1]\nName=work\nIsRelative=1\nPath=Profiles/abc.work\n\n[Profile0]\nName=default-release\nIsRelative=0\nPath=D:\\FF\\x\n\n[Install308046B0AF4A39CB]\nDefault=Profiles/abc.work\n";
        let p = parse_profiles_ini(ini);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0], ("work".into(), r"Profiles\abc.work".into(), true));
        assert_eq!(p[1], ("default-release".into(), r"D:\FF\x".into(), false));
    }

    #[test]
    fn rejects_odd_segments() {
        assert!(plain_segment("Profile 63"));
        assert!(!plain_segment(".."));
        assert!(!plain_segment(r"..\x"));
        assert!(!plain_segment("C:"));
        assert!(!plain_segment(""));
    }

    #[test]
    fn empties_dir_but_keeps_it() {
        let root = std::env::temp_dir().join(format!("rigsweep-bc-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let cache = root.join("Cache");
        fs::create_dir_all(cache.join("Cache_Data").join("deep")).unwrap();
        fs::write(cache.join("index"), vec![0u8; 100]).unwrap();
        fs::write(cache.join("Cache_Data").join("deep").join("f_1"), vec![0u8; 250]).unwrap();
        let keep = root.join("Cookies");
        fs::write(&keep, b"x").unwrap();

        assert_eq!(dir_size(&cache), 350);
        assert_eq!(empty_dir(&cache), (350, 0));
        assert!(cache.is_dir());
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);
        assert!(keep.exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn helper_processes_do_not_count_as_running() {
        let c = |v: &[&str]| v.iter().map(std::ffi::OsString::from).collect::<Vec<_>>();
        assert!(!is_helper_cmd(&c(&["chrome.exe", "--source-shortcut-location=start-menu"])));
        assert!(!is_helper_cmd(&c(&["firefox.exe"])));
        assert!(!is_helper_cmd(&[])); // unreadable command line: treat as main (fail safe)
        assert!(is_helper_cmd(&c(&["chrome.exe", "--type=utility", "--utility-sub-type=chrome.mojom.UtilWin"])));
        assert!(is_helper_cmd(&c(&["firefox.exe", "-contentproc", "-childID", "3"])));
    }
}

#[cfg(test)]
mod live {
    /// Read-only: print what the scan finds on this machine. cargo test live_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_report() {
        let r = super::report();
        for b in &r.browsers {
            println!("{} running={}", b.name, b.running);
            for p in &b.profiles {
                let items: Vec<String> = p.items.iter().map(|i| format!("{}={}MB", i.key, i.bytes / 1048576)).collect();
                println!("   {:<14} [{}] {}", if p.dir.is_empty() { "(shared)" } else { &p.name }, p.dir, items.join(" "));
            }
        }
        println!("safe {} MB, site {} MB", r.safe_total / 1048576, r.site_total / 1048576);
    }
}
