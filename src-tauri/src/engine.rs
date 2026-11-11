use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::actions::{self, Ctx, Status};
use crate::catalog::{Catalog, Tweak};
use crate::db::Db;
use crate::util;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    pub start_with_windows: bool,
    pub stay_in_tray: bool,
    pub watchdog_minutes: u32,
    pub dev_folders: Vec<String>,
    /// Dev Cleanup: a dev process tree with no CPU activity for this many minutes counts as idle
    #[serde(default = "default_idle_minutes")]
    pub idle_minutes: u32,
    /// Dev Cleanup: end stale dev process trees automatically in the background
    #[serde(default)]
    pub auto_kill_stale: bool,
    /// tray / taskbar dot showing how much memory stale dev processes hold
    #[serde(default = "yes")]
    pub badge_enabled: bool,
    /// orange dot from this many GB
    #[serde(default = "default_warn_gb")]
    pub badge_warn_gb: f32,
    /// red dot from this many GB
    #[serde(default = "default_alert_gb")]
    pub badge_alert_gb: f32,
    /// check GitHub for a new version at startup and once a day (off unless the user turns it on)
    #[serde(default)]
    pub auto_update: bool,
    /// Dev Disk: folders scanned for projects
    #[serde(default = "crate::devdisk::default_roots")]
    pub disk_roots: Vec<String>,
    /// Dev Disk: never scanned (full paths or folder-name patterns), on top of the built-in list
    #[serde(default)]
    pub disk_excludes: Vec<String>,
    /// Dev Disk: package caches marked "never clean"
    #[serde(default = "crate::devdisk::default_disabled")]
    pub disk_disabled: Vec<String>,
    /// Dev Disk: suggest projects nobody has edited for this many days
    #[serde(default = "crate::devdisk::default_stale_days")]
    pub disk_stale_days: u32,
    /// Dev Disk auto-select: tick build output (dist, target, .next…) of stale projects
    #[serde(default = "yes")]
    pub disk_auto_build: bool,
    /// Dev Disk auto-select: tick dependency folders (node_modules…) of stale projects
    #[serde(default = "yes")]
    pub disk_auto_deps: bool,
    /// Dev Disk auto-select: tick package caches that rebuild themselves
    #[serde(default = "yes")]
    pub disk_auto_caches: bool,
    /// Dev Disk auto-select: skip folders smaller than this
    #[serde(default = "default_auto_min_mb")]
    pub disk_auto_min_mb: u32,
}

fn yes() -> bool {
    true
}
fn default_auto_min_mb() -> u32 {
    50
}
fn default_warn_gb() -> f32 {
    1.0
}
fn default_alert_gb() -> f32 {
    2.0
}

fn json_list(db: &Db, key: &str) -> Option<Vec<String>> {
    db.get_setting(key).and_then(|s| serde_json::from_str(&s).ok())
}

fn default_idle_minutes() -> u32 {
    120
}

impl Settings {
    pub fn load(db: &Db) -> Settings {
        let dev_folders = db
            .get_setting("dev_folders")
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_else(|| {
                ["%USERPROFILE%\\.cargo", "%USERPROFILE%\\.rustup", "%LOCALAPPDATA%\\npm-cache"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            });
        Settings {
            start_with_windows: db.get_setting("start_with_windows").as_deref() == Some("1"),
            stay_in_tray: db.get_setting("stay_in_tray").as_deref() != Some("0"),
            watchdog_minutes: db.get_setting("watchdog_minutes").and_then(|s| s.parse().ok()).unwrap_or(30),
            dev_folders,
            // older versions stored hours
            idle_minutes: db
                .get_setting("idle_minutes")
                .and_then(|s| s.parse().ok())
                .or_else(|| db.get_setting("idle_hours").and_then(|s| s.parse::<u32>().ok()).map(|h| h * 60))
                .unwrap_or(default_idle_minutes()),
            auto_kill_stale: db.get_setting("auto_kill_stale").as_deref() == Some("1"),
            badge_enabled: db.get_setting("badge_enabled").as_deref() != Some("0"),
            badge_warn_gb: db.get_setting("badge_warn_gb").and_then(|s| s.parse().ok()).unwrap_or(default_warn_gb()),
            badge_alert_gb: db.get_setting("badge_alert_gb").and_then(|s| s.parse().ok()).unwrap_or(default_alert_gb()),
            auto_update: db.get_setting("auto_update").as_deref() == Some("1"),
            disk_roots: json_list(db, "disk_roots").unwrap_or_else(crate::devdisk::default_roots),
            disk_excludes: json_list(db, "disk_excludes").unwrap_or_default(),
            disk_disabled: json_list(db, "disk_disabled").unwrap_or_else(crate::devdisk::default_disabled),
            disk_stale_days: db
                .get_setting("disk_stale_days")
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(crate::devdisk::default_stale_days),
            disk_auto_build: db.get_setting("disk_auto_build").as_deref() != Some("0"),
            disk_auto_deps: db.get_setting("disk_auto_deps").as_deref() != Some("0"),
            disk_auto_caches: db.get_setting("disk_auto_caches").as_deref() != Some("0"),
            disk_auto_min_mb: db.get_setting("disk_auto_min_mb").and_then(|s| s.parse().ok()).unwrap_or(default_auto_min_mb()),
        }
    }

    pub fn save(&self, db: &Db) -> Result<()> {
        db.set_setting("start_with_windows", if self.start_with_windows { "1" } else { "0" })?;
        db.set_setting("stay_in_tray", if self.stay_in_tray { "1" } else { "0" })?;
        db.set_setting("watchdog_minutes", &self.watchdog_minutes.max(1).to_string())?;
        db.set_setting("dev_folders", &serde_json::to_string(&self.dev_folders)?)?;
        db.set_setting("idle_minutes", &self.idle_minutes.clamp(1, 10080).to_string())?;
        db.set_setting("auto_kill_stale", if self.auto_kill_stale { "1" } else { "0" })?;
        db.set_setting("badge_enabled", if self.badge_enabled { "1" } else { "0" })?;
        let warn = if self.badge_warn_gb.is_finite() { self.badge_warn_gb.clamp(0.1, 256.0) } else { default_warn_gb() };
        let alert = if self.badge_alert_gb.is_finite() { self.badge_alert_gb.clamp(warn, 256.0) } else { default_alert_gb().max(warn) };
        db.set_setting("badge_warn_gb", &warn.to_string())?;
        db.set_setting("badge_alert_gb", &alert.to_string())?;
        db.set_setting("auto_update", if self.auto_update { "1" } else { "0" })?;
        let clean = |v: &[String]| v.iter().map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect::<Vec<_>>();
        db.set_setting("disk_roots", &serde_json::to_string(&clean(&self.disk_roots))?)?;
        db.set_setting("disk_excludes", &serde_json::to_string(&clean(&self.disk_excludes))?)?;
        db.set_setting("disk_disabled", &serde_json::to_string(&clean(&self.disk_disabled))?)?;
        db.set_setting("disk_stale_days", &self.disk_stale_days.clamp(1, 3650).to_string())?;
        db.set_setting("disk_auto_build", if self.disk_auto_build { "1" } else { "0" })?;
        db.set_setting("disk_auto_deps", if self.disk_auto_deps { "1" } else { "0" })?;
        db.set_setting("disk_auto_caches", if self.disk_auto_caches { "1" } else { "0" })?;
        db.set_setting("disk_auto_min_mb", &self.disk_auto_min_mb.min(100_000).to_string())?;
        Ok(())
    }
}

pub fn make_ctx(db: &Db) -> Ctx {
    let s = Settings::load(db);
    Ctx::new(s.dev_folders, db.get_setting("power_dup_guid"))
}

pub fn load_catalog(db: &Db) -> Catalog {
    Catalog::with_custom(db.custom_tweaks().unwrap_or_default())
}

#[derive(Serialize, Clone, Debug)]
pub struct ActionView {
    pub target: String,
    pub status: Status,
}

#[derive(Serialize, Clone, Debug)]
pub struct TweakStatus {
    /// applied | partial | not_applied | not_present | error
    pub overall: &'static str,
    pub actions: Vec<ActionView>,
}

pub fn tweak_status(t: &Tweak, ctx: &Ctx) -> TweakStatus {
    let actions: Vec<ActionView> = t
        .actions
        .iter()
        .map(|a| ActionView { target: a.target(), status: actions::status(a, ctx) })
        .collect();
    let relevant: Vec<&Status> = actions.iter().map(|a| &a.status).filter(|s| **s != Status::NotPresent).collect();
    let overall = if relevant.is_empty() {
        "not_present"
    } else if relevant.iter().all(|s| **s == Status::Applied) {
        "applied"
    } else if relevant.iter().any(|s| matches!(s, Status::Error(_))) {
        "error"
    } else if relevant.iter().any(|s| **s == Status::Applied) {
        "partial"
    } else {
        "not_applied"
    };
    TweakStatus { overall, actions }
}

#[derive(Serialize, Clone, Debug)]
pub struct OpResult {
    pub tweak_id: String,
    pub ok: bool,
    pub messages: Vec<String>,
}

/// Back up originals, then apply every action of a tweak that isn't applied yet.
pub fn apply_tweak(db: &Db, t: &Tweak, ctx: &mut Ctx, source: &str) -> OpResult {
    let mut messages = vec![];
    let mut ok = true;
    for (i, a) in t.actions.iter().enumerate() {
        let target = a.target();
        match actions::status(a, ctx) {
            Status::Applied => continue,
            Status::NotPresent => {
                messages.push(format!("skipped (not present on this PC): {target}"));
                continue;
            }
            _ => {}
        }
        // Never change anything we can't restore.
        let mut fresh = false;
        if !db.has_backup(&t.id, i).unwrap_or(false) {
            fresh = true;
            match actions::read_current(a, ctx) {
                Ok(orig) => {
                    if let Err(e) = db.backup_once(&t.id, i, &target, &orig) {
                        ok = false;
                        messages.push(format!("backup failed, skipped {target}: {e}"));
                        continue;
                    }
                }
                Err(e) => {
                    ok = false;
                    messages.push(format!("couldn't read original, skipped {target}: {e}"));
                    continue;
                }
            }
        }
        match actions::apply(a, ctx) {
            Ok(dup) => {
                if let Some(g) = dup {
                    let _ = db.set_setting("power_dup_guid", &g);
                    ctx.power_dup_guid = Some(g);
                }
                messages.push(format!("applied: {target}"));
            }
            Err(e) => {
                ok = false;
                // nothing changed, so a just-captured backup is meaningless
                if fresh {
                    let _ = db.delete_backup(&t.id, i);
                }
                messages.push(format!("failed: {target}: {e}"));
            }
        }
    }
    let _ = db.set_enabled(&t.id, true);
    db.log(source, Some(&t.id), "apply", ok, &messages.join("; "));
    OpResult { tweak_id: t.id.clone(), ok, messages }
}

/// Restore every backed-up action of a tweak to its original value and remove it from the profile.
pub fn restore_tweak(db: &Db, t: &Tweak, ctx: &Ctx, source: &str) -> OpResult {
    let mut messages = vec![];
    let mut ok = true;
    for b in db.backups_for(&t.id).unwrap_or_default() {
        let Some(a) = t.actions.get(b.action_idx) else {
            let _ = db.delete_backup(&t.id, b.action_idx);
            continue;
        };
        match actions::revert(a, &b.original, ctx) {
            Ok(()) => {
                let _ = db.delete_backup(&t.id, b.action_idx);
                messages.push(format!("restored: {}", a.target()));
            }
            Err(e) => {
                ok = false;
                messages.push(format!("restore failed: {}: {e}", a.target()));
            }
        }
    }
    if messages.is_empty() {
        messages.push("nothing to restore (no backup)".into());
    }
    let _ = db.set_enabled(&t.id, false);
    db.log(source, Some(&t.id), "restore", ok, &messages.join("; "));
    OpResult { tweak_id: t.id.clone(), ok, messages }
}

pub fn restore_all(db: &Db, source: &str) -> Vec<OpResult> {
    let catalog = load_catalog(db);
    let ctx = make_ctx(db);
    let mut out = vec![];
    for id in db.tweaks_with_backups().unwrap_or_default() {
        if let Some(t) = catalog.get(&id) {
            out.push(restore_tweak(db, t, &ctx, source));
        }
    }
    for id in db.enabled_ids().unwrap_or_default() {
        let _ = db.set_enabled(&id, false);
    }
    out
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ReapplySummary {
    pub checked: usize,
    pub drifted: usize,
    pub fixed: usize,
    pub errors: Vec<String>,
}

/// Verify every enabled tweak; re-apply the ones Windows (or an update) reverted.
pub fn reapply_profile(db: &Db, source: &str) -> ReapplySummary {
    let catalog = load_catalog(db);
    let mut ctx = make_ctx(db);
    let mut sum = ReapplySummary::default();
    for id in db.enabled_ids().unwrap_or_default() {
        let Some(t) = catalog.get(&id) else { continue };
        sum.checked += 1;
        let st = tweak_status(t, &ctx);
        // Only actions we successfully applied before (they have a backup) count as drift.
        let reverted: Vec<String> = st
            .actions
            .iter()
            .enumerate()
            .filter(|(i, a)| a.status == Status::NotApplied && db.has_backup(&id, *i).unwrap_or(false))
            .map(|(_, a)| a.target.clone())
            .collect();
        if reverted.is_empty() {
            continue;
        }
        sum.drifted += 1;
        db.log(source, Some(&id), "drift", true, &format!("reverted by Windows: {}", reverted.join("; ")));
        let r = apply_tweak(db, t, &mut ctx, source);
        if r.ok {
            sum.fixed += 1;
        } else {
            sum.errors.push(format!("{}: {}", t.name, r.messages.join("; ")));
        }
    }
    db.log(
        source,
        None,
        "reapply",
        sum.errors.is_empty(),
        &format!("checked {}, drifted {}, fixed {}", sum.checked, sum.drifted, sum.fixed),
    );
    sum
}

/// Create a System Restore point once, before the very first change.
pub fn ensure_restore_point(db: &Db) -> Option<String> {
    if db.get_setting("restore_point_done").is_some() {
        return None;
    }
    let r = util::powershell(
        "Checkpoint-Computer -Description 'RigSweep - before first optimization' -RestorePointType MODIFY_SETTINGS",
    );
    let _ = db.set_setting("restore_point_done", "1");
    let msg = match &r {
        Ok(_) => "System Restore point created".to_string(),
        Err(e) => format!(
            "Could not create a System Restore point (System Protection may be off). RigSweep's own backups are still used. Details: {e}"
        ),
    };
    db.log("gui", None, "restore_point", r.is_ok(), &msg);
    Some(msg)
}

#[cfg(test)]
mod live_tests {
    /// Read-only: prints the detected status of every catalog tweak on this machine.
    /// Run with: cargo test live_status -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_status() {
        let ctx = crate::actions::Ctx::new(vec!["%USERPROFILE%\\.cargo".into()], None);
        for t in crate::catalog::Catalog::builtin().tweaks {
            let s = super::tweak_status(&t, &ctx);
            println!("{:<28} {:<12} {}", t.id, s.overall,
                s.actions.iter().filter_map(|a| match &a.status { crate::actions::Status::Error(e) => Some(e.clone()), _ => None }).collect::<Vec<_>>().join(" | "));
        }
        println!("services enumerated: {}", crate::actions::service::list_services().map(|v| v.len()).unwrap_or(0));
    }
}
