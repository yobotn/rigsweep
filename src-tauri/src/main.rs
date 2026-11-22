#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod analyzer;
mod badge;
mod browserclean;
mod catalog;
mod cli;
mod commands;
mod db;
mod devclean;
mod devdisk;
mod discovery;
mod elevation;
mod engine;
mod finders;
mod gpu;
mod hardware;
mod icons;
mod heat;
mod impact;
mod kill;
mod live;
mod locks;
mod netmon;
mod update;
mod persistence;
mod report;
mod share;
mod startup;
mod util;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

use commands::AppState;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |f: &str| args.iter().any(|a| a.eq_ignore_ascii_case(f));

    // headless commands for scripts and agents (no admin rights needed, no window)
    if cli::try_run(&args) {
        return;
    }

    if !elevation::is_elevated() {
        if !elevation::relaunch_elevated(&args) {
            elevation::error_box("RigSweep needs administrator rights to manage services, tasks and policies.\nPlease accept the UAC prompt.");
        }
        return;
    }

    db::migrate_legacy();
    db::rotate_backups();

    if has("--restore-all") {
        match db::Db::open() {
            Ok(db) => {
                engine::restore_all(&db, "cli");
            }
            Err(e) => elevation::error_box(&format!("Can't open database: {e}")),
        }
        return;
    }

    if has("--apply") {
        if let Ok(db) = db::Db::open() {
            engine::reapply_profile(&db, "startup");
        }
        if !has("--tray") {
            return;
        }
    }

    run_app(has("--tray"));
}

fn show_main(app: &AppHandle) {
    // open maximized the first time; after that, keep whatever size the user chose
    static FIRST: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    if let Some(w) = app.get_webview_window("main") {
        if FIRST.swap(false, std::sync::atomic::Ordering::Relaxed) {
            let _ = w.maximize();
        }
        let _ = w.show();
        badge::invalidate();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn run_app(start_hidden: bool) {
    let db = match db::Db::open() {
        Ok(d) => d,
        Err(e) => {
            elevation::error_box(&format!("Can't open database: {e}"));
            return;
        }
    };
    let _ = db.backfill_impact(); // once: past cleanup totals become the first Impact numbers

    let analyzer = match analyzer::Analyzer::new() {
        Ok(a) => a,
        Err(e) => {
            elevation::error_box(&e);
            return;
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            db: Mutex::new(db),
            analyzer: Mutex::new(analyzer),
            paused: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::get_impact,
            commands::get_report_preview,
            commands::get_share_state,
            commands::set_share,
            commands::share_notice_seen,
            commands::reset_install_id,
            commands::apply_tweaks,
            commands::restore_tweaks,
            commands::restore_all,
            commands::reapply_now,
            commands::get_processes,
            commands::kill_process,
            commands::process_why,
            commands::add_custom_service,
            commands::remove_custom,
            commands::get_startup,
            commands::get_icons,
            commands::add_startup_tweak,
            commands::get_history,
            commands::save_settings,
            commands::set_paused,
            commands::export_profile,
            commands::import_profile,
            commands::get_dev_processes,
            commands::kill_dev_processes,
            commands::get_browser_caches,
            commands::clean_browser_caches,
            commands::get_dev_disk,
            commands::start_disk_clean,
            commands::resume_disk_clean,
            commands::get_disk_job,
            commands::disk_job_control,
            commands::dismiss_disk_job,
            commands::set_disk_cache_enabled,
            commands::add_disk_exclude,
            commands::remove_disk_exclude,
            commands::set_disk_auto,
            commands::set_dev_keep,
            commands::set_monitors,
            gpu::get_gpu,
            gpu::gpu_reset,
            gpu::get_ai_workload,
            netmon::get_net_totals,
            netmon::get_connections,
            netmon::get_port_owner,
            locks::who_has_open,
            finders::finders_orphan_programs,
            finders::finders_remove_dead_entry,
            finders::finders_recycle_leftover,
            finders::finders_old_files,
            finders::finders_recycle_files,
            finders::finders_worth_a_look,
            finders::finders_defender_scan,
            commands::get_hardware,
            commands::disk_speed_test,
            commands::get_volumes,
            commands::compact_vdisk,
            commands::get_folder_sizes,
            commands::show_in_explorer,
            commands::get_move_info,
            commands::move_cache,
            commands::take_snapshot,
            commands::get_snapshots,
            commands::clear_snapshots,
            commands::defender_self_status,
            commands::defender_self_allow,
            commands::defender_why,
            commands::open_chrome_helper,
            commands::get_live,
            commands::check_update,
            commands::install_update,
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            spawn_watchdog(app.handle().clone());
            spawn_dev_monitor(app.handle().clone());
            spawn_disk_watch(app.handle().clone());
            spawn_disk_autoclean(app.handle().clone());
            if let Ok(db) = app.state::<AppState>().db.lock() {
                live::load_switches(&db);
            }
            live::spawn(app.handle().clone());
            share::spawn(app.handle().clone());
            let h = app.handle().clone();
            update::spawn_auto_check(app.handle().clone(), move || {
                let state = h.state::<AppState>();
                let on = state.db.lock().map(|db| engine::Settings::load(&db).auto_update).unwrap_or(false);
                on
            });
            if !start_hidden {
                show_main(app.handle());
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // closing the window keeps RigSweep running in the tray
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running RigSweep");
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open RigSweep", true, None::<&str>)?;
    let reapply = MenuItem::with_id(app, "reapply", "Re-apply profile now", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause watchdog", true, None::<&str>)?;
    let restore = MenuItem::with_id(app, "restore", "Restore all to Windows defaults", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&open, &sep1, &reapply, &pause, &restore, &sep2, &quit])?;

    let pause_item = pause.clone();
    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("app icon"))
        .tooltip("RigSweep")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| {
            let state = app.state::<AppState>();
            match event.id.as_ref() {
                "open" => show_main(app),
                "reapply" => {
                    let app = app.clone();
                    std::thread::spawn(move || {
                        let state = app.state::<AppState>();
                        if let Ok(db) = state.db.lock() {
                            let s = engine::reapply_profile(&db, "tray");
                            let _ = app.emit("profile-changed", &s);
                        };
                    });
                }
                "pause" => {
                    let now = !state.paused.load(Ordering::Relaxed);
                    state.paused.store(now, Ordering::Relaxed);
                    let _ = pause_item.set_text(if now { "Resume watchdog" } else { "Pause watchdog" });
                    let _ = app.emit("paused-changed", now);
                }
                "restore" => {
                    // destructive: open the UI so the user confirms there
                    show_main(app);
                    let _ = app.emit("request-restore-all", ());
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Settings > Dev Disk "clean automatically": once an hour, check whether a run is due (a week
/// since the last one, or the system drive below the chosen free space, at most twice a day),
/// then empty only the caches that rebuild themselves. Logged to History as "auto".
fn spawn_disk_autoclean(app: AppHandle) {
    std::thread::spawn(move || {
        const LAST: &str = "disk_auto_last";
        std::thread::sleep(Duration::from_secs(300));
        loop {
            let state = app.state::<AppState>();
            let (s, last) = match state.db.lock() {
                Ok(db) => (engine::Settings::load(&db), db.get_setting(LAST).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0)),
                Err(_) => {
                    std::thread::sleep(Duration::from_secs(3600));
                    continue;
                }
            };
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            let since = now.saturating_sub(last);
            let low = s.disk_auto_low_gb > 0 && devdisk::system_free().is_some_and(|f| f < u64::from(s.disk_auto_low_gb) << 30);
            let due = s.disk_auto_clean && (since >= 7 * 86_400 || (low && since >= 12 * 3600));
            if due && !state.paused.load(Ordering::Relaxed) && !devdisk::is_running() {
                if let Ok(db) = state.db.lock() {
                    let _ = db.set_setting(LAST, &now.to_string());
                }
                let cfg = commands::disk_config(&s);
                let targets = devdisk::auto_targets(&devdisk::report(&cfg));
                if !targets.is_empty() {
                    let done = app.clone();
                    let _ = devdisk::start(cfg, targets, |_| {}, move |r| {
                        if let Ok(db) = done.state::<AppState>().db.lock() {
                            for c in &r.cleaned {
                                db.log("auto", None, "disk_clean", true, c);
                            }
                            if r.freed > 0 {
                                db.log("auto", None, "disk_clean", true, &format!("automatic cleanup freed {:.1} MB", r.freed as f64 / 1048576.0));
                            }
                            db.add_impact("auto", &r.impact);
                        }
                        if r.freed > 0 {
                            let _ = done.emit("disk-autocleaned", r.freed);
                        }
                    });
                }
            }
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

/// Every minute: scan dev processes (that's also what measures "quiet for N minutes"), end the
/// stale trees that stayed CPU-silent for the whole idle period if auto-cleanup is on, and update
/// the tray / taskbar dot showing how much memory stale processes still hold.
/// Disk health alerts (Settings → Appearance & features): one PowerShell disk read every 6 hours,
/// compared with the last one. A worse disk is logged to History and shown in the app.
fn spawn_disk_watch(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(120));
        loop {
            if live::DISK_ALERTS_ON.load(Ordering::Relaxed) {
                if let Ok((_, alerts)) = hardware::check_disks() {
                    if !alerts.is_empty() {
                        if let Ok(db) = app.state::<AppState>().db.lock() {
                            for a in &alerts {
                                db.log("auto", None, "disk_health", a.severity != "bad", &format!("{} ({}): {} {} -> {}", a.model, a.disk_id, a.kind, a.from, a.to));
                            }
                        }
                        let _ = app.emit("disk-alerts", &alerts);
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}

fn spawn_dev_monitor(app: AppHandle) {
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            std::thread::sleep(Duration::from_secs(if first { 5 } else { 60 }));
            first = false;
            let state = app.state::<AppState>();
            let s = match state.db.lock() {
                Ok(db) => engine::Settings::load(&db),
                Err(_) => continue,
            };
            let auto = s.auto_kill_stale && !state.paused.load(Ordering::Relaxed);
            if !auto && !s.badge_enabled {
                badge::update(&app, 0, &s);
                continue;
            }
            let (stale_mem, result) = {
                let Ok(mut a) = state.analyzer.lock() else { continue };
                let report = a.dev_scan(s.idle_minutes);
                let targets = if auto { devclean::auto_targets(&report, s.idle_minutes) } else { vec![] };
                if targets.is_empty() {
                    (report.stale_mem, None)
                } else {
                    let r = devclean::kill(&report, &targets);
                    (report.stale_mem.saturating_sub(r.freed_mem), Some(r))
                }
            };
            badge::update(&app, stale_mem, &s);
            let Some(result) = result else { continue };
            if let Ok(db) = state.db.lock() {
                for e in &result.ended {
                    db.log("auto", None, "dev_cleanup", true, &format!("ended {e}"));
                }
                for f in &result.failed {
                    db.log("auto", None, "dev_cleanup", false, f);
                }
                db.add_impact("auto", &result.impact);
            }
            if !result.ended.is_empty() {
                let _ = app.emit("dev-autocleaned", &result);
            }
        }
    });
}

/// Periodically verify the profile and re-apply anything Windows reverted.
fn spawn_watchdog(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last = Instant::now();
        loop {
            std::thread::sleep(Duration::from_secs(15));
            let state = app.state::<AppState>();
            let minutes = match state.db.lock() {
                Ok(db) => engine::Settings::load(&db).watchdog_minutes.max(1),
                Err(_) => 30,
            };
            if last.elapsed() < Duration::from_secs(minutes as u64 * 60) || state.paused.load(Ordering::Relaxed) {
                continue;
            }
            last = Instant::now();
            if let Ok(db) = state.db.lock() {
                if db.enabled_ids().map(|v| v.is_empty()).unwrap_or(true) {
                    continue;
                }
                let s = engine::reapply_profile(&db, "watchdog");
                if s.drifted > 0 {
                    let _ = app.emit("profile-changed", &s);
                }
            };
        }
    });
}
