// Space freed with the AI's help.
//
// When RigSweep asks the user's AI about a folder (a git worktree, a program leftover), the user or
// the agent may remove it outside RigSweep. RigSweep remembers the folder and the size it measured
// ("hand-off"). Later, when the folder is gone and RigSweep didn't remove it itself, that size is
// credited to Impact under its own area, "ai". An agent can say which paths it removed (MCP
// `report_cleanup`) but never how much: the size is always RigSweep's own measurement, and a path
// counts only if it was handed off and is really gone. These rows stay out of the anonymous stats.

use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::db::Db;
use crate::impact::Freed;

pub const KINDS: &[&str] = &["worktree", "leftover"];
/// a hand-off older than this is dropped without credit: too long ago to say the AI helped
const MAX_AGE: i64 = 30 * 86400;
const MAX_ITEMS: usize = 200;
const MAX_BYTES: u64 = 64 << 40;

#[derive(Deserialize, Clone, Debug)]
pub struct Item {
    pub path: String,
    pub kind: String,
    pub bytes: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Settled {
    pub path: String,
    /// credited | still_there | unknown
    pub state: &'static str,
    pub bytes: u64,
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Gone for good: nothing at that path any more, while the folder it was in is still there (an
/// unreadable path, or one on an unplugged drive or an offline share, doesn't count as gone).
fn gone(p: &str) -> bool {
    let parent_there = Path::new(p).parent().is_some_and(|d| d.is_dir());
    parent_there && matches!(std::fs::symlink_metadata(p), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// Which of the hand-offs are gone, looked at without the database lock held: a path on a slow or
/// offline share can take seconds, and every other command waits on that lock.
pub fn gone_among(db: &std::sync::Mutex<Db>) -> Vec<String> {
    let paths: Vec<String> = match db.lock() {
        Ok(d) => d.handoffs().unwrap_or_default().into_iter().map(|h| h.0).collect(),
        Err(_) => return vec![],
    };
    paths.into_iter().filter(|p| gone(p)).collect()
}

/// `settle(db, None)` with the paths `gone_among` found gone (a hand-off added since isn't gone).
pub fn settle_known(db: &Db, gone_paths: &[String]) {
    settle_with(db, None, now(), &|p| gone_paths.iter().any(|g| g == p));
}

/// Remember folders handed to an AI. Only existing absolute folders of a known kind are kept.
pub fn add(db: &Db, items: &[Item]) -> Result<usize, String> {
    if items.len() > MAX_ITEMS {
        return Err(format!("at most {MAX_ITEMS} folders at a time"));
    }
    let t = now();
    let mut n = 0;
    for i in items {
        let p = Path::new(&i.path);
        if !KINDS.contains(&i.kind.as_str()) || !p.is_absolute() || !p.is_dir() || i.bytes > MAX_BYTES {
            continue;
        }
        db.add_handoff(&i.path, &i.kind, i.bytes, t).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

/// Credit the handed-off folders that are gone. `only`: just these paths (what an agent reported),
/// each answered; None: every hand-off (a scan or the Dashboard), answering only what was credited.
pub fn settle(db: &Db, only: Option<&[String]>) -> Vec<Settled> {
    settle_at(db, only, now())
}

fn settle_at(db: &Db, only: Option<&[String]>, t: i64) -> Vec<Settled> {
    settle_with(db, only, t, &gone)
}

fn settle_with(db: &Db, only: Option<&[String]>, t: i64, gone: &dyn Fn(&str) -> bool) -> Vec<Settled> {
    let all = db.handoffs().unwrap_or_default();
    let (mut out, mut drop, mut freed) = (vec![], vec![], vec![]);
    let wanted = |p: &str| only.is_none_or(|o| o.iter().any(|x| x.eq_ignore_ascii_case(p)));
    for (path, kind, bytes, ts) in &all {
        if !wanted(path) {
            continue;
        }
        if gone(path) {
            freed.push(Freed::new("ai", format!("ai:{kind}"), *bytes, 1));
            out.push(Settled { path: path.clone(), state: "credited", bytes: *bytes });
            drop.push(path.clone());
        } else if t - ts > MAX_AGE {
            drop.push(path.clone());
        } else if only.is_some() {
            out.push(Settled { path: path.clone(), state: "still_there", bytes: 0 });
        }
    }
    if let Some(o) = only {
        for p in o {
            if !all.iter().any(|(x, ..)| x.eq_ignore_ascii_case(p)) {
                out.push(Settled { path: p.clone(), state: "unknown", bytes: 0 });
            }
        }
    }
    db.add_impact("ai", &freed);
    db.drop_handoffs(&drop);
    out
}

/// What an agent's `report_cleanup` gets back. The MCP server usually runs without admin rights and
/// can't write RigSweep's database (in %ProgramData%); then it only says which paths are gone, and
/// the app credits them itself (when they were handed off) at its next Dev Disk scan or Dashboard load.
pub fn report(paths: &[String]) -> serde_json::Value {
    match Db::open() {
        Ok(db) if db.writable() && db.handoffs().is_ok() => {
            let r = settle(&db, Some(paths));
            serde_json::json!({ "credited_bytes": r.iter().map(|s| s.bytes).sum::<u64>(), "paths": r })
        }
        _ => {
            let r: Vec<serde_json::Value> = paths
                .iter()
                .map(|p| serde_json::json!({ "path": p, "state": if gone(p) { "gone" } else { "still_there" } }))
                .collect();
            serde_json::json!({
                "paths": r,
                "note": "RigSweep counts each gone folder it had asked you about the next time it opens Dev Disk or the Dashboard, with the size it measured. Nothing else is needed.",
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> String {
        let d = Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join(format!("handoff-test-{name}"));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn only_a_handed_off_folder_that_is_gone_is_credited_with_its_measured_size() {
        let db = Db::open_in_memory().unwrap();
        let (a, b) = (dir("a"), dir("b"));
        let items = [
            Item { path: a.clone(), kind: "worktree".into(), bytes: 1000 },
            Item { path: b.clone(), kind: "leftover".into(), bytes: 500 },
            Item { path: "relative".into(), kind: "worktree".into(), bytes: 9 },
            Item { path: a.clone(), kind: "secrets".into(), bytes: 9 },
        ];
        assert_eq!(add(&db, &items).unwrap(), 2);
        std::fs::remove_dir_all(&a).unwrap();
        // an agent reports both, plus one RigSweep never handed off
        let r = settle(&db, Some(&[a.to_uppercase(), b.clone(), "C:\\nope".into()]));
        assert!(r.contains(&Settled { path: a.clone(), state: "credited", bytes: 1000 }));
        assert!(r.contains(&Settled { path: b.clone(), state: "still_there", bytes: 0 }));
        assert!(r.contains(&Settled { path: "C:\\nope".into(), state: "unknown", bytes: 0 }));
        let t = crate::impact::total(&db.impact_rows(None).unwrap());
        assert_eq!((t.ai, t.disk, t.cleanups), (1000, 0, 0));
        // credited once only, and never sent with the anonymous stats
        std::fs::remove_dir_all(&b).unwrap();
        assert_eq!(settle(&db, None), vec![Settled { path: b.clone(), state: "credited", bytes: 500 }]);
        assert!(settle(&db, None).is_empty());
        assert!(db.impact_events_after(0, 100).unwrap().is_empty());
    }

    #[test]
    fn an_old_hand_off_expires_and_rigsweep_own_removal_is_not_credited() {
        let db = Db::open_in_memory().unwrap();
        let (a, b) = (dir("old"), dir("own"));
        add(&db, &[Item { path: a.clone(), kind: "worktree".into(), bytes: 10 }, Item { path: b.clone(), kind: "worktree".into(), bytes: 20 }]).unwrap();
        assert!(settle_at(&db, None, now() + MAX_AGE + 1).iter().all(|s| s.state != "credited"));
        assert!(db.handoffs().unwrap().is_empty());
        add(&db, &[Item { path: b.clone(), kind: "worktree".into(), bytes: 20 }]).unwrap();
        db.drop_handoffs(&[b.clone()]); // RigSweep is removing it itself
        std::fs::remove_dir_all(&b).unwrap();
        assert!(settle(&db, None).is_empty());
        let _ = std::fs::remove_dir_all(&a);
    }

    #[test]
    fn a_folder_on_a_drive_that_is_gone_is_not_credited() {
        let db = Db::open_in_memory().unwrap();
        let a = dir("offline");
        let inner = Path::new(&a).join("wt").to_string_lossy().into_owned();
        std::fs::create_dir_all(&inner).unwrap();
        add(&db, &[Item { path: inner.clone(), kind: "worktree".into(), bytes: 7 }]).unwrap();
        // the drive or share went away: the folder it was in is missing too
        std::fs::remove_dir_all(&a).unwrap();
        assert!(!gone(&inner));
        assert!(settle(&db, None).is_empty());
        // looked at outside the lock, then settled with what was found
        std::fs::create_dir_all(&a).unwrap();
        let m = std::sync::Mutex::new(db);
        let g = gone_among(&m);
        assert_eq!(g, vec![inner.clone()]);
        settle_known(&m.lock().unwrap(), &g);
        assert!(m.lock().unwrap().handoffs().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&a);
    }
}
