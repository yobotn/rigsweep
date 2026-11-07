#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod analyzer;
mod catalog;
mod commands;
mod db;
mod devclean;
mod elevation;
mod engine;
mod persistence;
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
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
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

    let analyzer = match analyzer::Analyzer::new() {
        Ok(a) => a,
        Err(e) => {
            elevation::error_box(&e);
            return;
        }
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .manage(AppState {
            db: Mutex::new(db),
            analyzer: Mutex::new(analyzer),
            paused: AtomicBool::new(false),
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_state,
            commands::apply_tweaks,
            commands::restore_tweaks,
            commands::restore_all,
            commands::reapply_now,
            commands::get_processes,
            commands::kill_process,
            commands::add_custom_service,
            commands::remove_custom,
            commands::get_history,
            commands::save_settings,
            commands::set_paused,
            commands::export_profile,
            commands::import_profile,
            commands::get_dev_processes,
            commands::kill_dev_processes,
            commands::take_snapshot,
            commands::get_snapshots,
            commands::clear_snapshots,
            commands::defender_self_status,
            commands::defender_self_allow,
            commands::open_chrome_helper,
        ])
        .setup(move |app| {
            build_tray(app.handle())?;
            spawn_watchdog(app.handle().clone());
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
