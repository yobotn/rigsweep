use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::State;

use crate::actions::defender;
use crate::analyzer::{Analyzer, Metrics, Snapshot};
use crate::browserclean::{self, BrowserReport, CleanResult, CleanTarget};
use crate::devclean::{self, DevReport, KillResult, KillTarget};
use crate::catalog::{custom_service_tweak, Category, Tweak};
use crate::db::{Db, HistoryRow, SnapshotRow};
use crate::engine::{self, OpResult, ReapplySummary, Settings, TweakStatus};
use crate::{elevation, persistence, util};

pub struct AppState {
    pub db: Mutex<Db>,
    pub analyzer: Mutex<Analyzer>,
    pub paused: AtomicBool,
}

type R<T> = Result<T, String>;

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

fn lock<'a>(s: &'a State<'_, AppState>) -> R<std::sync::MutexGuard<'a, Db>> {
    s.db.lock().map_err(|_| "database lock poisoned".to_string())
}

#[derive(Serialize)]
pub struct TweakView {
    #[serde(flatten)]
    pub tweak: Tweak,
    pub status: TweakStatus,
    pub in_profile: bool,
    pub drift_count: i64,
}

#[derive(Serialize)]
pub struct StateView {
    pub elevated: bool,
    pub categories: Vec<Category>,
    pub tweaks: Vec<TweakView>,
    pub settings: Settings,
    pub startup_registered: bool,
    pub backup_count: i64,
    pub data_dir: String,
    pub paused: bool,
}

#[tauri::command]
pub async fn get_state(state: State<'_, AppState>) -> R<StateView> {
    let db = lock(&state)?;
    let catalog = engine::load_catalog(&db);
    let ctx = engine::make_ctx(&db);
    let enabled = db.enabled_ids().map_err(err)?;
    let drift = db.drift_counts().map_err(err)?;
    let tweaks = catalog
        .tweaks
        .iter()
        .map(|t| TweakView {
            status: engine::tweak_status(t, &ctx),
            in_profile: enabled.contains(&t.id),
            drift_count: *drift.get(&t.id).unwrap_or(&0),
            tweak: t.clone(),
        })
        .collect();
    Ok(StateView {
        elevated: elevation::is_elevated(),
        categories: catalog.categories.clone(),
        tweaks,
        settings: Settings::load(&db),
        startup_registered: persistence::is_registered(),
        backup_count: db.backup_count().map_err(err)?,
        data_dir: crate::db::data_dir().display().to_string(),
        paused: state.paused.load(Ordering::Relaxed),
    })
}

#[derive(Serialize)]
pub struct ApplyResponse {
    pub restore_point: Option<String>,
    pub results: Vec<OpResult>,
}

#[tauri::command]
pub async fn apply_tweaks(state: State<'_, AppState>, ids: Vec<String>) -> R<ApplyResponse> {
    // Boost Report: measure the untouched machine once, before the very first change
    let need_baseline = !ids.is_empty() && !lock(&state)?.has_snapshot("baseline");
    if need_baseline {
        let m = measure(&state)?;
        lock(&state)?.add_snapshot("baseline", &serde_json::to_value(m).map_err(err)?).map_err(err)?;
    }
    let db = lock(&state)?;
    let restore_point = if ids.is_empty() { None } else { engine::ensure_restore_point(&db) };
    let catalog = engine::load_catalog(&db);
    let mut ctx = engine::make_ctx(&db);
    let results = ids
        .iter()
        .filter_map(|id| catalog.get(id))
        .map(|t| engine::apply_tweak(&db, t, &mut ctx, "gui"))
        .collect();
    Ok(ApplyResponse { restore_point, results })
}

#[tauri::command]
pub async fn restore_tweaks(state: State<'_, AppState>, ids: Vec<String>) -> R<Vec<OpResult>> {
    let db = lock(&state)?;
    let catalog = engine::load_catalog(&db);
    let ctx = engine::make_ctx(&db);
    Ok(ids
        .iter()
        .filter_map(|id| catalog.get(id))
        .map(|t| engine::restore_tweak(&db, t, &ctx, "gui"))
        .collect())
}

#[tauri::command]
pub async fn restore_all(state: State<'_, AppState>) -> R<Vec<OpResult>> {
    let db = lock(&state)?;
    Ok(engine::restore_all(&db, "gui"))
}

#[tauri::command]
pub async fn reapply_now(state: State<'_, AppState>) -> R<ReapplySummary> {
    let db = lock(&state)?;
    Ok(engine::reapply_profile(&db, "gui"))
}

#[tauri::command]
pub async fn get_processes(state: State<'_, AppState>) -> R<Snapshot> {
    let catalog = {
        let db = lock(&state)?;
        engine::load_catalog(&db)
    };
    let mut a = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?;
    Ok(a.snapshot(&catalog))
}

#[tauri::command]
pub async fn kill_process(state: State<'_, AppState>, pid: u32) -> R<String> {
    let r = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?.kill(pid);
    let db = lock(&state)?;
    match &r {
        Ok(name) => db.log("gui", None, "kill", true, &format!("ended {name} (pid {pid})")),
        Err(e) => db.log("gui", None, "kill", false, &format!("pid {pid}: {e}")),
    }
    r
}

/// Create (or reuse) a custom tweak for a service seen in the analyzer. Returns its id.
#[tauri::command]
pub async fn add_custom_service(state: State<'_, AppState>, name: String, display: String) -> R<String> {
    let db = lock(&state)?;
    let catalog = engine::load_catalog(&db);
    if let Some(t) = catalog.tweaks.iter().find(|t| {
        t.actions.iter().any(|a| matches!(a, crate::catalog::Action::Service { name: n, .. } if n.eq_ignore_ascii_case(&name)))
    }) {
        return Ok(t.id.clone());
    }
    let t = custom_service_tweak(&name, &display)?;
    db.add_custom(&t).map_err(err)?;
    db.log("gui", Some(&t.id), "add_custom", true, &t.name);
    Ok(t.id)
}

#[tauri::command]
pub async fn remove_custom(state: State<'_, AppState>, id: String) -> R<()> {
    let db = lock(&state)?;
    let catalog = engine::load_catalog(&db);
    if let Some(t) = catalog.get(&id) {
        if !t.custom {
            return Err("only custom tweaks can be removed".into());
        }
        let ctx = engine::make_ctx(&db);
        let r = engine::restore_tweak(&db, t, &ctx, "gui");
        if !r.ok {
            return Err(format!("restore failed, not removing: {}", r.messages.join("; ")));
        }
    }
    db.remove_custom(&id).map_err(err)
}

#[tauri::command]
pub async fn get_history(state: State<'_, AppState>, limit: Option<i64>) -> R<Vec<HistoryRow>> {
    lock(&state)?.history(limit.unwrap_or(300)).map_err(err)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: Settings) -> R<Vec<String>> {
    let db = lock(&state)?;
    let old = Settings::load(&db);
    let mut notes = vec![];
    let mut s = settings;
    s.dev_folders = s.dev_folders.into_iter().map(|f| f.trim().to_string()).filter(|f| !f.is_empty()).collect();
    s.save(&db).map_err(err)?;

    // startup task
    if s.start_with_windows {
        persistence::register(s.stay_in_tray).map_err(err)?;
        notes.push("Startup task registered (runs elevated at every logon).".into());
    } else {
        persistence::unregister().map_err(err)?;
    }

    // keep Defender folder exclusions in sync with the list
    let active = db.enabled_ids().map_err(err)?.contains(&"dev.defender_folders".to_string());
    if active && old.dev_folders != s.dev_folders {
        let pre: Vec<String> = db
            .backups_for("dev.defender_folders")
            .unwrap_or_default()
            .first()
            .and_then(|b| b.original["preexisting_paths"].as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let new_exp: Vec<String> = s.dev_folders.iter().map(|f| util::expand_env(f)).collect();
        let removed: Vec<String> = old
            .dev_folders
            .iter()
            .map(|f| util::expand_env(f))
            .filter(|f| !new_exp.iter().any(|n| n.eq_ignore_ascii_case(f)) && !pre.iter().any(|p| p.eq_ignore_ascii_case(f)))
            .collect();
        if let Err(e) = defender::remove(&removed, &[]) {
            notes.push(format!("Couldn't remove old Defender exclusions: {e}"));
        }
        let catalog = engine::load_catalog(&db);
        if let Some(t) = catalog.get("dev.defender_folders") {
            let mut ctx = engine::make_ctx(&db);
            let r = engine::apply_tweak(&db, t, &mut ctx, "gui");
            notes.push(format!("Defender folder exclusions updated{}", if r.ok { "" } else { " (with errors)" }));
        }
    }
    db.log("gui", None, "settings", true, &serde_json::to_string(&s).unwrap_or_default());
    Ok(notes)
}

#[tauri::command]
pub async fn set_paused(state: State<'_, AppState>, paused: bool) -> R<()> {
    state.paused.store(paused, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn export_profile(state: State<'_, AppState>) -> R<String> {
    let db = lock(&state)?;
    let doc = json!({
        "app": "RigSweep",
        "version": 1,
        "exported_at": chrono::Local::now().to_rfc3339(),
        "enabled": db.enabled_ids().map_err(err)?,
        "custom": db.custom_tweaks().map_err(err)?,
        "settings": Settings::load(&db),
    });
    let dir = std::env::var("USERPROFILE").map(|p| std::path::PathBuf::from(p).join("Documents")).unwrap_or_else(|_| crate::db::data_dir());
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join(format!("RigSweep-profile-{}.json", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(err)?).map_err(err)?;
    db.log("gui", None, "export", true, &path.display().to_string());
    Ok(path.display().to_string())
}

#[derive(Deserialize)]
struct ImportDoc {
    enabled: Vec<String>,
    #[serde(default)]
    custom: Vec<Tweak>,
    settings: Option<Value>,
}

/// Imports custom tweaks + dev folders and returns the tweak ids to apply (the UI confirms first).
#[tauri::command]
pub async fn import_profile(state: State<'_, AppState>, json: String) -> R<Vec<String>> {
    let doc: ImportDoc = serde_json::from_str(&json).map_err(|e| format!("not an RigSweep profile: {e}"))?;
    let db = lock(&state)?;
    for t in &doc.custom {
        for a in &t.actions {
            crate::catalog::validate_action(a).map_err(|e| format!("custom tweak {}: {e}", t.id))?;
        }
        let mut t = t.clone();
        t.custom = true;
        t.category = "custom".into();
        db.add_custom(&t).map_err(err)?;
    }
    if let Some(s) = doc.settings {
        let mut cur = Settings::load(&db);
        if let Some(f) = s.get("dev_folders").and_then(|v| serde_json::from_value(v.clone()).ok()) {
            cur.dev_folders = f;
        }
        if let Some(m) = s.get("watchdog_minutes").and_then(|v| v.as_u64()) {
            cur.watchdog_minutes = m as u32;
        }
        cur.save(&db).map_err(err)?;
    }
    let catalog = engine::load_catalog(&db);
    let ids: Vec<String> = doc.enabled.into_iter().filter(|id| catalog.get(id).is_some()).collect();
    db.log("gui", None, "import", true, &format!("{} tweaks", ids.len()));
    Ok(ids)
}

// ---------- dev cleanup ----------

fn idle_hours(state: &State<'_, AppState>) -> R<u32> {
    Ok(Settings::load(&*lock(state)?).idle_hours)
}

#[tauri::command]
pub async fn get_dev_processes(state: State<'_, AppState>) -> R<DevReport> {
    let h = idle_hours(&state)?;
    let mut a = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?;
    Ok(a.dev_scan(h))
}

#[derive(Serialize)]
pub struct CleanupResponse {
    pub result: KillResult,
    pub before: SnapshotRow,
    pub after: SnapshotRow,
}

/// End the selected process trees, with a before/after measurement for the Boost Report.
#[tauri::command]
pub async fn kill_dev_processes(state: State<'_, AppState>, targets: Vec<KillTarget>) -> R<CleanupResponse> {
    let h = idle_hours(&state)?;
    let (before, result) = {
        let mut a = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?;
        let before = a.metrics(h); // includes a fresh dev scan, taken right now
        let report = a.last_dev.clone();
        (before, devclean::kill(&report, &targets))
    };
    std::thread::sleep(std::time::Duration::from_millis(1500)); // let Windows reclaim the pages
    let after = measure(&state)?;
    let db = lock(&state)?;
    for e in &result.ended {
        db.log("gui", None, "dev_cleanup", true, &format!("ended {e}"));
    }
    for f in &result.failed {
        db.log("gui", None, "dev_cleanup", false, f);
    }
    let before = db.add_snapshot("before cleanup", &serde_json::to_value(before).map_err(err)?).map_err(err)?;
    let after = db.add_snapshot("after cleanup", &serde_json::to_value(after).map_err(err)?).map_err(err)?;
    Ok(CleanupResponse { result, before, after })
}

// ---------- browser cleanup ----------

#[tauri::command]
pub async fn get_browser_caches() -> R<BrowserReport> {
    tauri::async_runtime::spawn_blocking(browserclean::report).await.map_err(err)
}

/// Empty the selected caches. Paths are re-derived from (browser, profile, key) on this side.
#[tauri::command]
pub async fn clean_browser_caches(state: State<'_, AppState>, targets: Vec<CleanTarget>) -> R<CleanResult> {
    let r = tauri::async_runtime::spawn_blocking(move || browserclean::clean(&targets)).await.map_err(err)?;
    let db = lock(&state)?;
    for c in &r.cleaned {
        db.log("gui", None, "browser_clean", true, c);
    }
    for b in &r.skipped_running {
        db.log("gui", None, "browser_clean", false, &format!("{b} is open, skipped"));
    }
    if r.freed > 0 {
        db.log("gui", None, "browser_clean", true, &format!("freed {:.1} MB, {} locked file(s) left", r.freed as f64 / 1048576.0, r.locked_files));
    }
    Ok(r)
}

// ---------- boost report ----------

fn measure(state: &State<'_, AppState>) -> R<Metrics> {
    let h = idle_hours(state)?;
    let mut a = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?;
    Ok(a.metrics(h))
}

#[tauri::command]
pub async fn take_snapshot(state: State<'_, AppState>, label: String) -> R<SnapshotRow> {
    let m = measure(&state)?;
    let label: String = if label.trim().is_empty() { "manual".into() } else { label.trim().chars().take(40).collect() };
    lock(&state)?.add_snapshot(&label, &serde_json::to_value(m).map_err(err)?).map_err(err)
}

#[tauri::command]
pub async fn get_snapshots(state: State<'_, AppState>) -> R<Vec<SnapshotRow>> {
    lock(&state)?.snapshots().map_err(err)
}

#[tauri::command]
pub async fn clear_snapshots(state: State<'_, AppState>) -> R<()> {
    lock(&state)?.clear_snapshots().map_err(err)
}

// ---------- antivirus trust ----------

#[derive(Serialize)]
pub struct DefenderSelf {
    pub folder: String,
    pub excluded: bool,
    /// false when Defender isn't the active antivirus (Get-MpPreference fails)
    pub available: bool,
}

fn app_folder() -> R<String> {
    let exe = std::env::current_exe().map_err(err)?;
    Ok(exe.parent().ok_or("no exe folder")?.display().to_string())
}

fn defender_status_now() -> R<DefenderSelf> {
    let folder = app_folder()?;
    let norm = |s: &str| s.trim_end_matches('\\').to_ascii_lowercase();
    Ok(match defender::read_prefs() {
        Ok(p) => DefenderSelf { excluded: p.paths.iter().any(|x| norm(x) == norm(&folder)), folder, available: true },
        Err(_) => DefenderSelf { folder, excluded: false, available: false },
    })
}

#[tauri::command]
pub async fn defender_self_status() -> R<DefenderSelf> {
    defender_status_now()
}

/// Opt-in only: add / remove RigSweep's own install folder from Defender exclusions.
#[tauri::command]
pub async fn defender_self_allow(state: State<'_, AppState>, allow: bool) -> R<DefenderSelf> {
    let folder = app_folder()?;
    let q = util::ps_quote(&folder);
    let cmd = if allow { format!("Add-MpPreference -ExclusionPath {q}") } else { format!("Remove-MpPreference -ExclusionPath {q}") };
    let r = util::powershell(&cmd);
    lock(&state)?.log("gui", None, if allow { "av_allow" } else { "av_disallow" }, r.is_ok(), &folder);
    r.map_err(err)?;
    defender_status_now()
}

// ---------- chrome helper ----------

/// Locate the bundled Chrome extension folder and open it in Explorer.
#[tauri::command]
pub async fn open_chrome_helper(app: tauri::AppHandle) -> R<String> {
    use tauri::Manager;
    let mut candidates = vec![];
    if let Ok(r) = app.path().resource_dir() {
        candidates.push(r.join("chrome-extension"));
    }
    if let Some(d) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.to_path_buf())) {
        candidates.push(d.join("chrome-extension"));
    }
    // dev builds run from the repo checkout (not compiled into release builds)
    #[cfg(debug_assertions)]
    candidates.push(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("chrome-extension"));
    let dir = candidates
        .into_iter()
        .find(|p| p.join("manifest.json").exists())
        .ok_or("Chrome helper folder not found next to the app")?;
    let dir = dir.canonicalize().unwrap_or(dir);
    let shown = dir.display().to_string();
    let shown = shown.strip_prefix(r"\\?\").unwrap_or(&shown).to_string();
    let _ = util::run("explorer.exe", &[&shown]);
    Ok(shown)
}

#[cfg(test)]
mod demo_dump {
    /// Read-only: dump REAL data from this machine for README screenshots (docs/screenshot-data.json).
    /// Run with: cargo test dump_screenshot_data -- --ignored --nocapture
    #[test]
    #[ignore]
    fn dump_screenshot_data() {
        use crate::{analyzer::Analyzer, catalog::Catalog, engine};
        let mut a = Analyzer::new().unwrap();
        let dev = a.dev_scan(2);
        let snap = a.snapshot(&Catalog::builtin());
        let metrics = a.metrics(2);
        let ctx = crate::actions::Ctx::new(vec![r"%USERPROFILE%\.cargo".into()], None);
        let cat = Catalog::builtin();
        let tweaks: Vec<serde_json::Value> = cat
            .tweaks
            .iter()
            .map(|t| {
                let mut v = serde_json::to_value(t).unwrap();
                v["status"] = serde_json::to_value(engine::tweak_status(t, &ctx)).unwrap();
                v["in_profile"] = false.into();
                v["drift_count"] = 0.into();
                v
            })
            .collect();
        let out = serde_json::json!({
            "captured_at": chrono::Local::now().to_rfc3339(),
            "dev": dev, "snapshot": snap, "metrics": metrics,
            "categories": cat.categories, "tweaks": tweaks,
        });
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("docs").join("screenshot-data.json");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, serde_json::to_string_pretty(&out).unwrap()).unwrap();
        println!("wrote {}", p.display());
    }
}
