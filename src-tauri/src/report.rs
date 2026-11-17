//! The anonymous hourly report and the sharing default. This builds the exact payload (also shown
//! in the Preview) and decides, on this PC, whether sharing starts on (opt-out) or waits for a yes
//! (opt-in: EU/EEA, UK, Switzerland, unknown). Sending is in `share.rs`.
//!
//! What a report never holds: paths, project / folder / process names outside the known-app list,
//! command lines, user or machine names, hardware IDs, the region it was decided in.

use serde::Serialize;
use std::collections::HashMap;

pub const REPORT_VERSION: u32 = 2;

/// One thing cleaned, exactly as recorded in the local `impact` table (nothing summed).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Event {
    /// the local row number; the server stores each one once even if a report is resent
    pub seq: u64,
    /// UTC, to the minute: "2026-11-17T14:05Z"
    pub at: String,
    /// rows of one cleanup share a run
    pub run: i64,
    /// "gui" | "auto" | "watchdog" | "cli" | "import"
    pub source: String,
    pub area: String,
    pub kind: String,
    pub bytes: u64,
    pub count: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AppUse {
    pub app: String,
    /// average CPU % of all its processes (100 = one full core)
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Machine {
    pub ram_total: u64,
    /// average RAM in use over the live meter's window
    pub ram_used_avg: u64,
    pub cores: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct Report {
    pub v: u32,
    /// random, made on this PC, resettable; never derived from hardware
    pub install_id: String,
    pub app_version: String,
    pub windows_build: String,
    /// when the previous report was accepted (UTC, to the minute); absent on the first one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// every cleanup recorded since the previous report (all of them on the first report)
    pub events: Vec<Event>,
    pub machine: Machine,
    pub top_cpu: Vec<AppUse>,
    pub top_mem: Vec<AppUse>,
    /// built-in tweak ids in the profile (custom ones are left out: they name services)
    pub tweaks: Vec<String>,
    /// vendor software found on this PC (see discovery.rs), in at most one report a day
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<crate::discovery::Seen>,
}

/// Executables reported by name; everything else is left out of the app lists.
const KNOWN_APPS: &[(&str, &str)] = &[
    ("chrome.exe", "Chrome"), ("msedge.exe", "Edge"), ("firefox.exe", "Firefox"), ("brave.exe", "Brave"),
    ("opera.exe", "Opera"), ("vivaldi.exe", "Vivaldi"), ("arc.exe", "Arc"),
    ("code.exe", "VS Code"), ("cursor.exe", "Cursor"), ("windsurf.exe", "Windsurf"), ("zed.exe", "Zed"),
    ("devenv.exe", "Visual Studio"), ("idea64.exe", "IntelliJ IDEA"), ("rider64.exe", "Rider"),
    ("pycharm64.exe", "PyCharm"), ("webstorm64.exe", "WebStorm"), ("goland64.exe", "GoLand"), ("clion64.exe", "CLion"),
    ("studio64.exe", "Android Studio"), ("sublime_text.exe", "Sublime Text"), ("notepad++.exe", "Notepad++"),
    ("node.exe", "Node.js"), ("python.exe", "Python"), ("pythonw.exe", "Python"), ("java.exe", "Java"),
    ("javaw.exe", "Java"), ("dotnet.exe", ".NET"), ("bun.exe", "Bun"), ("deno.exe", "Deno"), ("go.exe", "Go"),
    ("cargo.exe", "Cargo"), ("rustc.exe", "rustc"), ("rust-analyzer.exe", "rust-analyzer"), ("msbuild.exe", "MSBuild"),
    ("git.exe", "Git"), ("bash.exe", "Bash"), ("pwsh.exe", "PowerShell"), ("powershell.exe", "PowerShell"),
    ("windowsterminal.exe", "Windows Terminal"), ("claude.exe", "Claude"), ("chatgpt.exe", "ChatGPT"),
    ("codex.exe", "Codex"), ("ollama.exe", "Ollama"), ("ollama app.exe", "Ollama"), ("lm studio.exe", "LM Studio"),
    ("docker desktop.exe", "Docker Desktop"), ("com.docker.backend.exe", "Docker Desktop"), ("vmmem", "WSL / Docker VM"),
    ("vmmemwsl", "WSL / Docker VM"), ("wslservice.exe", "WSL"), ("postman.exe", "Postman"),
    ("ms-teams.exe", "Teams"), ("teams.exe", "Teams"), ("slack.exe", "Slack"), ("discord.exe", "Discord"),
    ("zoom.exe", "Zoom"), ("outlook.exe", "Outlook"), ("olk.exe", "Outlook"), ("winword.exe", "Word"),
    ("excel.exe", "Excel"), ("onedrive.exe", "OneDrive"), ("dropbox.exe", "Dropbox"), ("notion.exe", "Notion"),
    ("obsidian.exe", "Obsidian"), ("spotify.exe", "Spotify"), ("whatsapp.exe", "WhatsApp"), ("telegram.exe", "Telegram"),
    ("steam.exe", "Steam"), ("steamwebhelper.exe", "Steam"), ("obs64.exe", "OBS"), ("photoshop.exe", "Photoshop"),
    ("figma.exe", "Figma"), ("msmpeng.exe", "Defender"), ("searchindexer.exe", "Windows Search"),
    ("explorer.exe", "Explorer"), ("dwm.exe", "Desktop Window Manager"), ("svchost.exe", "Windows services"),
    ("memory compression", "Memory compression"), ("system", "System"),
];

fn known_app(exe_name: &str) -> Option<&'static str> {
    let n = exe_name.to_lowercase();
    KNOWN_APPS.iter().find(|(e, _)| *e == n).map(|(_, a)| *a)
}

/// Sum (process name, cpu, mem) by known app; the top `n` by CPU and by memory.
pub fn top_apps(procs: &[(String, f32, u64)], n: usize) -> (Vec<AppUse>, Vec<AppUse>) {
    let mut by: HashMap<&'static str, (f32, u64)> = HashMap::new();
    for (name, cpu, mem) in procs {
        if let Some(app) = known_app(name) {
            let e = by.entry(app).or_default();
            e.0 += cpu;
            e.1 += mem;
        }
    }
    let all: Vec<AppUse> = by.into_iter().map(|(app, (cpu, mem))| AppUse { app: app.into(), cpu: (cpu * 10.0).round() / 10.0, mem }).collect();
    let mut cpu = all.clone();
    cpu.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(a.app.cmp(&b.app)));
    cpu.retain(|a| a.cpu > 0.0);
    cpu.truncate(n);
    let mut mem = all;
    mem.sort_by(|a, b| b.mem.cmp(&a.mem).then(a.app.cmp(&b.app)));
    mem.truncate(n);
    (cpu, mem)
}

pub struct Inputs<'a> {
    pub install_id: &'a str,
    pub app_version: &'a str,
    pub windows_build: &'a str,
    pub since: Option<String>,
    pub events: Vec<Event>,
    pub machine: Machine,
    pub procs: Vec<(String, f32, u64)>,
    pub tweaks: Vec<String>,
    pub seen: Vec<crate::discovery::Seen>,
}

pub fn build(i: Inputs) -> Report {
    let (top_cpu, top_mem) = top_apps(&i.procs, 5);
    let mut tweaks: Vec<String> = i.tweaks.into_iter().filter(|t| !t.starts_with("custom.")).collect();
    tweaks.sort();
    Report {
        v: REPORT_VERSION,
        install_id: i.install_id.into(),
        app_version: i.app_version.into(),
        windows_build: i.windows_build.into(),
        since: i.since,
        events: i.events,
        machine: i.machine,
        top_cpu,
        top_mem,
        tweaks,
        seen: i.seen,
    }
}

/// 32 lowercase hex characters, as made by `new_install_id` (the server checks the same).
pub fn install_id_ok(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// A new random install id: 128 bits from the OS-seeded hasher keys, as hex.
pub fn new_install_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let part = || {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        h.finish()
    };
    format!("{:016x}{:016x}", part(), part())
}

// ---------- the sharing default, decided on this PC ----------

/// EU/EEA, UK and Switzerland: sharing waits for a yes there.
const ASK_FIRST: &[&str] = &[
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IT", "LV", "LT", "LU", "MT",
    "NL", "PL", "PT", "RO", "SK", "SI", "ES", "SE", "IS", "LI", "NO", "GB", "CH",
];

/// Windows time zones that cover Europe (and so point to an ask-first region).
const EUROPE_TZ: &[&str] = &[
    "GMT Standard Time", "W. Europe Standard Time", "Central Europe Standard Time", "Central European Standard Time",
    "Romance Standard Time", "E. Europe Standard Time", "FLE Standard Time", "GTB Standard Time", "Kaliningrad Standard Time",
];

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    /// on unless turned off; a notice explains it
    OptOut,
    /// off until the user says yes
    OptIn,
}

/// `geo` = the Windows home location (ISO 3166 alpha-2), `tz` = the Windows time zone key name.
/// Unknown or conflicting signals take the careful side.
pub fn consent_default(geo: Option<&str>, tz: Option<&str>) -> Consent {
    let geo = geo.map(|g| g.trim().to_uppercase()).filter(|g| g.len() == 2);
    let tz_eu = tz.map(|t| EUROPE_TZ.contains(&t.trim()));
    match (geo, tz_eu) {
        (Some(g), _) if ASK_FIRST.contains(&g.as_str()) => Consent::OptIn,
        (Some(_), Some(false)) => Consent::OptOut,
        (Some(_), Some(true)) => Consent::OptIn, // a non-EU home location on a European clock: ask
        (Some(_), None) => Consent::OptOut,
        (None, Some(false)) => Consent::OptOut,
        _ => Consent::OptIn,
    }
}

fn reg(hive: winreg::HKEY, path: &str, value: &str) -> Option<String> {
    winreg::RegKey::predef(hive).open_subkey(path).ok()?.get_value::<String, _>(value).ok()
}

pub fn windows_build() -> String {
    reg(winreg::enums::HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", "CurrentBuild").unwrap_or_default()
}

/// The sharing default for this PC, from its home location and time zone.
pub fn local_consent_default() -> Consent {
    let geo = reg(winreg::enums::HKEY_CURRENT_USER, r"Control Panel\International\Geo", "Name");
    let tz = reg(winreg::enums::HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\TimeZoneInformation", "TimeZoneKeyName");
    consent_default(geo.as_deref(), tz.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_decides_the_default_carefully() {
        assert_eq!(consent_default(Some("US"), Some("Pacific Standard Time")), Consent::OptOut);
        assert_eq!(consent_default(Some("IL"), Some("Israel Standard Time")), Consent::OptOut);
        assert_eq!(consent_default(Some("BR"), None), Consent::OptOut);
        assert_eq!(consent_default(Some("de"), Some("W. Europe Standard Time")), Consent::OptIn);
        assert_eq!(consent_default(Some("GB"), Some("Pacific Standard Time")), Consent::OptIn); // travelling: home wins
        assert_eq!(consent_default(Some("US"), Some("Romance Standard Time")), Consent::OptIn); // conflicting
        assert_eq!(consent_default(None, Some("Tokyo Standard Time")), Consent::OptOut);
        assert_eq!(consent_default(None, Some("GMT Standard Time")), Consent::OptIn);
        assert_eq!(consent_default(None, None), Consent::OptIn);
        assert_eq!(consent_default(Some("??x"), None), Consent::OptIn);
    }

    #[test]
    fn only_known_apps_are_named() {
        let procs = vec![
            ("chrome.exe".to_string(), 10.0, 500),
            ("Chrome.exe".to_string(), 5.0, 300),
            ("acme-secret-tool.exe".to_string(), 90.0, 9000),
            ("Code.exe".to_string(), 1.0, 900),
        ];
        let (cpu, mem) = top_apps(&procs, 5);
        assert_eq!(cpu[0], AppUse { app: "Chrome".into(), cpu: 15.0, mem: 800 });
        assert_eq!(mem[0].app, "VS Code");
        assert!(cpu.iter().chain(&mem).all(|a| !a.app.contains("acme")));
    }

    #[test]
    fn report_drops_custom_tweaks_and_ids_are_random() {
        let r = build(Inputs {
            install_id: "x",
            app_version: "0.8.0",
            windows_build: "26300",
            since: None,
            events: vec![],
            machine: Machine { ram_total: 1, ram_used_avg: 1, cores: 8 },
            procs: vec![],
            tweaks: vec!["telemetry.diagtrack".into(), "custom.svc.adobearmservice".into()],
            seen: vec![],
        });
        assert_eq!(r.tweaks, vec!["telemetry.diagtrack"]);
        let (a, b) = (new_install_id(), new_install_id());
        assert_eq!(a.len(), 32);
        assert_ne!(a, b);
    }
}

#[cfg(test)]
mod wire {
    /// `RIGSWEEP_REPORT_OUT=path cargo test -- --ignored dump_report`: writes a real report for the
    /// server's contract check (server/tests/contract.rs).
    #[test]
    #[ignore]
    fn dump_report() {
        use super::*;
        let db = crate::db::Db::open_in_memory().unwrap();
        db.add_impact("gui", &[crate::impact::Freed::new("disk", "dir:node_modules", 5_000_000, 2), crate::impact::Freed::new("memory", "mcp", 900_000, 3)]);
        db.add_impact("auto", &[crate::impact::Freed::new("browser", "browser:cache", 70_000, 1)]);
        let r = build(Inputs {
            install_id: &new_install_id(),
            app_version: env!("CARGO_PKG_VERSION"),
            windows_build: &windows_build(),
            since: Some("2026-11-17T13:00Z".into()),
            events: db.impact_events_after(0, 5000).unwrap(),
            machine: Machine { ram_total: 34_000_000_000, ram_used_avg: 20_000_000_000, cores: 16 },
            procs: vec![("chrome.exe".into(), 12.5, 4_000_000_000), ("Code.exe".into(), 3.0, 2_000_000_000), ("secret.exe".into(), 50.0, 1)],
            tweaks: vec!["telemetry.diagtrack".into()],
            seen: crate::discovery::collect(&[], &[crate::analyzer::ProcInfo {
                pid: 1, start_time: 0, name: "ArmouryCrate.Service.exe".into(), exe: r"C:\Program Files\ASUS\ArmouryCrate\ArmouryCrate.Service.exe".into(),
                cpu: 1.5, mem: 80_000_000, disk_read: 0, disk_write: 0, services: vec![], protected: false,
            }], &Default::default()),
        });
        std::fs::write(std::env::var("RIGSWEEP_REPORT_OUT").unwrap(), serde_json::to_string(&r).unwrap()).unwrap();
    }
}
