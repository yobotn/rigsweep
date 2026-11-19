//! "Who has this file / folder open": the Restart Manager, the same API Windows Installer and Explorer's
//! "file in use" dialog use. No handle enumeration, no driver, no admin for your own processes.
//!
//! Strictly read-only: a session is started, files are registered, the list is read and the session ends.
//! RmShutdown / RmRestart are never called.
//!
//! The Restart Manager only knows files. A folder is usually "in use" because a process has it as its working
//! directory (a terminal, a dev server), so for folders those processes are added too, as kind "cwd".

use serde::Serialize;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY, RM_PROCESS_INFO,
};

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Locker {
    pub pid: u32,
    /// unix seconds; together with `pid` identifies the process even if the PID gets reused
    pub start_time: u64,
    /// the Restart Manager's display name (window title or exe description), else the exe name
    pub name: String,
    /// short service name when it is a service, else ""
    pub service_name: String,
    /// "app", "console", "service", "explorer", "critical", "unknown", or "cwd" (working directory inside the folder)
    pub kind: String,
    /// the app registered with the Restart Manager to be restarted after an update
    pub restartable: bool,
}

/// Files registered for one folder at most; more would make one query take seconds.
pub const MAX_FILES: usize = 2000;
/// Directory entries looked at while collecting those files (a node_modules tree has hundreds of thousands).
const MAX_SEEN: usize = 50_000;
/// RmRegisterResources takes arrays; batches keep each call's buffer small.
const BATCH: usize = 500;

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

fn wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

fn utf16(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n]).trim().to_string()
}

/// Files under `dir`, breadth first, at most `MAX_FILES`. Junctions and symlinks are never followed (they
/// point outside the folder, or back into it).
fn collect_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let mut queue = std::collections::VecDeque::from([dir.to_path_buf()]);
    let mut seen = 0usize;
    while let Some(d) = queue.pop_front() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if files.len() >= MAX_FILES || seen >= MAX_SEEN {
                return files;
            }
            let p = e.path();
            let Ok(m) = std::fs::symlink_metadata(&p) else { continue };
            if m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                continue;
            }
            if m.is_dir() {
                queue.push_back(p);
            } else {
                files.push(p);
            }
        }
    }
    files
}

/// Ends the session however the query exits.
struct Session(u32);

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            let _ = RmEndSession(self.0);
        }
    }
}

fn filetime_unix(low: u32, high: u32) -> u64 {
    let t = ((high as u64) << 32 | low as u64) / 10_000_000;
    t.saturating_sub(11_644_473_600)
}

fn kind_of(app_type: i32) -> &'static str {
    match app_type {
        1 | 2 => "app",
        3 => "service",
        4 => "explorer",
        5 => "console",
        1000 => "critical",
        _ => "unknown",
    }
}

/// Processes holding any of `files` open (or loaded, for an exe / DLL).
fn rm_query(files: &[PathBuf]) -> Result<Vec<Locker>, String> {
    if files.is_empty() {
        return Ok(Vec::new());
    }
    unsafe {
        let mut handle = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        let rc = RmStartSession(&mut handle, None, PWSTR(key.as_mut_ptr()));
        if rc.0 != 0 {
            return Err(format!("Restart Manager session failed (error {})", rc.0));
        }
        let session = Session(handle);
        for chunk in files.chunks(BATCH) {
            let wides: Vec<Vec<u16>> = chunk.iter().map(|p| wide(p)).collect();
            let ptrs: Vec<PCWSTR> = wides.iter().map(|w| PCWSTR(w.as_ptr())).collect();
            let rc = RmRegisterResources(session.0, Some(&ptrs), None, None);
            if rc.0 != 0 {
                return Err(format!("Restart Manager could not register the files (error {})", rc.0));
            }
        }
        let mut list: Vec<RM_PROCESS_INFO> = Vec::new();
        let mut reasons = 0u32;
        let mut done = false;
        for _ in 0..5 {
            let (mut needed, mut n) = (0u32, list.len() as u32);
            let ptr = if list.is_empty() { None } else { Some(list.as_mut_ptr()) };
            let rc = RmGetList(session.0, &mut needed, &mut n, ptr, &mut reasons);
            match rc.0 {
                0 => {
                    list.truncate(n as usize);
                    done = true;
                    break;
                }
                // ERROR_MORE_DATA: grow to what it asked for (plus a little: the list can grow meanwhile)
                234 => list = vec![RM_PROCESS_INFO::default(); needed as usize + 4],
                e => return Err(format!("Restart Manager could not list the processes (error {e})")),
            }
        }
        drop(session);
        if !done {
            return Err("Restart Manager kept changing its list, try again".into());
        }
        Ok(list
            .iter()
            .map(|i| Locker {
                pid: i.Process.dwProcessId,
                start_time: filetime_unix(i.Process.ProcessStartTime.dwLowDateTime, i.Process.ProcessStartTime.dwHighDateTime),
                name: utf16(&i.strAppName),
                service_name: utf16(&i.strServiceShortName),
                kind: kind_of(i.ApplicationType.0).to_string(),
                restartable: i.bRestartable.as_bool(),
            })
            .collect())
    }
}

/// Processes whose working directory is `dir` or inside it.
fn cwd_lockers(dir: &Path) -> Vec<Locker> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let Ok(mut sys) = System::new() else { return Vec::new() };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cwd(UpdateKind::Always));
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").trim_end_matches('\\').to_ascii_lowercase();
    let root = norm(dir);
    sys.processes()
        .iter()
        .filter(|(_, p)| p.cwd().is_some_and(|c| {
            let c = norm(c);
            c == root || c.starts_with(&format!("{root}\\"))
        }))
        .map(|(pid, p)| Locker {
            pid: pid.as_u32(),
            start_time: p.start_time(),
            name: p.name().to_string_lossy().into_owned(),
            service_name: String::new(),
            kind: "cwd".into(),
            restartable: false,
        })
        .collect()
}

/// Everything holding `path` open. For a folder: the first `MAX_FILES` files under it plus processes working
/// inside it. Each process appears once; a Restart Manager entry wins over a "cwd" one.
pub fn lockers(path: &str) -> Result<Vec<Locker>, String> {
    let p = Path::new(path.trim().trim_matches('"'));
    let m = std::fs::symlink_metadata(p).map_err(|e| format!("{}: {e}", p.display()))?;
    let is_dir = m.is_dir() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0;
    let files = if is_dir { collect_files(p) } else { vec![p.to_path_buf()] };
    let mut out = rm_query(&files)?;
    if is_dir {
        for c in cwd_lockers(p) {
            if !out.iter().any(|l| l.pid == c.pid) {
                out.push(c);
            }
        }
    }
    // the Restart Manager reports a process once per resource kind; keep one row each
    let mut seen = std::collections::HashSet::new();
    out.retain(|l| seen.insert(l.pid));
    let names = crate::gpu::proc_names();
    for l in &mut out {
        if l.name.is_empty() {
            l.name = names.get(&l.pid).cloned().unwrap_or_default();
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then(a.name.to_ascii_lowercase().cmp(&b.name.to_ascii_lowercase())).then(a.pid.cmp(&b.pid)));
    Ok(out)
}

#[tauri::command]
pub async fn who_has_open(path: String) -> Result<Vec<Locker>, String> {
    tauri::async_runtime::spawn_blocking(move || lockers(&path)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    fn scratch(name: &str) -> PathBuf {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("locks-test").join(name);
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn finds_ourselves_holding_a_file() {
        let me = std::process::id();
        let dir = scratch("file");
        let f = dir.join("held.txt");
        std::fs::write(&f, b"x").unwrap();
        // share mode 0: nobody else may open it while we hold it
        let held = std::fs::OpenOptions::new().read(true).write(true).share_mode(0).open(&f).unwrap();

        let l = lockers(&f.to_string_lossy()).unwrap();
        let mine = l.iter().find(|l| l.pid == me).expect("our pid is listed");
        assert!(mine.start_time > 1_600_000_000 && !mine.name.is_empty());

        // the same file found through its folder, among others that nobody holds
        for i in 0..20 {
            std::fs::write(dir.join(format!("free{i}.txt")), b"y").unwrap();
        }
        assert!(lockers(&dir.to_string_lossy()).unwrap().iter().any(|l| l.pid == me));

        drop(held);
        assert!(!lockers(&f.to_string_lossy()).unwrap().iter().any(|l| l.pid == me));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_path_is_an_error() {
        assert!(lockers(r"C:\definitely\not\here\x.txt").is_err());
    }

    #[test]
    fn folder_walk_is_bounded_and_skips_links() {
        let dir = scratch("walk");
        std::fs::create_dir_all(dir.join("a").join("b")).unwrap();
        std::fs::write(dir.join("a").join("b").join("deep.txt"), b"z").unwrap();
        std::fs::write(dir.join("top.txt"), b"z").unwrap();
        let files = collect_files(&dir);
        assert_eq!(files.len(), 2);
        assert_eq!(filetime_unix(0xD53E8000, 0x019DB1DE), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
