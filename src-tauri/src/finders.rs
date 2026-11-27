//! Finders: programs without an uninstaller, old unchanged files, files worth a look.
//! Every finder only lists what it found for review: nothing is removed without the user's click, files
//! and folders go to the Recycle Bin, and a registry entry gets a JSON backup first. Links and junctions
//! are never followed, and nothing under C:\Windows is ever offered.
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, OsStr};
use std::fs;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS, KEY_READ, KEY_WOW64_64KEY, KEY_WRITE};
use winreg::RegKey;

use crate::util;

const ATTR_REPARSE: u32 = 0x400;
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x40000;
const ATTR_PINNED: u32 = 0x80000;
const ATTR_UNPINNED: u32 = 0x100000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x400000;
/// Reading these would make OneDrive (or another cloud provider) download the file first.
const NOT_LOCAL: u32 = ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS;
const MB: u64 = 1024 * 1024;
const DAY: u64 = 86_400;

type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

// ---------------------------------------------------------------------------------------------------------------
// shared helpers

/// Lower-case, backslashes, no trailing separator: the form every path comparison here uses.
fn lower(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\");
    let s = s.strip_prefix(r"\\?\").unwrap_or(&s);
    s.trim_end_matches('\\').to_lowercase()
}

/// `a` is `b` or below it (segment aware: c:\ab is not below c:\a).
fn within(a: &str, b: &str) -> bool {
    a == b || (a.len() > b.len() && a.starts_with(b) && a.as_bytes()[b.len()] == b'\\')
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn windir() -> String {
    lower(&env_dir("SystemRoot").or_else(|| env_dir("windir")).unwrap_or_else(|| PathBuf::from(r"C:\Windows")))
}

fn unix(t: std::io::Result<SystemTime>) -> u64 {
    t.ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0)
}

fn now() -> u64 {
    unix(Ok(SystemTime::now()))
}

/// Junctions and symbolic links (name-surrogate reparse points). OneDrive files and folders are
/// reparse points too, but of the cloud kind; those are walked, just never read.
fn is_link(m: &fs::Metadata) -> bool {
    m.file_type().is_symlink()
}

fn is_reparse(m: &fs::Metadata) -> bool {
    m.file_attributes() & ATTR_REPARSE != 0
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain([0]).collect()
}

fn ext_of(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn utf16_lossy(b: &[u8]) -> String {
    let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
    String::from_utf16_lossy(&u)
}

/// Text of a file that may be UTF-16 (task XML, .reg) or UTF-8.
fn read_text(p: &Path) -> Option<String> {
    let b = fs::read(p).ok()?;
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        return Some(utf16_lossy(&b[2..]));
    }
    Some(String::from_utf8_lossy(&b).into_owned())
}

/// Running processes as (exe path, command line), both lower-case.
fn processes() -> Vec<(String, String)> {
    let Ok(mut sys) = System::new() else { return vec![] };
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_exe(UpdateKind::Always),
    );
    sys.processes()
        .values()
        .map(|p| {
            let exe = p.exe().map(lower).unwrap_or_default();
            let cmd = p.cmd().iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" ").to_lowercase();
            (exe, cmd)
        })
        .collect()
}

/// Executable and script extensions a command line, shortcut or task may launch.
const LAUNCH_EXTS: &[&str] = &["exe", "com", "scr", "pif", "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "ps1", "hta", "dll", "msi", "lnk"];

/// Split a command line into (program, rest). Unquoted programs may contain spaces
/// (`C:\Program Files\App\u.exe /S`), so the program ends after the first launchable extension
/// that ends a word; without one it ends at the first space.
fn split_command(cmd: &str) -> Option<(String, String)> {
    let c = util::expand_env(cmd.trim());
    let c = c.trim();
    if c.is_empty() {
        return None;
    }
    if let Some(r) = c.strip_prefix('"') {
        let end = r.find('"')?;
        return Some((r[..end].trim().to_string(), r[end + 1..].trim().to_string()));
    }
    // a bare name (RunDll32, MsiExec.exe, cmd) has no spaces in it
    let first = c.find(' ').unwrap_or(c.len());
    if !c[..first].contains(['\\', '/']) {
        return Some((c[..first].to_string(), c[first..].trim().to_string()));
    }
    // ASCII lower-casing keeps byte offsets
    let l = c.to_ascii_lowercase();
    let mut best: Option<usize> = None;
    for ext in LAUNCH_EXTS {
        let pat = format!(".{ext}");
        let mut from = 0;
        while let Some(i) = l[from..].find(&pat) {
            let end = from + i + pat.len();
            if end == l.len() || matches!(l.as_bytes()[end], b' ' | b',' | b'\t' | b'"') {
                best = Some(best.map_or(end, |b| b.min(end)));
                break;
            }
            from = end;
        }
    }
    let end = best.unwrap_or_else(|| c.find(' ').unwrap_or(c.len()));
    Some((c[..end].to_string(), c[end..].trim().to_string()))
}

/// Absolute local or UNC path, or None for bare names like `MsiExec.exe` that Windows looks up itself.
fn absolute(s: &str) -> Option<PathBuf> {
    let s = s.trim().trim_matches('"');
    let s = s.strip_prefix(r"\??\").unwrap_or(s);
    let b = s.as_bytes();
    let drive = b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/');
    if drive || s.starts_with(r"\\") {
        Some(PathBuf::from(s))
    } else {
        None
    }
}

fn base_name(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_ascii_lowercase()
}

/// The file a command line runs: the program itself, or for rundll32 the DLL it loads.
fn command_target(cmd: &str) -> Option<PathBuf> {
    let (prog, rest) = split_command(cmd)?;
    let name = base_name(&prog);
    if name == "rundll32" || name == "rundll32.exe" {
        let r = rest.trim();
        let dll = match r.strip_prefix('"') {
            Some(q) => &q[..q.find('"')?],
            None => r.split(',').next()?.trim(),
        };
        return absolute(dll);
    }
    absolute(&prog)
}

/// Windows' own interpreters: a command that runs one of them says little about the program it belongs to.
const INTERPRETERS: &[&str] = &[
    "msiexec.exe", "cmd.exe", "powershell.exe", "pwsh.exe", "wscript.exe", "cscript.exe", "mshta.exe", "explorer.exe", "conhost.exe",
];

/// The uninstaller file an UninstallString runs, or None when it can't be judged safely: MsiExec
/// (Windows Installer keeps its own records), bare names, interpreters.
fn uninstall_target(cmd: &str) -> Option<PathBuf> {
    let (prog, _) = split_command(cmd)?;
    if base_name(&prog).starts_with("msiexec") {
        return None;
    }
    let t = command_target(cmd)?;
    if INTERPRETERS.contains(&base_name(&t.to_string_lossy()).as_str()) {
        return None;
    }
    Some(t)
}

/// Absolute paths with a launchable extension inside free text (script arguments of an interpreter).
fn paths_in(s: &str) -> Vec<PathBuf> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i + 2 < b.len() {
        let start = b[i].is_ascii_alphabetic() && b[i + 1] == b':' && b[i + 2] == b'\\' && (i == 0 || !b[i - 1].is_ascii_alphanumeric());
        if start && s.is_char_boundary(i) {
            let tail = &s[i..];
            let tail = &tail[..tail.find(['"', '\'', '|', '<', '>']).unwrap_or(tail.len())];
            if let Some((p, _)) = split_command(tail) {
                if LAUNCH_EXTS.contains(&ext_of(Path::new(&p)).as_str()) {
                    out.push(PathBuf::from(&p));
                }
                i += p.len().max(1);
                continue;
            }
        }
        i += 1;
    }
    out
}

/// What a command line starts: its program (or rundll32's DLL) and, for interpreters, the scripts it hands them.
fn command_targets(cmd: &str) -> Vec<PathBuf> {
    let mut out = vec![];
    let Some((prog, rest)) = split_command(cmd) else { return out };
    if let Some(t) = command_target(cmd) {
        out.push(t);
    }
    let name = base_name(&prog);
    if INTERPRETERS.contains(&name.as_str()) || INTERPRETERS.contains(&format!("{name}.exe").as_str()) {
        out.extend(paths_in(&rest));
    }
    out
}

/// Text decoded from a shortcut's raw bytes (ANSI and UTF-16 at both alignments), enough to find the
/// paths it points to without starting COM.
fn lnk_text(p: &Path) -> String {
    let Ok(b) = fs::read(p) else { return String::new() };
    if b.len() > 1_000_000 {
        return String::new();
    }
    let ansi: String = b.iter().map(|&c| if c.is_ascii_graphic() || c == b' ' { c as char } else { '\0' }).collect();
    format!("{ansi}\0{}\0{}", utf16_lossy(&b), utf16_lossy(&b[1..]))
}

/// The registry root for a hive name used in reg_key strings.
fn hive_root(h: &str) -> Option<RegKey> {
    match h.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => Some(RegKey::predef(HKEY_LOCAL_MACHINE)),
        "HKCU" | "HKEY_CURRENT_USER" => Some(RegKey::predef(HKEY_CURRENT_USER)),
        _ => None,
    }
}

fn reg_strings(k: &RegKey) -> Vec<String> {
    k.enum_values()
        .flatten()
        .filter(|(_, v)| matches!(v.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ | RegType::REG_MULTI_SZ))
        .map(|(_, v)| utf16_lossy(&v.bytes).replace('\0', "\n"))
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// 1. programs without an uninstaller

const UNINSTALL: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";
const UNINSTALL_WOW: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall";

#[derive(Serialize, Clone, Debug)]
pub struct OrphanProgram {
    /// dead_entry | leftover_folder
    pub kind: &'static str,
    pub name: String,
    pub publisher: String,
    pub version: String,
    pub install_location: String,
    pub uninstall_string: String,
    /// dead entries: the full key, hive and view included (HKLM\SOFTWARE\WOW6432Node\...\Uninstall\Foo)
    pub reg_key: String,
    /// dead entries: why the uninstaller can't work. uninstaller_missing: its file is gone (missing_file).
    /// web_app_gone: a browser web app whose app is no longer in its profile (missing_file is that folder).
    pub reason: &'static str,
    /// dead entries: the uninstaller file that is gone, or the web app's missing profile folder
    pub missing_file: String,
    /// dead entries: the publisher's EstimatedSize in KB. Removing the entry frees none of it; the files are usually gone already.
    pub estimated_kb: u64,
    /// leftover folders: the folder
    pub path: String,
    /// leftover folders: bytes stored on this disk
    pub bytes: u64,
    /// leftover folders: newest file change, unix seconds (0 = unknown)
    pub modified: u64,
    /// the folder walk stopped at its cap: bytes and modified are lower bounds
    pub partial: bool,
    /// leftover folders: holds a program (.exe) at its top two levels, so it may be a portable app still in use
    pub has_exe: bool,
}

struct UninstEntry {
    hive: String,
    /// path below the hive, entry name included
    key: String,
    wow: bool,
    name: String,
    publisher: String,
    version: String,
    install_location: String,
    uninstall_string: String,
    system_component: bool,
    /// hotfixes and add-ons of another product
    is_update: bool,
    estimated_kb: u64,
    /// every string value, for "does anything still point into this folder"
    strings: Vec<String>,
}

impl UninstEntry {
    fn reg_key(&self) -> String {
        format!("{}\\{}", self.hive, self.key)
    }
}

/// Every uninstall entry: HKLM and HKCU, native and WOW6432Node lists. `other_users` adds the loaded
/// hives of other signed-in users (only used to find references, never to remove).
fn uninstall_entries(other_users: bool) -> Vec<UninstEntry> {
    let mut roots: Vec<(String, RegKey)> =
        vec![("HKLM".into(), RegKey::predef(HKEY_LOCAL_MACHINE)), ("HKCU".into(), RegKey::predef(HKEY_CURRENT_USER))];
    if other_users {
        let hku = RegKey::predef(HKEY_USERS);
        for sid in hku.enum_keys().flatten().filter(|s| !s.ends_with("_Classes")) {
            if let Ok(k) = hku.open_subkey_with_flags(&sid, KEY_READ) {
                roots.push((format!("HKU\\{sid}"), k));
            }
        }
    }
    let mut out = vec![];
    for (hive, root) in &roots {
        for base in [UNINSTALL, UNINSTALL_WOW] {
            let Ok(list) = root.open_subkey_with_flags(base, KEY_READ | KEY_WOW64_64KEY) else { continue };
            for name in list.enum_keys().flatten() {
                let Ok(k) = list.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) else { continue };
                let s = |v: &str| k.get_value::<String, _>(v).unwrap_or_default().trim().to_string();
                let d = |v: &str| k.get_value::<u32, _>(v).unwrap_or(0);
                out.push(UninstEntry {
                    hive: hive.clone(),
                    key: format!("{base}\\{name}"),
                    wow: base == UNINSTALL_WOW,
                    name: s("DisplayName"),
                    publisher: s("Publisher"),
                    version: s("DisplayVersion"),
                    install_location: s("InstallLocation"),
                    uninstall_string: s("UninstallString"),
                    system_component: d("SystemComponent") == 1,
                    is_update: !s("ParentKeyName").is_empty() || !s("ReleaseType").is_empty(),
                    estimated_kb: d("EstimatedSize") as u64,
                    strings: reg_strings(&k),
                });
            }
        }
    }
    out
}

/// Only a plain "not found" counts as missing, and only on a drive that is there: an unplugged USB
/// disk or a lost network share is not proof the program is gone.
fn missing(p: &Path) -> bool {
    let s = p.to_string_lossy();
    if s.starts_with(r"\\") {
        return false;
    }
    let Some(drive) = s.get(..3) else { return false };
    if fs::metadata(drive).is_err() {
        return false;
    }
    matches!(fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// The value of `--name=` in a command line: quoted ("Profile 1") or up to the next space.
fn arg_value(cmd: &str, name: &str) -> Option<String> {
    let i = cmd.to_ascii_lowercase().find(&name.to_ascii_lowercase())?;
    let rest = &cmd[i + name.len()..];
    let v = match rest.strip_prefix('"') {
        Some(q) => &q[..q.find('"')?],
        None => rest.split([' ', '\t']).next()?,
    };
    (!v.is_empty()).then(|| v.to_string())
}

/// Where a Chromium browser keeps its profiles, by the program its uninstall command runs.
fn browser_user_data(exe: &str) -> Option<PathBuf> {
    let local = env_dir("LOCALAPPDATA")?;
    let sub = match exe {
        "chrome.exe" | "chrome_proxy.exe" => r"Google\Chrome\User Data",
        "msedge.exe" | "msedge_proxy.exe" => r"Microsoft\Edge\User Data",
        "brave.exe" | "brave_proxy.exe" => r"BraveSoftware\Brave-Browser\User Data",
        _ => return None,
    };
    Some(local.join(sub))
}

/// A browser web app (Chrome, Edge or Brave "install this site as an app") registers an uninstall entry that
/// runs the browser with --uninstall-app-id. Once the app is gone from its profile that command does nothing,
/// and the entry stays in the installed apps list for good (Chrome Remote Desktop, Outlook PWA...).
/// Returns the profile folder the app is missing from, or None when it is there or can't be judged.
fn dead_web_app(e: &UninstEntry, user_data_of: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    // the browser's profiles are the current user's: entries of the machine or other users can't be judged here
    if e.hive != "HKCU" {
        return None;
    }
    let id = arg_value(&e.uninstall_string, "--uninstall-app-id=")?.to_ascii_lowercase();
    if id.len() != 32 || !id.bytes().all(|b| (b'a'..=b'p').contains(&b)) {
        return None;
    }
    let profile = arg_value(&e.uninstall_string, "--profile-directory=").unwrap_or_else(|| "Default".into());
    if profile.contains(['\\', '/']) || profile.contains("..") {
        return None;
    }
    let (prog, _) = split_command(&e.uninstall_string)?;
    let user_data = user_data_of(&base_name(&prog))?;
    // no Local State: not the folder this browser uses, so nothing can be said
    if !user_data.join("Local State").is_file() {
        return None;
    }
    let prof = user_data.join(&profile);
    if missing(&prof) {
        return Some(prof);
    }
    let apps = prof.join("Web Applications");
    let icons = apps.join("Manifest Resources");
    // a profile without the usual layout (an older or newer browser) is not judged
    if !icons.is_dir() {
        return None;
    }
    if !missing(&icons.join(&id)) || !missing(&apps.join(format!("_crx_{id}"))) {
        return None;
    }
    Some(icons.join(&id))
}

/// Why an entry that can be judged has an uninstaller that can't work, with the file or folder that is gone.
fn dead_reason(e: &UninstEntry, windir: &str) -> Option<(PathBuf, &'static str)> {
    if e.name.is_empty() || e.system_component || e.is_update || e.uninstall_string.is_empty() {
        return None;
    }
    if let Some(p) = dead_web_app(e, &browser_user_data) {
        return Some((p, "web_app_gone"));
    }
    dead_target(e, windir).map(|t| (t, "uninstaller_missing"))
}

/// The missing uninstaller of an entry that can be judged, or None.
fn dead_target(e: &UninstEntry, windir: &str) -> Option<PathBuf> {
    if e.name.is_empty() || e.system_component || e.is_update || e.uninstall_string.is_empty() {
        return None;
    }
    let t = uninstall_target(&e.uninstall_string)?;
    if within(&lower(&t), windir) || !missing(&t) {
        return None;
    }
    // a 32-bit installer's %ProgramFiles% means Program Files (x86); expanded here (64-bit) it doesn't
    if e.wow && e.uninstall_string.contains('%') {
        let alt = e
            .uninstall_string
            .replace("%ProgramFiles%", "%ProgramFiles(x86)%")
            .replace("%CommonProgramFiles%", "%CommonProgramFiles(x86)%");
        if let Some(t2) = uninstall_target(&alt) {
            if !missing(&t2) {
                return None;
            }
        }
    }
    Some(t)
}

fn dead_entries(entries: &[UninstEntry]) -> Vec<OrphanProgram> {
    let wd = windir();
    entries
        .iter()
        .filter(|e| e.hive == "HKLM" || e.hive == "HKCU")
        .filter_map(|e| {
            let (t, reason) = dead_reason(e, &wd)?;
            Some(OrphanProgram {
                kind: "dead_entry",
                name: e.name.clone(),
                publisher: e.publisher.clone(),
                version: e.version.clone(),
                install_location: e.install_location.clone(),
                uninstall_string: e.uninstall_string.clone(),
                reg_key: e.reg_key(),
                reason,
                missing_file: t.to_string_lossy().into_owned(),
                estimated_kb: e.estimated_kb,
                path: String::new(),
                bytes: 0,
                modified: 0,
                partial: false,
                has_exe: false,
            })
        })
        .collect()
}

/// Folders Windows, Microsoft or driver packages own. Driver packages drop tools into Program Files
/// without an uninstall entry of their own, so a missing entry proves nothing there.
const SYSTEM_FOLDERS: &[&str] = &[
    "common files", "windowsapps", "modifiablewindowsapps", "internet explorer", "dotnet", "powershell", "reference assemblies", "msbuild",
    "iis", "iis express", "uninstall information", "installshield installation information", "msecache", "package cache", "windows kits",
    "application verifier", "wsl", "nuget", "common", "rigsweep", "intel", "amd", "ati technologies", "nvidia", "nvidia corporation", "realtek",
    "synaptics", "elan", "dolby", "waves", "conexant", "qualcomm", "mediatek", "dell", "hp", "hewlett-packard", "lenovo", "asus", "acer",
    "msi", "samsung", "huawei", "microsoft.net", "temp", "difx", "packagemanagement", "ruxim", "modules",
];
const SYSTEM_PREFIXES: &[&str] = &["windows", "microsoft", "msbuild", "dotnet"];
/// A folder changed this recently may be mid-install or mid-update.
const LEFTOVER_QUIET_DAYS: u64 = 30;

fn is_system_folder(name: &str) -> bool {
    let n = name.to_lowercase();
    SYSTEM_FOLDERS.contains(&n.as_str()) || SYSTEM_PREFIXES.iter().any(|p| n.starts_with(p)) || n.starts_with('$') || n.starts_with('.')
}

fn leftover_roots() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = vec![];
    let pf = env_dir("ProgramW6432").or_else(|| env_dir("ProgramFiles"));
    let local = env_dir("LOCALAPPDATA").map(|l| l.join("Programs"));
    for p in [pf, env_dir("ProgramFiles(x86)"), local].into_iter().flatten() {
        if !out.iter().any(|o| lower(o) == lower(&p)) {
            out.push(p);
        }
    }
    out
}

fn push_key_values(out: &mut String, root: &RegKey, path: &str, values: &[&str]) {
    if let Ok(k) = root.open_subkey_with_flags(path, KEY_READ | KEY_WOW64_64KEY) {
        for v in values {
            if let Ok(s) = k.get_value::<String, _>(v) {
                out.push_str(&s);
                out.push('\n');
            }
        }
    }
}

/// A path as Windows Installer stores it in a component record: "C?\Program Files\..." and
/// "C:\?Program Files (x86)\..." both mean C:\Program Files... A real path never holds a '?'.
fn msi_path(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b'?' && b[2] == b'\\' {
        return format!("{}:{}", &s[..1], s[2..].replace('?', ""));
    }
    if b.len() > 3 && b[0].is_ascii_alphabetic() && b[1] == b':' {
        return s.replace('?', "");
    }
    s.to_string()
}

/// Top-level keys under SOFTWARE that are not one program's own settings, or are read elsewhere.
const NOT_VENDOR_KEYS: &[&str] = &["classes", "microsoft", "policies", "wow6432node", "wow64aarch64node"];

/// Every string value below `base\<vendor>`, up to `budget` keys and 6 levels.
fn vendor_strings(out: &mut String, root: &RegKey, base: &str, budget: &mut usize) {
    fn walk(out: &mut String, k: &RegKey, depth: usize, budget: &mut usize) {
        for s in reg_strings(k) {
            out.push_str(&s);
            out.push('\n');
        }
        if depth == 0 {
            return;
        }
        for name in k.enum_keys().flatten() {
            if *budget == 0 {
                return;
            }
            *budget -= 1;
            if let Ok(sub) = k.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) {
                walk(out, &sub, depth - 1, budget);
            }
        }
    }
    let Ok(sw) = root.open_subkey_with_flags(base, KEY_READ | KEY_WOW64_64KEY) else { return };
    for name in sw.enum_keys().flatten() {
        if NOT_VENDOR_KEYS.contains(&name.to_ascii_lowercase().as_str()) || *budget == 0 {
            continue;
        }
        if let Ok(k) = sw.open_subkey_with_flags(&name, KEY_READ | KEY_WOW64_64KEY) {
            walk(out, &k, 6, budget);
        }
    }
}

/// Letters and digits only, lower-case: "Remote Setup" and "RemoteSetup" compare equal.
fn name_key(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// A file's CompanyName from its version resource.
fn company_name(p: &Path) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW};
    let w = wide(p.as_os_str());
    unsafe {
        let n = GetFileVersionInfoSizeW(PCWSTR(w.as_ptr()), None);
        if n == 0 || n > 4 * MB as u32 {
            return None;
        }
        let mut buf = vec![0u8; n as usize];
        GetFileVersionInfoW(PCWSTR(w.as_ptr()), None, n, buf.as_mut_ptr() as *mut c_void).ok()?;
        let query = |sub: &str| -> Option<(*const u16, usize)> {
            let s = wide(OsStr::new(sub));
            let mut ptr: *mut c_void = std::ptr::null_mut();
            let mut len = 0u32;
            if !VerQueryValueW(buf.as_ptr() as *const c_void, PCWSTR(s.as_ptr()), &mut ptr, &mut len).as_bool() || ptr.is_null() || len == 0 {
                return None;
            }
            Some((ptr as *const u16, len as usize))
        };
        let mut langs: Vec<String> = vec![];
        // the translation table's length is in bytes: pairs of (language, code page)
        if let Some((ptr, bytes)) = query(r"\VarFileInfo\Translation") {
            for pair in std::slice::from_raw_parts(ptr, bytes / 2).chunks_exact(2) {
                langs.push(format!("{:04x}{:04x}", pair[0], pair[1]));
            }
        }
        langs.extend(["040904b0".to_string(), "040904e4".into(), "04090000".into()]);
        for l in langs {
            // a string value's length is in characters
            if let Some((ptr, chars)) = query(&format!(r"\StringFileInfo\{l}\CompanyName")) {
                let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, chars));
                let s = s.trim_matches(['\0', ' ']).to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
    }
    None
}

/// Who made the program in a folder: the version info, else the signer, of its largest .exe at the top
/// two levels (uninstallers aside).
fn folder_publisher(dir: &Path) -> String {
    let mut exes: Vec<(u64, PathBuf)> = vec![];
    let mut budget = 2_000usize;
    collect_files(dir, 1, &mut budget, &mut |p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        if ext_of(p) == "exe" && !n.starts_with("unins") {
            let len = fs::metadata(p).map(|m| if m.file_attributes() & NOT_LOCAL == 0 { m.len() } else { 0 }).unwrap_or(0);
            if len > 0 {
                exes.push((len, p.to_path_buf()));
            }
        }
    });
    exes.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, exe) in exes.iter().take(3) {
        if let Some(c) = company_name(exe) {
            return c;
        }
    }
    match exes.first().filter(|(len, _)| *len <= 512 * MB).map(|(_, p)| verify_embedded(p)) {
        Some((0, Some(signer))) => signer,
        _ => String::new(),
    }
}

/// Every place that can still point into an installed program's folder, as one lower-case text:
/// uninstall entries (all values), services, Run keys, App Paths, PATH, scheduled tasks and shortcuts.
fn reference_hay(entries: &[UninstEntry]) -> String {
    let mut h = String::new();
    for e in entries {
        for s in &e.strings {
            h.push_str(s);
            h.push('\n');
        }
    }
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(svcs) = hklm.open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Services", KEY_READ) {
        for name in svcs.enum_keys().flatten() {
            push_key_values(&mut h, &svcs, &name, &["ImagePath"]);
            push_key_values(&mut h, &svcs, &format!(r"{name}\Parameters"), &["ServiceDll"]);
        }
    }
    for root in [&hklm, &hkcu] {
        for base in [r"SOFTWARE\Microsoft\Windows\CurrentVersion", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion"] {
            for list in ["Run", "RunOnce", r"Policies\Explorer\Run"] {
                if let Ok(k) = root.open_subkey_with_flags(format!(r"{base}\{list}"), KEY_READ | KEY_WOW64_64KEY) {
                    h.push_str(&reg_strings(&k).join("\n"));
                    h.push('\n');
                }
            }
            if let Ok(ap) = root.open_subkey_with_flags(format!(r"{base}\App Paths"), KEY_READ | KEY_WOW64_64KEY) {
                for name in ap.enum_keys().flatten() {
                    if let Ok(k) = ap.open_subkey_with_flags(&name, KEY_READ) {
                        h.push_str(&reg_strings(&k).join("\n"));
                        h.push('\n');
                    }
                }
            }
        }
        // per-user Python installs register here, not always with an InstallLocation
        for base in [r"SOFTWARE\Python\PythonCore", r"SOFTWARE\WOW6432Node\Python\PythonCore"] {
            if let Ok(pc) = root.open_subkey_with_flags(base, KEY_READ | KEY_WOW64_64KEY) {
                for ver in pc.enum_keys().flatten() {
                    push_key_values(&mut h, &pc, &format!(r"{ver}\InstallPath"), &["", "ExecutablePath"]);
                }
            }
        }
    }
    // Windows Installer's record of every file it installed: a suite often spreads over several folders
    // while its uninstall entry names only one of them
    if let Ok(ud) = hklm.open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Installer\UserData", KEY_READ | KEY_WOW64_64KEY) {
        for sid in ud.enum_keys().flatten() {
            let Ok(comps) = ud.open_subkey_with_flags(format!(r"{sid}\Components"), KEY_READ | KEY_WOW64_64KEY) else { continue };
            for c in comps.enum_keys().flatten() {
                if let Ok(k) = comps.open_subkey_with_flags(&c, KEY_READ) {
                    for s in reg_strings(&k) {
                        h.push_str(&msi_path(&s));
                        h.push('\n');
                    }
                }
            }
        }
    }
    // programs' own settings: a suite's launcher or another of its parts often names the folder only here
    // (Brother's utilities point to RemoteSetup from HKLM\SOFTWARE\WOW6432Node\Brother)
    for (root, base) in [(&hklm, "SOFTWARE"), (&hklm, r"SOFTWARE\WOW6432Node"), (&hkcu, "SOFTWARE")] {
        let mut budget = 60_000usize;
        vendor_strings(&mut h, root, base, &mut budget);
    }
    push_key_values(&mut h, &hklm, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", &["Path"]);
    push_key_values(&mut h, &hkcu, "Environment", &["Path"]);
    h.push_str(&std::env::var("PATH").unwrap_or_default());
    h.push('\n');
    // scheduled tasks (readable when elevated) and shortcuts
    let mut files: Vec<PathBuf> = vec![];
    let tasks = PathBuf::from(windir()).join(r"System32\Tasks");
    let mut budget = 20_000usize;
    collect_files(&tasks, 6, &mut budget, &mut |p| files.push(p.to_path_buf()));
    for p in files.drain(..) {
        if let Some(t) = read_text(&p) {
            h.push_str(&t);
            h.push('\n');
        }
    }
    let menus = [
        env_dir("ProgramData").map(|p| p.join(r"Microsoft\Windows\Start Menu")),
        env_dir("APPDATA").map(|p| p.join(r"Microsoft\Windows\Start Menu")),
        env_dir("PUBLIC").map(|p| p.join("Desktop")),
        known_folder(KnownFolder::Desktop),
    ];
    for m in menus.into_iter().flatten() {
        let mut budget = 20_000usize;
        collect_files(&m, 6, &mut budget, &mut |p| {
            if ext_of(p) == "lnk" {
                files.push(p.to_path_buf());
            }
        });
    }
    for p in files {
        h.push_str(&lnk_text(&p));
        h.push('\n');
    }
    util::expand_env(&h).replace('/', "\\").to_lowercase()
}

/// Files below `dir` (links and junctions not followed), up to `depth` levels and `budget` entries.
fn collect_files(dir: &Path, depth: usize, budget: &mut usize, f: &mut dyn FnMut(&Path)) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        if *budget == 0 {
            return;
        }
        *budget -= 1;
        let Ok(m) = e.metadata() else { continue };
        if is_link(&m) {
            continue;
        }
        if m.is_dir() {
            if depth > 0 {
                collect_files(&e.path(), depth - 1, budget, f);
            }
        } else {
            f(&e.path());
        }
    }
}

/// `key` (a lower-case folder path) is named in `hay`, followed by something that ends a path segment.
/// "c:\program files\foo" also matches "c:\program files\foo bar": a false "still used" is the safe side.
fn mentions(hay: &str, key: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(key) {
        let end = from + i + key.len();
        match hay[end..].chars().next() {
            None | Some('\\' | '/' | '"' | '\'' | ',' | ' ' | ';' | '\0' | '\r' | '\n' | '|' | '<' | '\t') => return true,
            _ => {}
        }
        from = end;
    }
    false
}

fn short_path(p: &Path) -> Option<String> {
    use windows::Win32::Storage::FileSystem::GetShortPathNameW;
    let w = wide(p.as_os_str());
    let mut buf = [0u16; 1024];
    let n = unsafe { GetShortPathNameW(windows::core::PCWSTR(w.as_ptr()), Some(&mut buf)) } as usize;
    (n > 0 && n < buf.len()).then(|| lower(Path::new(&String::from_utf16_lossy(&buf[..n]))))
}

/// Bytes stored on this disk below `dir` and its newest file change. Reparse points of any kind are
/// skipped, and the walk stops after `cap` entries (then `partial` is true).
fn dir_stats(dir: &Path, cap: usize) -> (u64, u64, bool) {
    let (mut bytes, mut newest, mut seen) = (0u64, 0u64, 0usize);
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > cap {
                return (bytes, newest, true);
            }
            let Ok(m) = e.metadata() else { continue };
            if is_reparse(&m) {
                continue;
            }
            newest = newest.max(unix(m.modified()));
            if m.is_dir() {
                stack.push(e.path());
            } else if m.file_attributes() & NOT_LOCAL == 0 {
                bytes += m.len();
            }
        }
    }
    (bytes, newest, false)
}

/// Program folders nothing points into any more. Conservative on purpose: anything named by an
/// uninstall entry, a service, a Run key, App Paths, PATH, a scheduled task or a shortcut, anything a
/// running process was started from, anything changed in the last 30 days, and Windows', Microsoft's
/// and driver vendors' folders are left out.
fn leftover_folders(entries: &[UninstEntry]) -> Vec<OrphanProgram> {
    let hay = reference_hay(entries);
    let procs = processes();
    let fresh = now().saturating_sub(LEFTOVER_QUIET_DAYS * DAY);
    // an entry hidden from Installed apps (a suite's part, SystemComponent) often has no InstallLocation,
    // but its name is the folder's: "RemoteSetup" in C:\Program Files (x86)\RemoteSetup
    let entry_names: HashSet<String> = entries.iter().map(|e| name_key(&e.name)).filter(|n| n.len() >= 3).collect();
    let mut out = vec![];
    for root in leftover_roots() {
        let Ok(rd) = fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let Ok(m) = fs::symlink_metadata(e.path()) else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if !m.is_dir() || is_reparse(&m) || is_system_folder(&name) {
                continue;
            }
            let path = e.path();
            let l = lower(&path);
            let short = short_path(&path).filter(|s| *s != l);
            if mentions(&hay, &l) || short.as_deref().is_some_and(|s| mentions(&hay, s)) || entry_names.contains(&name_key(&name)) {
                continue;
            }
            if procs.iter().any(|(exe, cmd)| within(exe, &l) || mentions(cmd, &l)) {
                continue;
            }
            // a folder too big to walk fully is left out: its newest change can't be known
            let (bytes, modified, partial) = dir_stats(&path, 50_000);
            if partial || modified == 0 || modified > fresh || unix(m.modified()) > fresh {
                continue;
            }
            let mut budget = 2_000usize;
            let mut has_exe = false;
            collect_files(&path, 1, &mut budget, &mut |p| has_exe |= ext_of(p) == "exe");
            out.push(OrphanProgram {
                kind: "leftover_folder",
                name,
                publisher: if has_exe { folder_publisher(&path) } else { String::new() },
                version: String::new(),
                install_location: String::new(),
                uninstall_string: String::new(),
                reg_key: String::new(),
                reason: "",
                missing_file: String::new(),
                estimated_kb: 0,
                path: path.to_string_lossy().into_owned(),
                bytes,
                modified,
                partial,
                has_exe,
            });
        }
    }
    out
}

/// Uninstall entries whose uninstaller is gone, then program folders nothing points to any more.
pub fn find_orphan_programs() -> Vec<OrphanProgram> {
    let entries = uninstall_entries(true);
    let mut out = dead_entries(&entries);
    out.extend(leftover_folders(&entries));
    out
}

fn reg_type_name(t: &RegType) -> &'static str {
    match t {
        RegType::REG_NONE => "REG_NONE",
        RegType::REG_SZ => "REG_SZ",
        RegType::REG_EXPAND_SZ => "REG_EXPAND_SZ",
        RegType::REG_BINARY => "REG_BINARY",
        RegType::REG_DWORD => "REG_DWORD",
        RegType::REG_DWORD_BIG_ENDIAN => "REG_DWORD_BIG_ENDIAN",
        RegType::REG_LINK => "REG_LINK",
        RegType::REG_MULTI_SZ => "REG_MULTI_SZ",
        RegType::REG_RESOURCE_LIST => "REG_RESOURCE_LIST",
        RegType::REG_FULL_RESOURCE_DESCRIPTOR => "REG_FULL_RESOURCE_DESCRIPTOR",
        RegType::REG_RESOURCE_REQUIREMENTS_LIST => "REG_RESOURCE_REQUIREMENTS_LIST",
        RegType::REG_QWORD => "REG_QWORD",
    }
}

/// A key with its values and subkeys as JSON. `raw` (hex) restores any value exactly; `data` is the readable form.
fn export_key(k: &RegKey, path: &str) -> serde_json::Value {
    let values: Vec<serde_json::Value> = k
        .enum_values()
        .flatten()
        .map(|(name, v)| {
            let data = match v.vtype {
                RegType::REG_SZ | RegType::REG_EXPAND_SZ => serde_json::json!(utf16_lossy(&v.bytes).trim_end_matches('\0')),
                RegType::REG_MULTI_SZ => {
                    serde_json::json!(utf16_lossy(&v.bytes).split('\0').filter(|s| !s.is_empty()).collect::<Vec<_>>())
                }
                RegType::REG_DWORD if v.bytes.len() >= 4 => serde_json::json!(u32::from_le_bytes([v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3]])),
                RegType::REG_QWORD if v.bytes.len() >= 8 => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&v.bytes[..8]);
                    serde_json::json!(u64::from_le_bytes(b))
                }
                _ => serde_json::Value::Null,
            };
            let raw: String = v.bytes.iter().map(|b| format!("{b:02x}")).collect();
            serde_json::json!({ "name": name, "type": reg_type_name(&v.vtype), "data": data, "raw": raw })
        })
        .collect();
    let subkeys: Vec<serde_json::Value> = k
        .enum_keys()
        .flatten()
        .filter_map(|n| k.open_subkey_with_flags(&n, KEY_READ).ok().map(|s| export_key(&s, &format!("{path}\\{n}"))))
        .collect();
    serde_json::json!({ "key": path, "values": values, "subkeys": subkeys })
}

fn backup_dir() -> R<PathBuf> {
    let d = env_dir("LOCALAPPDATA").ok_or("LOCALAPPDATA is not set")?.join(r"RigSweep\backups");
    fs::create_dir_all(&d).map_err(|e| format!("could not create the backup folder: {e}"))?;
    Ok(d)
}

/// Delete one uninstall entry whose uninstaller file is still missing, after a JSON backup of the key.
/// Returns the backup file's path.
pub fn remove_dead_entry(reg_key: &str) -> R<String> {
    let (hive, sub) = reg_key.split_once('\\').ok_or("not a registry key")?;
    let root = hive_root(hive).ok_or("only HKLM and HKCU uninstall entries can be removed")?;
    let (parent, name) = sub.rsplit_once('\\').ok_or("not an uninstall entry")?;
    if name.is_empty() || !(parent.eq_ignore_ascii_case(UNINSTALL) || parent.eq_ignore_ascii_case(UNINSTALL_WOW)) {
        return Err("not an uninstall entry".into());
    }
    let hive_short = if hive.to_ascii_uppercase().contains("LOCAL_MACHINE") || hive.eq_ignore_ascii_case("HKLM") { "HKLM" } else { "HKCU" };
    let entries = uninstall_entries(false);
    let e = entries
        .iter()
        .find(|e| e.hive == hive_short && e.key.eq_ignore_ascii_case(sub))
        .ok_or("the entry is already gone")?;
    if dead_reason(e, &windir()).is_none() {
        return Err("its uninstaller can be found again, so the entry was left alone".into());
    }
    let key = root.open_subkey_with_flags(sub, KEY_READ | KEY_WOW64_64KEY).map_err(err)?;
    let mut doc = export_key(&key, &format!("{hive_short}\\{sub}"));
    doc["exported"] = serde_json::json!(chrono::Local::now().to_rfc3339());
    doc["display_name"] = serde_json::json!(e.name);
    drop(key);
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).take(60).collect();
    let file = backup_dir()?.join(format!("uninstall-{hive_short}-{safe}-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    let text = serde_json::to_string_pretty(&doc).map_err(err)?;
    fs::write(&file, text).map_err(|e| format!("could not write the backup, nothing was removed: {e}"))?;
    let denied = |e: std::io::Error| {
        if e.raw_os_error() == Some(5) {
            "Windows denied access: removing an entry for all users needs administrator rights".to_string()
        } else {
            e.to_string()
        }
    };
    let p = root.open_subkey_with_flags(parent, KEY_READ | KEY_WRITE | KEY_WOW64_64KEY).map_err(denied)?;
    p.delete_subkey_all(name).map_err(denied)?;
    Ok(file.to_string_lossy().into_owned())
}

/// Recycle a program folder, but only if it is still a leftover by every check of the finder.
pub fn recycle_leftover(path: &str) -> R<()> {
    let target = lower(Path::new(path));
    let entries = uninstall_entries(true);
    if !leftover_folders(&entries).iter().any(|o| lower(Path::new(&o.path)) == target) {
        return Err("this folder is no longer a leftover (something uses it now, or it changed), so it was left alone".into());
    }
    util::recycle(Path::new(path)).map_err(err)
}

// ---------------------------------------------------------------------------------------------------------------
// 2. old files

#[derive(Clone, Copy)]
enum KnownFolder {
    Downloads,
    Desktop,
    Documents,
}

/// The real location (Documents is often redirected to OneDrive, Downloads may be moved to another drive).
fn known_folder(f: KnownFolder) -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, KNOWN_FOLDER_FLAG};
    let id = match f {
        KnownFolder::Downloads => FOLDERID_Downloads,
        KnownFolder::Desktop => FOLDERID_Desktop,
        KnownFolder::Documents => FOLDERID_Documents,
    };
    unsafe {
        let p = SHGetKnownFolderPath(&id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s.filter(|s| !s.is_empty()).map(PathBuf::from)
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct OldFile {
    pub path: String,
    pub bytes: u64,
    /// last modified, unix seconds: what "old" is based on
    pub modified: u64,
    /// last access, unix seconds. Unreliable: Windows often doesn't update it, so it's shown, never used.
    pub accessed: u64,
    /// big_old | old_installer | old_archive
    pub category: &'static str,
    /// in a OneDrive folder: recycling deletes it from OneDrive on every device too (OneDrive keeps its own recycle bin)
    pub onedrive: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct OldFiles {
    /// the folders that were scanned
    pub folders: Vec<String>,
    /// biggest first
    pub files: Vec<OldFile>,
    /// more matched than `files` holds
    pub truncated: bool,
    pub total_found: usize,
    pub total_bytes: u64,
}

const INSTALLER_EXTS: &[&str] = &["exe", "msi", "msix", "msixbundle", "appx", "iso", "dmg"];
const ARCHIVE_EXTS: &[&str] = &["zip", "7z", "rar", "tar", "gz", "tgz", "xz", "bz2", "zst"];
const OLD_MAX_DEPTH: usize = 8;
const OLD_MAX_ENTRIES: usize = 300_000;
const OLD_MAX_RESULTS: usize = 500;
const PROJECT_MARKERS: &[&str] = &[".git", "package.json", "cargo.toml"];

/// The folders the last old-files scan covered: recycle_files only touches files below them.
static LAST_ROOTS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn default_old_roots() -> Vec<PathBuf> {
    [KnownFolder::Downloads, KnownFolder::Desktop, KnownFolder::Documents].into_iter().filter_map(known_folder).collect()
}

fn onedrive_roots() -> Vec<String> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"].iter().filter_map(|v| env_dir(v)).map(|p| lower(&p)).collect()
}

fn is_cloud(m: &fs::Metadata) -> bool {
    let a = m.file_attributes();
    a & (ATTR_PINNED | ATTR_UNPINNED | NOT_LOCAL) != 0 || (is_reparse(m) && !is_link(m))
}

struct WalkLimits {
    depth: usize,
    entries: usize,
}

/// Walk files below `root` without following links or junctions and without entering node_modules or a
/// project (a folder holding .git, package.json, Cargo.toml or a .sln): those belong to Dev Disk.
fn walk(dir: &Path, depth: usize, lim: &mut WalkLimits, f: &mut dyn FnMut(&Path, &fs::Metadata)) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let list: Vec<fs::DirEntry> = rd.flatten().collect();
    if depth > 0 {
        let project = list.iter().any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            PROJECT_MARKERS.contains(&n.as_str()) || n.ends_with(".sln")
        });
        if project {
            return;
        }
    }
    for e in list {
        if lim.entries == 0 {
            return;
        }
        lim.entries -= 1;
        let Ok(m) = e.metadata() else { continue };
        if is_link(&m) {
            continue;
        }
        if m.is_dir() {
            let n = e.file_name().to_string_lossy().to_lowercase();
            if depth < lim.depth && n != "node_modules" && !n.starts_with('$') {
                walk(&e.path(), depth + 1, lim, f);
            }
        } else {
            f(&e.path(), &m);
        }
    }
}

struct OldRules {
    min_days: u64,
    min_bytes: u64,
    installer_days: u64,
    installer_bytes: u64,
}

fn scan_old(roots: &[PathBuf], downloads: Option<&Path>, rules: &OldRules) -> OldFiles {
    let now = now();
    let od = onedrive_roots();
    let dl = downloads.map(lower);
    let mut seen: HashSet<String> = HashSet::new();
    let mut files: Vec<OldFile> = vec![];
    let mut lim = WalkLimits { depth: OLD_MAX_DEPTH, entries: OLD_MAX_ENTRIES };
    for root in roots {
        walk(root, 0, &mut lim, &mut |p, m| {
            // online-only files are left out: even reading their size is fine, but we never trigger downloads
            if m.file_attributes() & NOT_LOCAL != 0 {
                return;
            }
            let l = lower(p);
            if !seen.insert(l.clone()) {
                return;
            }
            let modified = unix(m.modified());
            let age = now.saturating_sub(modified) / DAY;
            let bytes = m.len();
            let ext = ext_of(p);
            let kind = if !dl.as_deref().is_some_and(|d| within(&l, d)) {
                None
            } else if INSTALLER_EXTS.contains(&ext.as_str()) {
                Some("old_installer")
            } else if ARCHIVE_EXTS.contains(&ext.as_str()) {
                Some("old_archive")
            } else {
                None
            };
            let category = if let Some(k) = kind.filter(|_| age >= rules.installer_days && bytes >= rules.installer_bytes) {
                k
            } else if age >= rules.min_days && bytes >= rules.min_bytes {
                "big_old"
            } else {
                return;
            };
            files.push(OldFile {
                path: p.to_string_lossy().into_owned(),
                bytes,
                modified,
                accessed: unix(m.accessed()),
                category,
                onedrive: is_cloud(m) || od.iter().any(|o| within(&l, o)),
            });
        });
    }
    files.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let total_found = files.len();
    let total_bytes = files.iter().map(|f| f.bytes).sum();
    let truncated = files.len() > OLD_MAX_RESULTS;
    files.truncate(OLD_MAX_RESULTS);
    OldFiles { folders: roots.iter().map(|r| r.to_string_lossy().into_owned()).collect(), files, truncated, total_found, total_bytes }
}

/// Big files nobody changed for `min_days` (defaults: 365 days, 50 MB), plus installers and archives in
/// Downloads older than 90 days (5 MB and up). `folders` empty = Downloads, Desktop and Documents.
pub fn find_old_files(folders: Vec<String>, min_days: u32, min_bytes: u64) -> OldFiles {
    let wd = windir();
    let roots: Vec<PathBuf> = if folders.is_empty() {
        default_old_roots()
    } else {
        folders.iter().filter_map(|f| absolute(f)).filter(|p| p.is_dir() && !within(&lower(p), &wd)).collect()
    };
    if let Ok(mut l) = LAST_ROOTS.lock() {
        *l = roots.clone();
    }
    let rules = OldRules {
        min_days: if min_days == 0 { 365 } else { min_days as u64 },
        min_bytes: if min_bytes == 0 { 50 * MB } else { min_bytes },
        installer_days: 90,
        installer_bytes: 5 * MB,
    };
    let dl = known_folder(KnownFolder::Downloads);
    scan_old(&roots, dl.as_deref(), &rules)
}

#[derive(Serialize, Clone, Debug)]
pub struct RecycleResult {
    pub path: String,
    pub ok: bool,
    pub error: Option<String>,
}

fn check_recyclable_file(p: &str, roots: &[PathBuf]) -> R<PathBuf> {
    let path = absolute(p).ok_or("not a full path")?;
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("not a plain path".into());
    }
    let l = lower(&path);
    if within(&l, &windir()) || !roots.iter().any(|r| within(&l, &lower(r)) && l != lower(r)) {
        return Err("not in a scanned folder".into());
    }
    let m = fs::symlink_metadata(&path).map_err(|_| "no longer exists")?;
    if !m.is_file() || is_link(&m) {
        return Err("not a regular file".into());
    }
    Ok(path)
}

/// Recycle files the user picked from the old-files list; each must still be a regular file below a scanned folder.
pub fn recycle_files(paths: Vec<String>) -> Vec<RecycleResult> {
    let mut roots = LAST_ROOTS.lock().map(|l| l.clone()).unwrap_or_default();
    if roots.is_empty() {
        roots = default_old_roots();
    }
    paths
        .into_iter()
        .map(|p| {
            let r = check_recyclable_file(&p, &roots).and_then(|path| util::recycle(&path).map_err(err));
            RecycleResult { path: p, ok: r.is_ok(), error: r.err() }
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------------------------
// 3. files worth a look. These are hints for a closer look, never a verdict: Microsoft Defender decides.

#[derive(Serialize, Clone, Debug)]
pub struct LookItem {
    pub path: String,
    /// unsigned_autostart | unsigned_in_appdata | double_extension | rtlo_name | unsigned_download
    pub reason: &'static str,
    /// signed | unsigned | invalid | not_checked
    pub signature: &'static str,
    /// certificate name when the file carries a signature Windows doesn't trust
    pub signer: Option<String>,
    /// where it is set to start with Windows (registry value, Startup folder or task)
    pub autostart: Option<String>,
    pub running: bool,
    /// only for listed files under 200 MB that are stored on this disk
    pub sha256: Option<String>,
    pub bytes: u64,
    pub modified: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum Sig {
    Signed(Option<String>),
    Unsigned,
    /// a signature Windows does not trust (bad digest, untrusted or distrusted certificate)
    Invalid(Option<String>),
    /// the file couldn't be read
    Unknown,
}

#[repr(C)]
struct CatalogInfo {
    cb_struct: u32,
    catalog_file: [u16; 260],
}

// the catalog functions sit behind Cargo features this crate doesn't enable; their ABI is stable
#[link(name = "wintrust", kind = "raw-dylib")]
extern "system" {
    fn CryptCATAdminAcquireContext2(admin: *mut isize, subsystem: *const windows::core::GUID, alg: windows::core::PCWSTR, policy: *const c_void, flags: u32) -> i32;
    fn CryptCATAdminCalcHashFromFileHandle2(admin: isize, file: windows::Win32::Foundation::HANDLE, cb: *mut u32, hash: *mut u8, flags: u32) -> i32;
    fn CryptCATAdminEnumCatalogFromHash(admin: isize, hash: *const u8, cb: u32, flags: u32, prev: *mut isize) -> isize;
    fn CryptCATCatalogInfoFromContext(cat: isize, info: *mut CatalogInfo, flags: u32) -> i32;
    fn CryptCATAdminReleaseCatalogContext(admin: isize, cat: isize, flags: u32) -> i32;
    fn CryptCATAdminReleaseContext(admin: isize, flags: u32) -> i32;
    fn WTHelperProvDataFromStateData(state: windows::Win32::Foundation::HANDLE) -> *mut c_void;
    fn WTHelperGetProvSignerFromChain(
        data: *mut c_void,
        signer: u32,
        counter: i32,
        counter_idx: u32,
    ) -> *mut windows::Win32::Security::WinTrust::CRYPT_PROVIDER_SGNR;
}

/// Name on the leaf certificate of a verified (or failed) signature.
unsafe fn signer_name(state: windows::Win32::Foundation::HANDLE) -> Option<String> {
    use windows::Win32::Security::Cryptography::{CertGetNameStringW, CERT_NAME_SIMPLE_DISPLAY_TYPE};
    if state.0.is_null() {
        return None;
    }
    unsafe {
        let data = WTHelperProvDataFromStateData(state);
        if data.is_null() {
            return None;
        }
        let sg = WTHelperGetProvSignerFromChain(data, 0, 0, 0);
        if sg.is_null() || (*sg).csCertChain == 0 || (*sg).pasCertChain.is_null() {
            return None;
        }
        let cert = (*(*sg).pasCertChain).pCert;
        if cert.is_null() {
            return None;
        }
        let mut buf = [0u16; 256];
        let n = CertGetNameStringW(cert, CERT_NAME_SIMPLE_DISPLAY_TYPE, 0, None, Some(&mut buf)) as usize;
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n - 1]))
    }
}

/// WinVerifyTrust (generic verify v2, no UI, no revocation check, cached URLs only) on prepared data.
unsafe fn win_verify(data: &mut windows::Win32::Security::WinTrust::WINTRUST_DATA) -> (i32, Option<String>) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Security::WinTrust::*;
    data.cbStruct = std::mem::size_of::<WINTRUST_DATA>() as u32;
    data.dwUIChoice = WTD_UI_NONE;
    data.fdwRevocationChecks = WTD_REVOKE_NONE;
    data.dwStateAction = WTD_STATEACTION_VERIFY;
    data.dwProvFlags = WTD_CACHE_ONLY_URL_RETRIEVAL;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let no_ui = HWND(-1isize as *mut c_void);
    unsafe {
        let rc = WinVerifyTrust(no_ui, &mut action, data as *mut _ as *mut c_void);
        let signer = signer_name(data.hWVTStateData);
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        WinVerifyTrust(no_ui, &mut action, data as *mut _ as *mut c_void);
        (rc, signer)
    }
}

fn verify_embedded(path: &Path) -> (i32, Option<String>) {
    use windows::Win32::Security::WinTrust::*;
    let w = wide(path.as_os_str());
    let mut file = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: windows::core::PCWSTR(w.as_ptr()),
        ..Default::default()
    };
    let mut data = WINTRUST_DATA { dwUnionChoice: WTD_CHOICE_FILE, ..Default::default() };
    data.Anonymous = WINTRUST_DATA_0 { pFile: &mut file };
    unsafe { win_verify(&mut data) }
}

/// Windows' own files are mostly signed through a catalog, not inside the file. Returns None when no
/// installed catalog lists the file.
fn verify_catalog(path: &Path) -> Option<(i32, Option<String>)> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::WinTrust::*;
    let f = fs::File::open(path).ok()?;
    let h = HANDLE(f.as_raw_handle());
    let member = wide(path.as_os_str());
    for alg in [windows::core::w!("SHA256"), windows::core::w!("SHA1")] {
        let mut admin: isize = 0;
        unsafe {
            if CryptCATAdminAcquireContext2(&mut admin, std::ptr::null(), alg, std::ptr::null(), 0) == 0 {
                continue;
            }
            let mut hash = [0u8; 64];
            let mut cb = hash.len() as u32;
            let mut result = None;
            if CryptCATAdminCalcHashFromFileHandle2(admin, h, &mut cb, hash.as_mut_ptr(), 0) != 0 {
                let cat = CryptCATAdminEnumCatalogFromHash(admin, hash.as_ptr(), cb, 0, std::ptr::null_mut());
                if cat != 0 {
                    let mut info = CatalogInfo { cb_struct: std::mem::size_of::<CatalogInfo>() as u32, catalog_file: [0; 260] };
                    if CryptCATCatalogInfoFromContext(cat, &mut info, 0) != 0 {
                        let tag: Vec<u16> = hash[..cb as usize].iter().map(|b| format!("{b:02X}")).collect::<String>().encode_utf16().chain([0]).collect();
                        let mut ci = WINTRUST_CATALOG_INFO {
                            cbStruct: std::mem::size_of::<WINTRUST_CATALOG_INFO>() as u32,
                            pcwszCatalogFilePath: windows::core::PCWSTR(info.catalog_file.as_ptr()),
                            pcwszMemberTag: windows::core::PCWSTR(tag.as_ptr()),
                            pcwszMemberFilePath: windows::core::PCWSTR(member.as_ptr()),
                            hMemberFile: h,
                            pbCalculatedFileHash: hash.as_mut_ptr(),
                            cbCalculatedFileHash: cb,
                            hCatAdmin: admin,
                            ..Default::default()
                        };
                        let mut data = WINTRUST_DATA { dwUnionChoice: WTD_CHOICE_CATALOG, ..Default::default() };
                        data.Anonymous = WINTRUST_DATA_0 { pCatalog: &mut ci };
                        result = Some(win_verify(&mut data));
                    }
                    CryptCATAdminReleaseCatalogContext(admin, cat, 0);
                }
            }
            CryptCATAdminReleaseContext(admin, 0);
            if result.is_some() {
                return result;
            }
        }
    }
    None
}

/// Embedded Authenticode signature first, then the system catalogs.
fn check_signature(path: &Path) -> Sig {
    const NO_SIGNATURE: [u32; 3] = [0x800B_0100, 0x800B_0003, 0x800B_0001];
    const UNREADABLE: [u32; 4] = [0x8009_2003, 0x8007_0005, 0x8007_0020, 0x8007_0002];
    let (rc, signer) = verify_embedded(path);
    let code = rc as u32;
    if rc == 0 {
        return Sig::Signed(signer);
    }
    if UNREADABLE.contains(&code) {
        return Sig::Unknown;
    }
    if !NO_SIGNATURE.contains(&code) {
        return Sig::Invalid(signer);
    }
    match verify_catalog(path) {
        Some((0, s)) => Sig::Signed(s),
        Some((_, s)) => Sig::Invalid(s),
        None => Sig::Unsigned,
    }
}

/// Files under C:\Windows count as signed: Windows protects and services them itself.
fn signature(path: &Path, cache: &mut HashMap<String, Sig>) -> Sig {
    let l = lower(path);
    if within(&l, &windir()) {
        return Sig::Signed(None);
    }
    if let Some(s) = cache.get(&l) {
        return s.clone();
    }
    // hashing a huge file for a hint isn't worth the wait
    let s = match fs::metadata(path) {
        Ok(m) if m.file_attributes() & NOT_LOCAL != 0 || m.len() > 512 * MB => Sig::Unknown,
        Ok(_) => check_signature(path),
        Err(_) => Sig::Unknown,
    };
    cache.insert(l, s.clone());
    s
}

fn sha256(path: &Path) -> Option<String> {
    use windows::Win32::Security::Cryptography::*;
    let mut f = fs::File::open(path).ok()?;
    unsafe {
        let mut alg = BCRYPT_ALG_HANDLE::default();
        if BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_SHA256_ALGORITHM, windows::core::PCWSTR::null(), BCRYPT_OPEN_ALGORITHM_PROVIDER_FLAGS(0)).0 < 0 {
            return None;
        }
        let mut h = BCRYPT_HASH_HANDLE::default();
        let mut out = [0u8; 32];
        let mut ok = BCryptCreateHash(alg, &mut h, None, None, 0).0 >= 0;
        if ok {
            let mut buf = vec![0u8; 1 << 20];
            loop {
                match f.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if BCryptHashData(h, &buf[..n], 0).0 < 0 {
                            ok = false;
                            break;
                        }
                    }
                    Err(_) => {
                        ok = false;
                        break;
                    }
                }
            }
            ok = ok && BCryptFinishHash(h, &mut out, 0).0 >= 0;
            let _ = BCryptDestroyHash(h);
        }
        let _ = BCryptCloseAlgorithmProvider(alg, 0);
        ok.then(|| out.iter().map(|b| format!("{b:02x}")).collect())
    }
}

/// Executable types a double extension or a Downloads check cares about.
const EXEC_EXTS: &[&str] = &["exe", "scr", "com", "pif", "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "ps1", "hta", "cpl", "msi", "lnk", "jar"];
/// Programs checked in Downloads (top level and one folder down). Scripts are left out: files unpacked
/// from a downloaded zip inherit its Mark-of-the-Web, so every .js of a web template would show up.
const DOWNLOAD_EXTS: &[&str] = &["exe", "msi", "scr", "com", "pif", "cpl", "hta"];
/// What the fake first extension pretends to be.
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "txt", "rtf", "odt", "csv", "jpg", "jpeg", "png", "gif", "bmp", "webp", "mp3",
    "mp4", "avi", "mov", "mkv", "wav", "zip", "rar", "7z", "html", "htm",
];

/// `invoice.pdf.exe` (also `invoice.pdf   .exe`): a document extension followed by an executable one.
fn is_double_extension(name: &str) -> bool {
    let n = name.to_lowercase();
    let Some((stem, last)) = n.rsplit_once('.') else { return false };
    if !EXEC_EXTS.contains(&last.trim()) {
        return false;
    }
    let stem = stem.trim_end();
    let Some((_, prev)) = stem.rsplit_once('.') else { return false };
    DOC_EXTS.contains(&prev.trim())
}

/// U+202E (right-to-left override) turns `invoice<U+202E>fdp.exe` into "invoiceexe.pdf" on screen.
fn has_rtlo(name: &str) -> bool {
    name.contains('\u{202E}')
}

/// ZoneId of the Mark-of-the-Web stream (3 = downloaded from the internet).
fn zone_id(path: &Path) -> Option<u32> {
    let mut s = path.as_os_str().to_owned();
    s.push(":Zone.Identifier");
    let b = fs::read(PathBuf::from(s)).ok()?;
    let text = if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE { utf16_lossy(&b[2..]) } else { String::from_utf8_lossy(&b).into_owned() };
    text.lines().find_map(|l| l.trim().strip_prefix("ZoneId=").and_then(|v| v.trim().parse().ok()))
}

/// Target and arguments of a shortcut.
fn resolve_lnk(path: &Path) -> Option<(String, String)> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, IPersistFile, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink, SLGP_RAWPATH};
    let w = wide(path.as_os_str());
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let pf: IPersistFile = link.cast().ok()?;
        pf.Load(windows::core::PCWSTR(w.as_ptr()), STGM_READ).ok()?;
        let mut target = [0u16; 1024];
        link.GetPath(&mut target, std::ptr::null_mut(), SLGP_RAWPATH.0 as u32).ok()?;
        let mut args = [0u16; 2048];
        let _ = link.GetArguments(&mut args);
        let s = |b: &[u16]| String::from_utf16_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(b.len())]);
        Some((util::expand_env(&s(&target)), s(&args)))
    }
}

struct Autostart {
    path: PathBuf,
    source: String,
}

/// Run/RunOnce values, Startup folder items and scheduled-task programs (outside \Microsoft\, readable when elevated).
fn autostarts() -> Vec<Autostart> {
    let mut out = vec![];
    for (hive, root) in [("HKLM", RegKey::predef(HKEY_LOCAL_MACHINE)), ("HKCU", RegKey::predef(HKEY_CURRENT_USER))] {
        for base in [r"SOFTWARE\Microsoft\Windows\CurrentVersion", r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion"] {
            for list in ["Run", "RunOnce"] {
                let key = format!(r"{base}\{list}");
                let Ok(k) = root.open_subkey_with_flags(&key, KEY_READ | KEY_WOW64_64KEY) else { continue };
                for (name, v) in k.enum_values().flatten() {
                    if !matches!(v.vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
                        continue;
                    }
                    let cmd = utf16_lossy(&v.bytes).trim_end_matches('\0').to_string();
                    for t in command_targets(&cmd) {
                        out.push(Autostart { path: t, source: format!(r"{hive}\{key} : {name}") });
                    }
                }
            }
        }
    }
    let folders = [
        env_dir("APPDATA").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\Startup")),
        env_dir("ProgramData").map(|p| p.join(r"Microsoft\Windows\Start Menu\Programs\StartUp")),
    ];
    for dir in folders.into_iter().flatten() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let source = dir.to_string_lossy().into_owned();
            match ext_of(&p).as_str() {
                "ini" => {}
                "lnk" => {
                    if let Some((t, args)) = resolve_lnk(&p) {
                        for t in command_targets(&format!("\"{t}\" {args}")) {
                            out.push(Autostart { path: t, source: source.clone() });
                        }
                    }
                }
                _ => out.push(Autostart { path: p, source }),
            }
        }
    }
    let tasks = PathBuf::from(windir()).join(r"System32\Tasks");
    let mut files = vec![];
    let mut budget = 20_000usize;
    collect_files(&tasks, 6, &mut budget, &mut |p| files.push(p.to_path_buf()));
    let ms = lower(&tasks.join("Microsoft"));
    for p in files.into_iter().filter(|p| !within(&lower(p), &ms)) {
        let Some(xml) = read_text(&p) else { continue };
        let name = p.strip_prefix(&tasks).map(|r| format!("\\{}", r.display())).unwrap_or_default();
        let mut rest = xml.as_str();
        while let Some(i) = rest.find("<Command>") {
            rest = &rest[i + 9..];
            let Some(j) = rest.find("</Command>") else { break };
            let cmd = xml_unescape(rest[..j].trim());
            let args = rest[j..].find("<Arguments>").and_then(|a| {
                let s = &rest[j + a + 11..];
                s.find("</Arguments>").map(|b| xml_unescape(&s[..b]))
            });
            let line = format!("\"{}\" {}", cmd.trim_matches('"'), args.unwrap_or_default());
            for t in command_targets(&line) {
                out.push(Autostart { path: t, source: format!("Task {name}") });
            }
            rest = &rest[j..];
        }
    }
    out
}

fn xml_unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// `p` is directly in one of `roots` or one folder below.
fn shallow_in(p: &str, roots: &[String]) -> bool {
    roots.iter().any(|r| within(p, r) && p.len() > r.len() && p[r.len() + 1..].split('\\').count() <= 2)
}

fn look_item(path: &Path, reason: &'static str, sig: &Sig, autostart: Option<String>, running: bool) -> Option<LookItem> {
    let m = fs::symlink_metadata(path).ok()?;
    if !m.is_file() || is_link(&m) {
        return None;
    }
    let local = m.file_attributes() & NOT_LOCAL == 0;
    let (signature, signer) = match sig {
        Sig::Signed(_) => ("signed", None),
        Sig::Unsigned => ("unsigned", None),
        Sig::Invalid(s) => ("invalid", s.clone()),
        Sig::Unknown => ("not_checked", None),
    };
    Some(LookItem {
        path: path.to_string_lossy().into_owned(),
        reason,
        signature,
        signer,
        autostart,
        running,
        sha256: if local && m.len() < 200 * MB { sha256(path) } else { None },
        bytes: m.len(),
        modified: unix(m.modified()),
    })
}

/// Files that deserve a closer look: unsigned programs that start with Windows or run from AppData/Temp,
/// disguised names (double extension, right-to-left override) in Downloads/Desktop/Documents, and
/// unsigned programs downloaded from the internet. A hint, never a verdict.
pub fn find_worth_a_look() -> Vec<LookItem> {
    let wd = windir();
    let mut cache: HashMap<String, Sig> = HashMap::new();
    let mut listed: HashSet<String> = HashSet::new();
    let mut out: Vec<LookItem> = vec![];
    let procs = processes();
    let running: HashSet<String> = procs.iter().map(|(e, _)| e.clone()).filter(|e| !e.is_empty()).collect();
    let appdata: Vec<String> = ["TEMP", "APPDATA", "LOCALAPPDATA"].iter().filter_map(|v| env_dir(v)).map(|p| lower(&p)).collect();

    // (a) + (b): what starts with Windows, and what runs from AppData/Temp (programs and the scripts interpreters run)
    let mut cands: Vec<(PathBuf, Option<String>)> = autostarts().into_iter().map(|a| (a.path, Some(a.source))).collect();
    for (exe, cmd) in &procs {
        if shallow_in(exe, &appdata) {
            cands.push((PathBuf::from(exe), None));
        }
        if INTERPRETERS.contains(&base_name(exe).as_str()) {
            cands.extend(paths_in(cmd).into_iter().filter(|p| shallow_in(&lower(p), &appdata)).map(|p| (p, None)));
        }
    }
    let mut sources: HashMap<String, String> = HashMap::new();
    for (p, s) in &cands {
        if let Some(s) = s {
            sources.entry(lower(p)).or_insert_with(|| s.clone());
        }
    }
    for (p, _) in cands {
        let l = lower(&p);
        if within(&l, &wd) || listed.contains(&l) {
            continue;
        }
        let in_appdata = shallow_in(&l, &appdata) && EXEC_EXTS.contains(&ext_of(&p).as_str());
        let source = sources.get(&l).cloned();
        if !in_appdata && source.is_none() {
            continue;
        }
        let sig = signature(&p, &mut cache);
        if !matches!(sig, Sig::Unsigned | Sig::Invalid(_)) {
            continue;
        }
        let reason = if in_appdata { "unsigned_in_appdata" } else { "unsigned_autostart" };
        let run = running.contains(&l) || procs.iter().any(|(_, c)| mentions(c, &l));
        if let Some(item) = look_item(&p, reason, &sig, source, run) {
            listed.insert(l);
            out.push(item);
        }
    }

    // (c) + (d): disguised names in the user folders, unsigned downloads
    let downloads = known_folder(KnownFolder::Downloads);
    let dl = downloads.as_deref().map(lower);
    let roots: Vec<PathBuf> = [KnownFolder::Downloads, KnownFolder::Desktop, KnownFolder::Documents].into_iter().filter_map(known_folder).collect();
    let mut found: Vec<(PathBuf, &'static str)> = vec![];
    let mut lim = WalkLimits { depth: 6, entries: 200_000 };
    for root in &roots {
        walk(root, 0, &mut lim, &mut |p, m| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            if has_rtlo(&name) {
                found.push((p.to_path_buf(), "rtlo_name"));
            } else if is_double_extension(&name) {
                found.push((p.to_path_buf(), "double_extension"));
            } else if m.file_attributes() & NOT_LOCAL == 0
                && dl.as_ref().is_some_and(|d| shallow_in(&lower(p), std::slice::from_ref(d)))
                && DOWNLOAD_EXTS.contains(&ext_of(p).as_str())
            {
                found.push((p.to_path_buf(), "unsigned_download"));
            }
        });
    }
    for (p, reason) in found {
        let l = lower(&p);
        if listed.contains(&l) {
            continue;
        }
        let sig = if reason == "unsigned_download" {
            if zone_id(&p) != Some(3) {
                continue;
            }
            let s = signature(&p, &mut cache);
            if !matches!(s, Sig::Unsigned | Sig::Invalid(_)) {
                continue;
            }
            s
        } else if EXEC_EXTS.contains(&ext_of(&p).as_str()) {
            signature(&p, &mut cache)
        } else {
            Sig::Unknown
        };
        if let Some(item) = look_item(&p, reason, &sig, sources.get(&l).cloned(), running.contains(&l)) {
            listed.insert(l);
            out.push(item);
        }
    }
    out
}

#[derive(Serialize, Clone, Debug)]
pub struct DefenderResult {
    /// clean | threat | error
    pub verdict: &'static str,
    /// the last lines MpCmdRun printed
    pub detail: String,
    pub exit_code: Option<i32>,
}

/// The Defender platform under ProgramData is the one that gets updated; Program Files holds the original.
fn mpcmdrun() -> Option<PathBuf> {
    let ver = |s: &str| s.split(['.', '-']).map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    if let Some(pd) = env_dir("ProgramData") {
        let base = pd.join(r"Microsoft\Windows Defender\Platform");
        if let Ok(rd) = fs::read_dir(&base) {
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("MpCmdRun.exe").is_file()).collect();
            dirs.sort_by_key(|p| ver(&p.file_name().unwrap_or_default().to_string_lossy()));
            if let Some(d) = dirs.pop() {
                return Some(d.join("MpCmdRun.exe"));
            }
        }
    }
    let pf = env_dir("ProgramFiles")?.join(r"Windows Defender\MpCmdRun.exe");
    pf.is_file().then_some(pf)
}

const DEFENDER_TIMEOUT: Duration = Duration::from_secs(120);

/// Ask Microsoft Defender about one file or folder (custom scan, no remediation: it reports, it doesn't quarantine).
/// Exit code 0 = no threats, 2 = threats found. Nothing leaves the PC.
pub fn defender_scan(path: &str) -> DefenderResult {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    let fail = |d: String| DefenderResult { verdict: "error", detail: d, exit_code: None };
    let Some(p) = absolute(path).filter(|p| p.exists()) else { return fail("it no longer exists".into()) };
    // a folder (a program leftover) has many files to read
    let limit = if p.is_dir() { DEFENDER_TIMEOUT * 5 } else { DEFENDER_TIMEOUT };
    let Some(exe) = mpcmdrun() else { return fail("Microsoft Defender's command-line scanner was not found".into()) };
    let child = Command::new(&exe)
        .args(["-Scan", "-ScanType", "3", "-File"])
        .arg(&p)
        .arg("-DisableRemediation")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000)
        .spawn();
    let mut child = match child {
        Ok(c) => c,
        Err(e) => return fail(format!("could not start MpCmdRun: {e}")),
    };
    let mut so = child.stdout.take();
    let mut se = child.stderr.take();
    let t_out = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(s) = so.as_mut() {
            let _ = s.read_to_end(&mut b);
        }
        b
    });
    let t_err = std::thread::spawn(move || {
        let mut b = vec![];
        if let Some(s) = se.as_mut() {
            let _ = s.read_to_end(&mut b);
        }
        b
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(_) => break None,
        }
    };
    let text = format!("{}{}", String::from_utf8_lossy(&t_out.join().unwrap_or_default()), String::from_utf8_lossy(&t_err.join().unwrap_or_default()));
    let lines: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let detail = lines[lines.len().saturating_sub(15)..].join("\n");
    match status.and_then(|s| s.code()) {
        Some(0) => DefenderResult { verdict: "clean", detail, exit_code: Some(0) },
        Some(2) => DefenderResult { verdict: "threat", detail, exit_code: Some(2) },
        Some(c) => DefenderResult { verdict: "error", detail, exit_code: Some(c) },
        None => fail(if status.is_none() { format!("the scan took longer than {} s and was stopped", limit.as_secs()) } else { detail }),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// commands (registered in main.rs)

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> R<T> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(err)
}

#[tauri::command]
pub async fn finders_orphan_programs() -> R<Vec<OrphanProgram>> {
    blocking(find_orphan_programs).await
}

#[tauri::command]
pub async fn finders_remove_dead_entry(reg_key: String) -> R<String> {
    blocking(move || remove_dead_entry(&reg_key)).await?
}

#[tauri::command]
pub async fn finders_recycle_leftover(state: tauri::State<'_, crate::commands::AppState>, path: String) -> R<()> {
    let p = path.clone();
    blocking(move || recycle_leftover(&p)).await??;
    // RigSweep removed it: not space the AI helped free
    if let Ok(db) = state.db.lock() {
        db.drop_handoffs(&[path]);
    }
    Ok(())
}

#[tauri::command]
pub async fn finders_old_files(folders: Option<Vec<String>>, min_days: Option<u32>, min_bytes: Option<u64>) -> R<OldFiles> {
    blocking(move || find_old_files(folders.unwrap_or_default(), min_days.unwrap_or(365), min_bytes.unwrap_or(50 * MB))).await
}

#[tauri::command]
pub async fn finders_recycle_files(paths: Vec<String>) -> R<Vec<RecycleResult>> {
    blocking(move || recycle_files(paths)).await
}

#[tauri::command]
pub async fn finders_worth_a_look() -> R<Vec<LookItem>> {
    blocking(find_worth_a_look).await
}

#[tauri::command]
pub async fn finders_defender_scan(path: String) -> R<DefenderResult> {
    blocking(move || defender_scan(&path)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("finders-test-{name}"));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn file(p: &Path, bytes: u64, days_old: u64) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        let f = fs::File::create(p).unwrap();
        f.set_len(bytes).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(days_old * DAY)).unwrap();
    }

    #[test]
    fn finders_disguised_names() {
        assert!(is_double_extension("invoice.pdf.exe"));
        assert!(is_double_extension("Photo.JPG.scr"));
        assert!(is_double_extension("report.docx   .exe"));
        assert!(!is_double_extension("setup.exe"));
        assert!(!is_double_extension("node-v20.11.0-x64.msi"));
        assert!(!is_double_extension("archive.tar.gz"));
        assert!(!is_double_extension("notes.pdf"));
        assert!(has_rtlo("invoice\u{202E}fdp.exe"));
        assert!(!has_rtlo("invoice.pdf"));
        assert!(!has_rtlo("מסמך.pdf"));
    }

    #[test]
    fn finders_uninstall_strings() {
        let t = |s: &str| uninstall_target(s).map(|p| p.to_string_lossy().into_owned());
        assert_eq!(t(r#""C:\Program Files\Foo\unins000.exe" /SILENT"#).as_deref(), Some(r"C:\Program Files\Foo\unins000.exe"));
        assert_eq!(t(r"C:\Program Files\Foo Bar\uninstall.exe /S").as_deref(), Some(r"C:\Program Files\Foo Bar\uninstall.exe"));
        assert_eq!(t(r"C:\Program Files (x86)\X Y\uninst.exe").as_deref(), Some(r"C:\Program Files (x86)\X Y\uninst.exe"));
        assert_eq!(t(r#"rundll32.exe "C:\Program Files\X\x.dll",Uninstall"#).as_deref(), Some(r"C:\Program Files\X\x.dll"));
        assert_eq!(
            t(r#"RunDll32 C:\PROGRA~2\COMMON~1\INSTAL~1\PROFES~1\RunTime\Ctor.dll,LaunchSetup "C:\Program Files\x" -l0x9"#).as_deref(),
            Some(r"C:\PROGRA~2\COMMON~1\INSTAL~1\PROFES~1\RunTime\Ctor.dll")
        );
        assert_eq!(t(r#""C:\Windows\System32\rundll32.exe" "C:\Apps\a b.dll",Remove"#).as_deref(), Some(r"C:\Apps\a b.dll"));
        assert_eq!(t("MsiExec.exe /X{12345678-1234-1234-1234-123456789012}"), None);
        assert_eq!(t(r"C:\Windows\System32\msiexec.exe /x {12345678-1234-1234-1234-123456789012}"), None);
        assert_eq!(t("uninstall.exe"), None);
        assert_eq!(t(r#"cmd /c "C:\x\u.bat""#), None);
    }

    fn entry(hive: &str, name: &str, uninstall: &str) -> UninstEntry {
        UninstEntry {
            hive: hive.into(),
            key: format!(r"{UNINSTALL}\x"),
            wow: false,
            name: name.into(),
            publisher: String::new(),
            version: String::new(),
            install_location: String::new(),
            uninstall_string: uninstall.into(),
            system_component: false,
            is_update: false,
            estimated_kb: 0,
            strings: vec![],
        }
    }

    #[test]
    fn finders_msi_paths_and_names() {
        assert_eq!(msi_path(r"C:\?Program Files (x86)\RemoteSetup\brmfrmss.exe"), r"C:\Program Files (x86)\RemoteSetup\brmfrmss.exe");
        assert_eq!(msi_path(r"C?\Program Files\Foo\a.dll"), r"C:\Program Files\Foo\a.dll");
        assert_eq!(msi_path(r"02:\Software\Foo\Bar"), r"02:\Software\Foo\Bar");
        assert_eq!(msi_path(r"D:\Apps\x.exe"), r"D:\Apps\x.exe");
        assert_eq!(name_key("Remote Setup"), name_key("RemoteSetup"));
        assert_eq!(arg_value(r#"chrome.exe --profile-directory="Profile 1" --x"#, "--profile-directory=").as_deref(), Some("Profile 1"));
        assert_eq!(arg_value("chrome.exe --Uninstall-App-Id=abc --y", "--uninstall-app-id=").as_deref(), Some("abc"));
        assert_eq!(arg_value("chrome.exe --y", "--uninstall-app-id="), None);
    }

    #[test]
    fn finders_dead_web_apps() {
        let ud = tmp("webapps");
        fs::write(ud.join("Local State"), "{}").unwrap();
        let icons = ud.join(r"Default\Web Applications\Manifest Resources");
        let kept = "aghbiahbpaijignceidepookljebhfak";
        let gone = "efmjfjelnicpmdcmfikempdhlmainjcb";
        fs::create_dir_all(icons.join(kept)).unwrap();
        let of = |exe: &str| (exe == "chrome.exe").then(|| ud.clone());
        let cmd = |id: &str, prof: &str| format!(r#""C:\Program Files\Google\Chrome\Application\chrome.exe" --profile-directory={prof} --uninstall-app-id={id}"#);
        assert_eq!(dead_web_app(&entry("HKCU", "Chrome Remote Desktop", &cmd(gone, "Default")), &of), Some(icons.join(gone)));
        assert_eq!(dead_web_app(&entry("HKCU", "Kept", &cmd(kept, "Default")), &of), None);
        // a profile that is gone takes its apps with it
        assert_eq!(dead_web_app(&entry("HKCU", "Old", &cmd(gone, r#""Profile 9""#)), &of), Some(ud.join("Profile 9")));
        // only the current user's entries, only a known browser, only a real app id
        assert_eq!(dead_web_app(&entry("HKLM", "x", &cmd(gone, "Default")), &of), None);
        assert_eq!(dead_web_app(&entry("HKCU", "x", &cmd("short", "Default")), &of), None);
        assert_eq!(dead_web_app(&entry("HKCU", "x", &cmd(gone, r"..\..\Windows")), &of), None);
        let other = format!(r#""C:\Tools\other.exe" --uninstall-app-id={gone}"#);
        assert_eq!(dead_web_app(&entry("HKCU", "x", &other), &of), None);
        // without a Local State the folder is not the browser's, so nothing is said
        fs::remove_file(ud.join("Local State")).unwrap();
        assert_eq!(dead_web_app(&entry("HKCU", "x", &cmd(gone, "Default")), &of), None);
        let _ = fs::remove_dir_all(&ud);
    }

    #[test]
    fn finders_folder_publisher() {
        let d = tmp("publisher");
        let sys = PathBuf::from(windir()).join(r"System32\notepad.exe");
        fs::copy(&sys, d.join("notepad.exe")).unwrap();
        assert!(company_name(&sys).is_some_and(|c| c.contains("Microsoft")));
        assert!(folder_publisher(&d).contains("Microsoft"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn finders_command_targets() {
        let t = command_targets(r#"wscript.exe //B "C:\Users\u\AppData\Roaming\x\run.vbs""#);
        assert!(t.iter().any(|p| p == Path::new(r"C:\Users\u\AppData\Roaming\x\run.vbs")));
        assert!(mentions(r#""c:\program files\foo\a.exe" /s"#, r"c:\program files\foo"));
        assert!(!mentions(r"c:\program files\foobar\a.exe", r"c:\program files\foo"));
        assert!(shallow_in(r"c:\u\appdata\local\temp\x.exe", &[r"c:\u\appdata\local".into()]));
        assert!(!shallow_in(r"c:\u\appdata\local\a\b\x.exe", &[r"c:\u\appdata\local".into()]));
    }

    #[test]
    fn finders_old_file_walk() {
        let root = tmp("old");
        let dl = root.join("Downloads");
        file(&root.join("old-big.bin"), 4000, 400);
        file(&root.join("new-big.bin"), 4000, 10);
        file(&root.join("old-small.bin"), 10, 400);
        file(&root.join(r"deep\a\b\old-big2.bin"), 5000, 500);
        file(&root.join(r"proj\package.json"), 10, 400);
        file(&root.join(r"proj\old-big-in-project.bin"), 4000, 400);
        file(&root.join(r"node_modules\x\old.bin"), 4000, 400);
        file(&dl.join("setup.exe"), 600, 100);
        file(&dl.join("photos.zip"), 600, 100);
        file(&dl.join("fresh-setup.msi"), 600, 30);
        file(&dl.join("doc.pdf"), 600, 100);
        let rules = OldRules { min_days: 365, min_bytes: 1000, installer_days: 90, installer_bytes: 500 };
        let r = scan_old(&[root.clone()], Some(&dl), &rules);
        let got: HashMap<String, &str> =
            r.files.iter().map(|f| (Path::new(&f.path).file_name().unwrap().to_string_lossy().into_owned(), f.category)).collect();
        assert_eq!(got.get("old-big.bin"), Some(&"big_old"));
        assert_eq!(got.get("old-big2.bin"), Some(&"big_old"));
        assert_eq!(got.get("setup.exe"), Some(&"old_installer"));
        assert_eq!(got.get("photos.zip"), Some(&"old_archive"));
        for n in ["new-big.bin", "old-small.bin", "old-big-in-project.bin", "old.bin", "fresh-setup.msi", "doc.pdf"] {
            assert!(!got.contains_key(n), "{n} should not be listed");
        }
        assert_eq!(r.files[0].bytes, 5000);
        assert!(r.files.iter().all(|f| !f.onedrive && f.modified > 0));
        let roots = vec![root.clone()];
        assert!(check_recyclable_file(&root.join("old-big.bin").to_string_lossy(), &roots).is_ok());
        assert!(check_recyclable_file(r"C:\Windows\explorer.exe", &roots).is_err());
        assert!(check_recyclable_file(&root.to_string_lossy(), &roots).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn finders_signatures() {
        let explorer = PathBuf::from(windir()).join("explorer.exe");
        let notepad = PathBuf::from(windir()).join(r"System32\notepad.exe");
        let sys = if explorer.is_file() { explorer } else { notepad };
        assert!(matches!(check_signature(&sys), Sig::Signed(_)), "{} should verify (embedded or catalog)", sys.display());
        let mut cache = HashMap::new();
        assert!(matches!(signature(&sys, &mut cache), Sig::Signed(_)));
        let d = tmp("sig");
        let fake = d.join("fake.exe");
        let mut bytes = b"MZ".to_vec();
        bytes.extend((0..4096u32).map(|i| (i * 7 % 251) as u8));
        fs::write(&fake, bytes).unwrap();
        assert_eq!(check_signature(&fake), Sig::Unsigned);
        assert_eq!(signature(&fake, &mut cache), Sig::Unsigned);
        assert_eq!(sha256(&fake).map(|s| s.len()), Some(64));
        let _ = fs::remove_dir_all(&d);
    }

    /// Read-only run on this PC: cargo test finders_live -- --ignored --nocapture
    #[test]
    #[ignore]
    fn finders_live() {
        let t = Instant::now();
        for o in find_orphan_programs() {
            println!("{:<15} {} [{}] {} | {} | {} | {} B exe={}", o.kind, o.name, o.publisher, o.reason, o.reg_key, o.missing_file, o.bytes, o.has_exe);
        }
        println!("orphans: {:?}", t.elapsed());
        let t = Instant::now();
        let r = find_old_files(vec![], 365, 50 * MB);
        for f in r.files.iter().take(20) {
            println!("{:<13} {:>12} od={} {}", f.category, f.bytes, f.onedrive, f.path);
        }
        println!("old files: {} found, {:?} {:?}", r.total_found, r.folders, t.elapsed());
        let t = Instant::now();
        for i in find_worth_a_look() {
            println!("{:<20} {:<11} {:?} {:?} {}", i.reason, i.signature, i.signer, i.autostart, i.path);
        }
        println!("worth a look: {:?}", t.elapsed());
        let t = Instant::now();
        let d = defender_scan(&PathBuf::from(windir()).join(r"System32\notepad.exe").to_string_lossy());
        println!("defender: {} {:?} {:?}\n{}", d.verdict, d.exit_code, t.elapsed(), d.detail);
    }

    #[test]
    fn finders_sha256_known_value() {
        let d = tmp("sha");
        let f = d.join("abc.txt");
        fs::write(&f, b"abc").unwrap();
        assert_eq!(sha256(&f).as_deref(), Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
        let _ = fs::remove_dir_all(&d);
    }
}
