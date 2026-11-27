// Cleanups run without admin rights, waiting for the app to record them.
//
// The app's database lives in %ProgramData%\RigSweep and only administrators may write it. The
// headless commands (`--disk-clean --safe`, the MCP tool `free_disk_space`) run without admin
// rights, so they can't add their Impact rows or History entries there. They leave them here, in
// the user's own %LOCALAPPDATA%\RigSweep\inbox, and the app (elevated, same user) takes them in the
// next time it loads the Dashboard or Dev Disk. Every row is checked again on the way in, since any
// program running as this user could write a file here.
//
// The app is elevated and this folder is the user's, so each file is opened once, without
// following a link at its name, and read and deleted through that handle, only after its real
// path is checked to be in this folder. The inbox folder itself must not be a link either. A
// swapped-in junction or symbolic link can then never point the app's delete at another file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::db::Db;
use crate::impact::Freed;

const MAX_FILES: usize = 100;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_ROWS: usize = 500;
const MAX_BYTES: u64 = 64 << 40;
const MAX_COUNT: u64 = 1_000_000;

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct Entry {
    /// what ran it: only "cli" is taken in
    pub source: String,
    /// one History line per cache emptied
    #[serde(default)]
    pub cleaned: Vec<String>,
    #[serde(default)]
    pub impact: Vec<Freed>,
}

fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).filter(|p| p.is_absolute())?;
    Some(base.join("RigSweep").join("inbox"))
}

/// Leave a cleanup for the app to record.
pub fn save(e: &Entry) -> Result<PathBuf, String> {
    let d = dir().ok_or("LOCALAPPDATA is not set")?;
    std::fs::create_dir_all(&d).map_err(|x| x.to_string())?;
    let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|t| t.as_nanos()).unwrap_or(0);
    let p = d.join(format!("impact-{n}-{}.json", std::process::id()));
    std::fs::write(&p, serde_json::to_vec(e).map_err(|x| x.to_string())?).map_err(|x| x.to_string())?;
    Ok(p)
}

/// A row as the headless commands write it: a disk cache, within sane limits, no CPU numbers.
fn valid(f: &Freed) -> bool {
    f.area == "disk"
        && f.kind.len() <= 64
        && f.kind.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b":._-".contains(&b))
        && f.bytes <= MAX_BYTES
        && f.count <= MAX_COUNT
        && f.cpu == 0.0
        && f.cpu_secs == 0
        && f.run_secs == 0
}

/// Record what one entry holds (only checked rows), as one cleanup run.
fn take(db: &Db, e: &Entry) {
    if e.source != "cli" {
        return;
    }
    for c in e.cleaned.iter().take(MAX_ROWS) {
        let c: String = c.chars().filter(|c| !c.is_control()).take(300).collect();
        db.log("cli", None, "disk_clean", true, &c);
    }
    let rows: Vec<Freed> = e.impact.iter().take(MAX_ROWS).filter(|f| valid(f)).cloned().collect();
    db.add_impact("cli", &rows);
}

/// Take in every waiting cleanup and remove its file; a file that can't be read is removed too.
pub fn ingest(db: &Db) -> usize {
    let Some(d) = dir() else { return 0 };
    // %LOCALAPPDATA%\RigSweep\inbox must really be there, with no link on the way
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).and_then(|b| std::fs::canonicalize(b).ok());
    let real = std::fs::canonicalize(&d).ok();
    match (base, real) {
        (Some(b), Some(r)) if same(&b.join("RigSweep").join("inbox"), &r) => ingest_from(db, &d),
        _ => 0,
    }
}

fn same(a: &Path, b: &Path) -> bool {
    a.as_os_str().to_string_lossy().eq_ignore_ascii_case(&b.as_os_str().to_string_lossy())
}

/// The path Windows has for an open file (\\?\C:\...), after every link and junction.
fn final_path(f: &std::fs::File) -> Option<PathBuf> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED};
    let mut buf = vec![0u16; 1024];
    let n = unsafe { GetFinalPathNameByHandleW(HANDLE(f.as_raw_handle()), &mut buf, FILE_NAME_NORMALIZED) } as usize;
    (n > 0 && n < buf.len()).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..n])))
}

/// Open one inbox file, check it, read it and delete it, all through one handle. None: it was not
/// removed (left for next time, or not ours to touch); Some(None): removed, but unreadable.
fn take_file(real_dir: &Path, p: &Path) -> Option<Option<Entry>> {
    use std::io::Read;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO};
    const GENERIC_READ: u32 = 0x8000_0000;
    const DELETE: u32 = 0x0001_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    let mut f = std::fs::OpenOptions::new()
        .access_mode(GENERIC_READ | DELETE)
        .share_mode(0) // nobody swaps or writes it while it is read
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(p)
        .ok()?;
    let m = f.metadata().ok()?;
    if !m.is_file() || m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return None;
    }
    let name = p.file_name()?;
    if !final_path(&f).is_some_and(|fp| same(&fp, &real_dir.join(name))) {
        return None;
    }
    let e = if m.len() <= MAX_FILE_BYTES {
        let mut b = vec![];
        f.by_ref().take(MAX_FILE_BYTES).read_to_end(&mut b).ok().and_then(|_| serde_json::from_slice(&b).ok())
    } else {
        None
    };
    // remove first: a file that can't be removed is never counted twice
    let info = FILE_DISPOSITION_INFO { DeleteFile: true };
    let ok = unsafe {
        SetFileInformationByHandle(
            HANDLE(f.as_raw_handle()),
            FileDispositionInfo,
            &info as *const _ as *const _,
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    ok.is_ok().then_some(e)
}

fn ingest_from(db: &Db, d: &Path) -> usize {
    let Ok(real_dir) = std::fs::canonicalize(d) else { return 0 };
    let Ok(rd) = std::fs::read_dir(d) else { return 0 };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("impact-") && n.ends_with(".json")))
        .collect();
    files.sort();
    let mut n = 0;
    for p in files.into_iter().take(MAX_FILES) {
        let Some(e) = take_file(&real_dir, &p) else { continue };
        if let Some(e) = e {
            take(db, &e);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_waiting_cleanup_is_recorded_once_and_bad_rows_are_dropped() {
        let d = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("inbox-test");
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let good = Entry { source: "cli".into(), cleaned: vec!["npm cache: 1.0 GB".into()], impact: vec![Freed::new("disk", "cache:npm", 1 << 30, 1)] };
        std::fs::write(d.join("impact-1.json"), serde_json::to_vec(&good).unwrap()).unwrap();
        let bad = Entry {
            source: "cli".into(),
            cleaned: vec![],
            impact: vec![Freed::new("memory", "mcp", 5, 1), Freed::new("disk", "cache:<b>", 5, 1), Freed::new("disk", "cache:pip", MAX_BYTES + 1, 1), Freed::new("disk", "cache:uv", 7, 1)],
        };
        std::fs::write(d.join("impact-2.json"), serde_json::to_vec(&bad).unwrap()).unwrap();
        std::fs::write(d.join("impact-3.json"), serde_json::to_vec(&Entry { source: "gui".into(), ..Default::default() }).unwrap()).unwrap();
        std::fs::write(d.join("impact-4.json"), b"not json").unwrap();
        std::fs::write(d.join("other.txt"), b"left alone").unwrap();
        let db = Db::open_in_memory().unwrap();
        assert_eq!(ingest_from(&db, &d), 3);
        let t = crate::impact::total(&db.impact_rows(None).unwrap());
        assert_eq!((t.disk, t.memory, t.cleanups), ((1 << 30) + 7, 0, 2));
        assert_eq!(db.history(10).unwrap().len(), 1);
        // the files are gone, so nothing is counted twice; other files are left alone
        assert_eq!(ingest_from(&db, &d), 0);
        assert!(d.join("other.txt").exists());
    }

    #[test]
    fn a_link_in_the_inbox_is_never_followed_or_deleted() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("inbox-link-test");
        let _ = std::fs::remove_dir_all(&root);
        let (d, outside) = (root.join("inbox"), root.join("outside"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        let good = Entry { source: "cli".into(), impact: vec![Freed::new("disk", "cache:npm", 5, 1)], ..Default::default() };
        let victim = outside.join("victim.json");
        std::fs::write(&victim, serde_json::to_vec(&good).unwrap()).unwrap();
        let db = Db::open_in_memory().unwrap();
        // a file symlink needs Developer Mode or admin rights; without them this part can't run
        if std::os::windows::fs::symlink_file(&victim, d.join("impact-1.json")).is_ok() {
            assert_eq!(ingest_from(&db, &d), 0);
            assert!(victim.exists(), "the link's target must never be deleted");
        }
        // a junction: a file reached through it has a real path outside the folder that was checked
        let j = d.join("sub");
        let made = std::process::Command::new("cmd.exe").args(["/d", "/c", "mklink", "/J"]).arg(&j).arg(&outside).output().is_ok_and(|o| o.status.success());
        assert!(made, "mklink /J needs no special rights");
        std::fs::write(outside.join("impact-2.json"), serde_json::to_vec(&good).unwrap()).unwrap();
        assert!(take_file(&std::fs::canonicalize(&d).unwrap().join("sub"), &j.join("impact-2.json")).is_none());
        assert!(outside.join("impact-2.json").exists());
        let _ = std::fs::remove_dir(&j);
        let _ = std::fs::remove_dir_all(&root);
    }
}
