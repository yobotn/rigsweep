//! Updates, the user's call: nothing is checked unless they click "Check now" or turn on
//! "Check for updates automatically" in Settings (off by default). A check only downloads
//! `latest.json` from the GitHub release; nothing about this PC is sent. An update is installed
//! only after the user clicks Install, and only if the installer's signature matches the public
//! key built into RigSweep (`plugins.updater.pubkey` in tauri.conf.json).

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

#[derive(Serialize, Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub current: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Serialize, Clone)]
struct Progress {
    done: u64,
    total: Option<u64>,
}

fn err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// `None` when this is the latest version.
pub async fn check(app: &AppHandle) -> Result<Option<UpdateInfo>, String> {
    let u = app.updater().map_err(err)?.check().await.map_err(err)?;
    Ok(u.map(|u| UpdateInfo {
        version: u.version.clone(),
        current: u.current_version.clone(),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
        date: u.date.map(|d| d.date().to_string()),
    }))
}

/// Downloads, verifies and runs the installer. On Windows the app exits while the installer
/// runs, and the installer starts RigSweep again when it's done.
pub async fn install(app: &AppHandle) -> Result<(), String> {
    let Some(u) = app.updater().map_err(err)?.check().await.map_err(err)? else {
        return Err("RigSweep is already up to date.".into());
    };
    let mut done = 0u64;
    let a = app.clone();
    u.download_and_install(
        move |chunk, total| {
            done += chunk as u64;
            let _ = a.emit("update-progress", Progress { done, total });
        },
        || {},
    )
    .await
    .map_err(err)
}

const FIRST_CHECK: Duration = Duration::from_secs(30);
const EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Background check while RigSweep runs (window or tray): shortly after start, then daily.
/// Reads the setting each time, so turning it off in Settings takes effect without a restart.
pub fn spawn_auto_check(app: AppHandle, enabled: impl Fn() -> bool + Send + 'static) {
    tauri::async_runtime::spawn(async move {
        tokio_sleep(FIRST_CHECK).await;
        loop {
            if enabled() {
                if let Ok(Some(info)) = check(&app).await {
                    let _ = app.emit("update-available", info);
                }
            }
            tokio_sleep(EVERY).await;
        }
    });
}

/// Sleep without pulling in tokio's timer feature directly: park a blocking-pool thread.
async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}
