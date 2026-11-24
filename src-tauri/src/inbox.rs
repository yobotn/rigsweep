// Cleanups run without admin rights, waiting for the app to record them.
//
// The app's database lives in %ProgramData%\RigSweep and only administrators may write it. The
// headless commands (`--disk-clean --safe`, the MCP tool `free_disk_space`) run without admin
// rights, so they can't add their Impact rows or History entries there. They leave them here, in
// the user's own %LOCALAPPDATA%\RigSweep\inbox, and the app (elevated, same user) takes them in the
// next time it loads the Dashboard or Dev Disk. Every row is checked again on the way in, since any
// program running as this user could write a file here.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    ingest_from(db, &d)
}

fn ingest_from(db: &Db, d: &std::path::Path) -> usize {
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
        let small = std::fs::metadata(&p).is_ok_and(|m| m.len() <= MAX_FILE_BYTES);
        let e: Option<Entry> = if small { std::fs::read(&p).ok().and_then(|b| serde_json::from_slice(&b).ok()) } else { None };
        // remove first: a file that can't be removed is never counted twice
        if std::fs::remove_file(&p).is_err() {
            continue;
        }
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
}
