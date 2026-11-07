use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;

use crate::catalog::Tweak;

pub struct Db {
    conn: Connection,
}

#[derive(Serialize, Clone, Debug)]
pub struct HistoryRow {
    pub id: i64,
    pub ts: String,
    pub source: String,
    pub tweak_id: Option<String>,
    pub action: String,
    pub result: String,
    pub detail: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SnapshotRow {
    pub id: i64,
    pub ts: String,
    pub label: String,
    pub metrics: Value,
}

#[derive(Clone, Debug)]
pub struct Backup {
    pub action_idx: usize,
    pub original: Value,
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    PathBuf::from(base).join("RigSweep")
}

pub fn db_path() -> PathBuf {
    data_dir().join("rigsweep.db")
}

/// One-time move of the pre-rename data (%ProgramData%\aiBoost\aiboost.db) so backups of
/// original values are never lost. The old folder is left in place as a safety copy.
pub fn migrate_legacy() {
    let new = db_path();
    if new.exists() {
        return;
    }
    let base = std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let old = PathBuf::from(base).join("aiBoost").join("aiboost.db");
    if old.exists() && std::fs::create_dir_all(data_dir()).is_ok() {
        // checkpoint-free copy: include WAL side files if present
        let _ = std::fs::copy(&old, &new);
        for ext in ["-wal", "-shm"] {
            let o = PathBuf::from(format!("{}{ext}", old.display()));
            if o.exists() {
                let _ = std::fs::copy(&o, PathBuf::from(format!("{}{ext}", new.display())));
            }
        }
    }
}

/// Keep the last 5 copies of the database: rigsweep.db.bak.1 (newest) .. .bak.5
pub fn rotate_backups() {
    let p = db_path();
    if !p.exists() {
        return;
    }
    let bak = |n: u32| PathBuf::from(format!("{}.bak.{n}", p.display()));
    let _ = std::fs::remove_file(bak(5));
    for n in (1..5).rev() {
        let _ = std::fs::rename(bak(n), bak(n + 1));
    }
    let _ = std::fs::copy(&p, bak(1));
}

fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

impl Db {
    pub fn open() -> Result<Db> {
        std::fs::create_dir_all(data_dir()).context("create data dir")?;
        let conn = Connection::open(db_path()).context("open sqlite db")?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.busy_timeout(std::time::Duration::from_secs(10))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS profile(
                tweak_id TEXT PRIMARY KEY, enabled INTEGER NOT NULL, updated_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS backups(
                id INTEGER PRIMARY KEY AUTOINCREMENT, tweak_id TEXT NOT NULL, action_idx INTEGER NOT NULL,
                target TEXT NOT NULL, original_json TEXT NOT NULL, captured_at TEXT NOT NULL,
                UNIQUE(tweak_id, action_idx));
             CREATE TABLE IF NOT EXISTS history(
                id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, source TEXT NOT NULL,
                tweak_id TEXT, action TEXT NOT NULL, result TEXT NOT NULL, detail TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS custom_tweaks(id TEXT PRIMARY KEY, json TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS snapshots(
                id INTEGER PRIMARY KEY AUTOINCREMENT, ts TEXT NOT NULL, label TEXT NOT NULL, metrics_json TEXT NOT NULL);",
        )?;
        Ok(Db { conn })
    }

    // ---------- profile ----------
    pub fn set_enabled(&self, tweak_id: &str, enabled: bool) -> Result<()> {
        self.conn.execute(
            "INSERT INTO profile(tweak_id, enabled, updated_at) VALUES(?1, ?2, ?3)
             ON CONFLICT(tweak_id) DO UPDATE SET enabled=?2, updated_at=?3",
            params![tweak_id, enabled as i64, now()],
        )?;
        Ok(())
    }

    pub fn enabled_ids(&self) -> Result<Vec<String>> {
        let mut st = self.conn.prepare("SELECT tweak_id FROM profile WHERE enabled=1 ORDER BY tweak_id")?;
        let rows = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    // ---------- backups ----------
    /// Store the original value only once: later re-applies never overwrite the stock value.
    pub fn backup_once(&self, tweak_id: &str, idx: usize, target: &str, original: &Value) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO backups(tweak_id, action_idx, target, original_json, captured_at)
             VALUES(?1, ?2, ?3, ?4, ?5)",
            params![tweak_id, idx as i64, target, original.to_string(), now()],
        )?;
        Ok(())
    }

    pub fn has_backup(&self, tweak_id: &str, idx: usize) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM backups WHERE tweak_id=?1 AND action_idx=?2",
                params![tweak_id, idx as i64],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn backups_for(&self, tweak_id: &str) -> Result<Vec<Backup>> {
        let mut st = self.conn.prepare(
            "SELECT action_idx, original_json FROM backups WHERE tweak_id=?1 ORDER BY action_idx DESC",
        )?;
        let rows = st
            .query_map(params![tweak_id], |r| {
                let idx: i64 = r.get(0)?;
                let js: String = r.get(1)?;
                Ok((idx, js))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows
            .into_iter()
            .map(|(idx, js)| Backup {
                action_idx: idx as usize,
                original: serde_json::from_str(&js).unwrap_or(Value::Null),
            })
            .collect())
    }

    pub fn delete_backup(&self, tweak_id: &str, idx: usize) -> Result<()> {
        self.conn.execute(
            "DELETE FROM backups WHERE tweak_id=?1 AND action_idx=?2",
            params![tweak_id, idx as i64],
        )?;
        Ok(())
    }

    pub fn tweaks_with_backups(&self) -> Result<Vec<String>> {
        // newest first, so restore-all undoes changes in reverse order
        let mut st = self
            .conn
            .prepare("SELECT tweak_id FROM backups GROUP BY tweak_id ORDER BY MAX(id) DESC")?;
        let rows = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
        Ok(rows)
    }

    pub fn backup_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM backups", [], |r| r.get(0))?)
    }

    // ---------- history ----------
    pub fn log(&self, source: &str, tweak_id: Option<&str>, action: &str, ok: bool, detail: &str) {
        let _ = self.conn.execute(
            "INSERT INTO history(ts, source, tweak_id, action, result, detail) VALUES(?1,?2,?3,?4,?5,?6)",
            params![now(), source, tweak_id, action, if ok { "ok" } else { "error" }, detail],
        );
    }

    pub fn history(&self, limit: i64) -> Result<Vec<HistoryRow>> {
        let mut st = self.conn.prepare(
            "SELECT id, ts, source, tweak_id, action, result, detail FROM history ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = st
            .query_map(params![limit], |r| {
                Ok(HistoryRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    source: r.get(2)?,
                    tweak_id: r.get(3)?,
                    action: r.get(4)?,
                    result: r.get(5)?,
                    detail: r.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// How many times Windows reverted each tweak (and we re-applied it).
    pub fn drift_counts(&self) -> Result<std::collections::HashMap<String, i64>> {
        let mut st = self.conn.prepare(
            "SELECT tweak_id, COUNT(*) FROM history WHERE action='drift' AND tweak_id IS NOT NULL GROUP BY tweak_id",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    // ---------- custom tweaks ----------
    pub fn custom_tweaks(&self) -> Result<Vec<Tweak>> {
        let mut st = self.conn.prepare("SELECT json FROM custom_tweaks ORDER BY id")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows.iter().filter_map(|j| serde_json::from_str(j).ok()).collect())
    }

    pub fn add_custom(&self, t: &Tweak) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO custom_tweaks(id, json) VALUES(?1, ?2)",
            params![t.id, serde_json::to_string(t)?],
        )?;
        Ok(())
    }

    pub fn remove_custom(&self, id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM custom_tweaks WHERE id=?1", params![id])?;
        self.conn.execute("DELETE FROM profile WHERE tweak_id=?1", params![id])?;
        Ok(())
    }

    // ---------- boost report snapshots ----------
    pub fn add_snapshot(&self, label: &str, metrics: &Value) -> Result<SnapshotRow> {
        let ts = now();
        self.conn.execute(
            "INSERT INTO snapshots(ts, label, metrics_json) VALUES(?1, ?2, ?3)",
            params![ts, label, metrics.to_string()],
        )?;
        Ok(SnapshotRow { id: self.conn.last_insert_rowid(), ts, label: label.into(), metrics: metrics.clone() })
    }

    pub fn snapshots(&self) -> Result<Vec<SnapshotRow>> {
        let mut st = self.conn.prepare("SELECT id, ts, label, metrics_json FROM snapshots ORDER BY id")?;
        let rows = st
            .query_map([], |r| {
                let js: String = r.get(3)?;
                Ok(SnapshotRow {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    label: r.get(2)?,
                    metrics: serde_json::from_str(&js).unwrap_or(Value::Null),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn has_snapshot(&self, label: &str) -> bool {
        self.conn
            .query_row("SELECT 1 FROM snapshots WHERE label=?1", params![label], |_| Ok(()))
            .optional()
            .ok()
            .flatten()
            .is_some()
    }

    pub fn clear_snapshots(&self) -> Result<()> {
        self.conn.execute("DELETE FROM snapshots", [])?;
        Ok(())
    }

    // ---------- settings ----------
    pub fn get_setting(&self, key: &str) -> Option<String> {
        self.conn
            .query_row("SELECT value FROM settings WHERE key=?1", params![key], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=?2",
            params![key, value],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backup_is_captured_only_once() {
        let db = Db::open_in_memory().unwrap();
        db.backup_once("t", 0, "x", &json!({"start": 2})).unwrap();
        db.backup_once("t", 0, "x", &json!({"start": 4})).unwrap();
        let b = db.backups_for("t").unwrap();
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].original, json!({"start": 2}));
        assert!(db.has_backup("t", 0).unwrap());
        db.delete_backup("t", 0).unwrap();
        assert!(!db.has_backup("t", 0).unwrap());
    }

    #[test]
    fn profile_and_drift() {
        let db = Db::open_in_memory().unwrap();
        db.set_enabled("a", true).unwrap();
        db.set_enabled("b", true).unwrap();
        db.set_enabled("b", false).unwrap();
        assert_eq!(db.enabled_ids().unwrap(), vec!["a".to_string()]);
        db.log("watchdog", Some("a"), "drift", true, "reverted");
        db.log("watchdog", Some("a"), "drift", true, "reverted");
        assert_eq!(db.drift_counts().unwrap().get("a"), Some(&2));
    }

    #[test]
    fn snapshots_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        assert!(!db.has_snapshot("baseline"));
        db.add_snapshot("baseline", &json!({"mem_used": 10})).unwrap();
        db.add_snapshot("now", &json!({"mem_used": 7})).unwrap();
        let s = db.snapshots().unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].metrics["mem_used"], 10);
        assert!(db.has_snapshot("baseline"));
        db.clear_snapshots().unwrap();
        assert!(db.snapshots().unwrap().is_empty());
    }
}
