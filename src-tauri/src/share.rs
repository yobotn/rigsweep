//! Anonymous hourly stats: the on/off state, the sender and the "forget me" request.
//!
//! On by default outside Europe (see `report::consent_default`), but nothing is sent before the
//! first-run notice has been shown, so nobody shares without having seen what and why. Turning
//! sharing off asks the server to delete everything this install sent and starts a new random id.
//!
//! Settings keys: `share` ("on" / "off", missing = the regional default), `share_notice` (the notice version, see NOTICE, once
//! the notice was shown), `share_last` (when the last report was accepted), `share_last_seq` (the
//! last impact row the server has), `share_last_report` (that report, as sent), `install_id`.
//!
//! Each report carries the cleanups recorded since the previous one, row by row, so the server
//! can store them as they are; nothing is summed on this PC.

use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::commands::AppState;
use crate::db::Db;
use crate::report::{self, Consent, Report};

pub const SERVER: &str = "https://stats.rigsweep.com";
const EVERY_MINUTES: i64 = 60;
/// at most this many cleanup rows per report (a long backlog goes out over the next hours)
const MAX_EVENTS: usize = 2000;
/// the vendor software section (`seen`) goes out at most this often
const SEEN_EVERY_HOURS: i64 = 24;
const TS: &str = "%Y-%m-%d %H:%M:%S";
/// Version of what the notice describes. When a report gains a new kind of data (2 = the daily vendor
/// software section), people who agreed to the earlier version see the notice again, and nothing is
/// sent until they have.
const NOTICE: &str = "2";

#[derive(Serialize, Clone, Debug)]
pub struct ShareState {
    /// sharing is on (by choice, or by the regional default)
    pub enabled: bool,
    /// the user chose explicitly
    pub chosen: bool,
    pub notice_seen: bool,
    pub consent_default: Consent,
    pub install_id: String,
    pub last_sent: Option<String>,
    pub last_report: Option<String>,
}

pub fn install_id(db: &Db) -> String {
    match db.get_setting("install_id") {
        Some(id) if report::install_id_ok(&id) => id,
        _ => {
            let id = report::new_install_id();
            let _ = db.set_setting("install_id", &id);
            id
        }
    }
}

fn enabled_with(db: &Db, default: Consent) -> (bool, bool) {
    match db.get_setting("share").as_deref() {
        Some("on") => (true, true),
        Some("off") => (false, true),
        _ => (default == Consent::OptOut, false),
    }
}

pub fn state(db: &Db) -> ShareState {
    let consent_default = report::local_consent_default();
    let (enabled, chosen) = enabled_with(db, consent_default);
    ShareState {
        enabled,
        chosen,
        notice_seen: db.get_setting("share_notice").as_deref() == Some(NOTICE),
        consent_default,
        install_id: install_id(db),
        last_sent: db.get_setting("share_last").filter(|s| !s.is_empty()),
        last_report: db.get_setting("share_last_report").filter(|s| !s.is_empty()),
    }
}

/// Sending is allowed only when sharing is on AND the notice has been shown.
fn may_send(db: &Db) -> bool {
    db.get_setting("share_notice").as_deref() == Some(NOTICE) && enabled_with(db, report::local_consent_default()).0
}

fn due(db: &Db) -> bool {
    let Some(last) = db.get_setting("share_last") else { return true };
    match chrono::NaiveDateTime::parse_from_str(&last, TS) {
        Ok(t) => chrono::Local::now().naive_local() - t >= chrono::Duration::minutes(EVERY_MINUTES),
        Err(_) => true,
    }
}

/// The last impact row the server has (0 = none yet).
fn last_seq(db: &Db) -> i64 {
    db.get_setting("share_last_seq").and_then(|s| s.parse().ok()).unwrap_or(0)
}

/// When the last report was accepted, as UTC to the minute.
fn since_utc(db: &Db) -> Option<String> {
    use chrono::TimeZone;
    let t = chrono::NaiveDateTime::parse_from_str(&db.get_setting("share_last")?, TS).ok()?;
    let t = chrono::Local.from_local_datetime(&t).earliest()?;
    Some(t.with_timezone(&chrono::Utc).format("%Y-%m-%dT%H:%MZ").to_string())
}

/// Whether the next report carries the vendor software section again.
fn seen_due(db: &Db) -> bool {
    match db.get_setting("share_seen_last").and_then(|s| chrono::NaiveDateTime::parse_from_str(&s, TS).ok()) {
        Some(t) => chrono::Local::now().naive_local() - t >= chrono::Duration::hours(SEEN_EVERY_HOURS),
        None => true,
    }
}

/// The report this PC would send now (also the Preview, which always shows the vendor section).
pub fn build_report(app: &AppHandle) -> Result<Report, String> {
    build_report_with(app, true)
}

fn build_report_with(app: &AppHandle, with_seen: bool) -> Result<Report, String> {
    let state = app.state::<AppState>();
    let (catalog, id, since, events, tweaks) = {
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        let events = db.impact_events_after(last_seq(&db), MAX_EVENTS).map_err(|e| e.to_string())?;
        (crate::engine::load_catalog(&db), install_id(&db), since_utc(&db), events, db.enabled_ids().map_err(|e| e.to_string())?)
    };
    let snap = state.analyzer.lock().map_err(|_| "analyzer lock poisoned")?.snapshot(&catalog);
    let live = crate::live::history();
    let ram_used_avg = if live.is_empty() { snap.mem_used } else { live.iter().map(|s| s.mem_used).sum::<u64>() / live.len() as u64 };
    Ok(report::build(report::Inputs {
        install_id: &id,
        app_version: &app.package_info().version.to_string(),
        windows_build: &report::windows_build(),
        since,
        events,
        machine: report::Machine {
            ram_total: snap.mem_total,
            ram_used_avg,
            cores: std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        },
        procs: snap.processes.iter().map(|p| (p.name.clone(), p.cpu, p.mem)).collect(),
        seen: if with_seen {
            let enabled: std::collections::HashSet<String> = tweaks.iter().cloned().collect();
            crate::discovery::collect(&crate::startup::scan(&catalog), &snap.processes, &enabled)
        } else {
            vec![]
        },
        tweaks,
    }))
}

fn client() -> Result<reqwest::Client, String> {
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    reqwest::Client::builder()
        .user_agent(concat!("RigSweep/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

async fn post(report: &Report) -> Result<(), String> {
    let r = client()?.post(format!("{SERVER}/v1/report")).json(report).send().await.map_err(|e| e.to_string())?;
    if r.status().is_success() {
        Ok(())
    } else {
        Err(format!("server said {}", r.status()))
    }
}

/// Ask the server to delete everything sent under `id`. Best effort: 404 means nothing was there.
async fn forget(id: &str) -> Result<(), String> {
    let r = client()?.delete(format!("{SERVER}/v1/install/{id}")).send().await.map_err(|e| e.to_string())?;
    if r.status().is_success() || r.status() == reqwest::StatusCode::NOT_FOUND {
        Ok(())
    } else {
        Err(format!("server said {}", r.status()))
    }
}

/// Turn sharing on or off (an explicit choice; also counts as having seen the notice). Turning it
/// off deletes this install's data on the server and starts a new id, so on again = a new install.
pub fn set(app: &AppHandle, on: bool) -> Result<ShareState, String> {
    let state = app.state::<AppState>();
    let old = {
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        let was_on = enabled_with(&db, report::local_consent_default()).0;
        db.set_setting("share", if on { "on" } else { "off" }).map_err(|e| e.to_string())?;
        db.set_setting("share_notice", NOTICE).map_err(|e| e.to_string())?;
        if !on && was_on {
            let old = install_id(&db);
            rotate(&db)?;
            Some(old)
        } else {
            None
        }
    };
    if let Some(id) = old {
        tauri::async_runtime::spawn(async move {
            if let Err(e) = forget(&id).await {
                eprintln!("stats: delete failed: {e}");
            }
        });
    }
    if on {
        kick(app.clone());
    }
    let db = state.db.lock().map_err(|_| "database lock poisoned")?;
    Ok(self::state(&db))
}

fn rotate(db: &Db) -> Result<(), String> {
    db.set_setting("install_id", &report::new_install_id()).map_err(|e| e.to_string())?;
    db.set_setting("share_last", "").map_err(|e| e.to_string())?;
    db.set_setting("share_last_seq", "").map_err(|e| e.to_string())?;
    db.set_setting("share_seen_last", "").map_err(|e| e.to_string())?;
    db.set_setting("share_last_report", "").map_err(|e| e.to_string())?;
    Ok(())
}

/// A new random id; the server forgets the old one.
pub fn reset_id(app: &AppHandle) -> Result<ShareState, String> {
    let state = app.state::<AppState>();
    let old = {
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        let old = install_id(&db);
        rotate(&db)?;
        old
    };
    tauri::async_runtime::spawn(async move {
        let _ = forget(&old).await;
    });
    kick(app.clone());
    let db = state.db.lock().map_err(|_| "database lock poisoned")?;
    Ok(self::state(&db))
}

/// The notice was shown and the user kept the default.
pub fn notice_seen(app: &AppHandle) -> Result<ShareState, String> {
    let state = app.state::<AppState>();
    {
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        db.set_setting("share_notice", NOTICE).map_err(|e| e.to_string())?;
    }
    kick(app.clone());
    let db = state.db.lock().map_err(|_| "database lock poisoned")?;
    Ok(self::state(&db))
}

/// Send now if allowed and due. Returns whether a report was accepted.
pub async fn send_if_due(app: &AppHandle) -> Result<bool, String> {
    let (ok, with_seen) = {
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        (may_send(&db) && due(&db), seen_due(&db))
    };
    if !ok {
        return Ok(false);
    }
    let r = build_report_with(app, with_seen)?;
    post(&r).await?;
    // still the same install and still on? (it may have been turned off while sending)
    let kept = {
        let state = app.state::<AppState>();
        let db = state.db.lock().map_err(|_| "database lock poisoned")?;
        let kept = may_send(&db) && install_id(&db) == r.install_id;
        if kept {
            db.set_setting("share_last", &chrono::Local::now().format(TS).to_string()).map_err(|e| e.to_string())?;
            if let Some(last) = r.events.iter().map(|e| e.seq).max() {
                db.set_setting("share_last_seq", &last.to_string()).map_err(|e| e.to_string())?;
            }
            db.set_setting("share_last_report", &serde_json::to_string_pretty(&r).unwrap_or_default()).map_err(|e| e.to_string())?;
            if with_seen {
                db.set_setting("share_seen_last", &chrono::Local::now().format(TS).to_string()).map_err(|e| e.to_string())?;
            }
        }
        kept
    };
    if !kept {
        let _ = forget(&r.install_id).await;
    }
    Ok(kept)
}

fn kick(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        if let Err(e) = send_if_due(&app).await {
            eprintln!("stats: {e}");
        }
    });
}

/// Checks every 10 minutes (first after 3, so the live meter has a RAM average); sends at most hourly.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(180));
        loop {
            if let Err(e) = tauri::async_runtime::block_on(send_if_due(&app)) {
                eprintln!("stats: {e}");
            }
            std::thread::sleep(Duration::from_secs(600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_follows_region_until_chosen() {
        let db = Db::open_in_memory().unwrap();
        assert_eq!(enabled_with(&db, Consent::OptOut), (true, false));
        assert_eq!(enabled_with(&db, Consent::OptIn), (false, false));
        db.set_setting("share", "off").unwrap();
        assert_eq!(enabled_with(&db, Consent::OptOut), (false, true));
        db.set_setting("share", "on").unwrap();
        assert_eq!(enabled_with(&db, Consent::OptIn), (true, true));
    }

    #[test]
    fn nothing_sent_before_the_notice_and_hourly_after() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("share", "on").unwrap();
        assert!(!may_send(&db));
        db.set_setting("share_notice", NOTICE).unwrap();
        assert!(may_send(&db));
        assert!(due(&db));
        let ten_min = (chrono::Local::now() - chrono::Duration::minutes(10)).format(TS).to_string();
        db.set_setting("share_last", &ten_min).unwrap();
        assert!(!due(&db));
        let hour = (chrono::Local::now() - chrono::Duration::minutes(61)).format(TS).to_string();
        db.set_setting("share_last", &hour).unwrap();
        assert!(due(&db));
        db.set_setting("share", "off").unwrap();
        assert!(!may_send(&db));
    }

    #[test]
    fn each_cleanup_row_is_sent_once_as_is() {
        let db = Db::open_in_memory().unwrap();
        db.add_impact("gui", &[crate::impact::Freed::new("disk", "dir:target", 1000, 1), crate::impact::Freed::new("memory", "mcp", 50, 2)]);
        let first = db.impact_events_after(last_seq(&db), MAX_EVENTS).unwrap();
        assert_eq!(first.len(), 2);
        assert_eq!((first[0].kind.as_str(), first[0].bytes, first[0].source.as_str()), ("dir:target", 1000, "gui"));
        assert_eq!(first[0].run, first[1].run);
        assert!(first[0].at.ends_with('Z') && first[0].at.len() == 17, "{}", first[0].at);
        // after the server accepted them, only newer rows go out
        db.set_setting("share_last_seq", &first.iter().map(|e| e.seq).max().unwrap().to_string()).unwrap();
        assert!(db.impact_events_after(last_seq(&db), MAX_EVENTS).unwrap().is_empty());
        db.add_impact("auto", &[crate::impact::Freed::new("browser", "browser:cache", 70, 1)]);
        let next = db.impact_events_after(last_seq(&db), MAX_EVENTS).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].source, "auto");
        // a new install id starts over
        rotate(&db).unwrap();
        assert_eq!(db.impact_events_after(last_seq(&db), MAX_EVENTS).unwrap().len(), 3);
    }

    #[test]
    fn a_new_notice_version_pauses_sending_until_seen() {
        let db = Db::open_in_memory().unwrap();
        db.set_setting("share", "on").unwrap();
        db.set_setting("share_notice", "1").unwrap(); // agreed before the vendor section existed
        assert!(!may_send(&db));
        db.set_setting("share_notice", NOTICE).unwrap();
        assert!(may_send(&db));
    }

    #[test]
    fn ids_are_valid_and_rotate() {
        let db = Db::open_in_memory().unwrap();
        let a = install_id(&db);
        assert!(report::install_id_ok(&a));
        assert_eq!(install_id(&db), a);
        rotate(&db).unwrap();
        assert_ne!(install_id(&db), a);
    }
}

#[cfg(test)]
mod net {
    /// `cargo test -- --ignored https_works`: the TLS setup can reach a real HTTPS server.
    #[test]
    #[ignore]
    fn https_works() {
        let r = tauri::async_runtime::block_on(async { super::client().unwrap().head("https://github.com").send().await });
        assert!(r.unwrap().status().is_success());
    }
}
