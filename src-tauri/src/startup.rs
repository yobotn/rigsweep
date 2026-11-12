//! Everything that starts on its own at boot or sign-in, including what Task Manager's "Startup apps"
//! never shows: third-party auto-start services and scheduled tasks. Turning an item off creates a
//! custom tweak, so it gets the usual backup, Restore and watchdog re-apply.
use serde::{Deserialize, Serialize};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

use crate::actions::{self, service, startup_approved as sa, Status};
use crate::catalog::{is_protected_service, Action, Catalog, Hive, Risk, StartType, Tweak};
use crate::util;

#[derive(Serialize, Clone, Debug)]
pub struct StartupItem {
    /// service | task | run | folder
    pub kind: &'static str,
    /// what add_startup_tweak needs to find it again
    pub key: String,
    pub name: String,
    pub command: String,
    /// file whose icon represents the item
    pub exe: String,
    /// registry key, folder or task path (technical, shown as a tooltip)
    pub location: String,
    /// machine | user
    pub scope: &'static str,
    /// scheduled tasks: logon | boot | unlock | schedule | idle | event | other
    pub triggers: Vec<String>,
    pub running: bool,
    /// driver-package software (audio, graphics, OEM utilities): turning it off can cost a feature
    pub hardware: bool,
    /// tweak that already covers this item (curated or one you added)
    pub tweak_id: Option<String>,
}

fn hive_name(h: Hive) -> &'static str {
    match h {
        Hive::HKLM => "HKLM",
        Hive::HKCU => "HKCU",
    }
}

fn parse_hive(s: &str) -> Option<Hive> {
    match s {
        "HKLM" => Some(Hive::HKLM),
        "HKCU" => Some(Hive::HKCU),
        _ => None,
    }
}

fn root(h: Hive) -> RegKey {
    RegKey::predef(match h {
        Hive::HKLM => HKEY_LOCAL_MACHINE,
        Hive::HKCU => HKEY_CURRENT_USER,
    })
}

pub fn task_tweak_id(path: &str) -> String {
    format!("custom.task.{}", path.to_ascii_lowercase())
}

pub fn run_tweak_id(h: Hive, list: &str, name: &str) -> String {
    format!("custom.run.{}.{}.{}", hive_name(h).to_ascii_lowercase(), list.to_ascii_lowercase(), name.to_ascii_lowercase())
}

/// Tweak (curated or custom) that already manages this service.
fn service_tweak(catalog: &Catalog, name: &str) -> Option<String> {
    catalog
        .tweaks
        .iter()
        .find(|t| t.actions.iter().any(|a| matches!(a, Action::Service { name: n, .. } if n.eq_ignore_ascii_case(name))))
        .map(|t| t.id.clone())
}

/// Executable path of a service command line, lower-cased, env vars expanded, quotes removed.
fn image_path(cmd: &str) -> String {
    let c = util::expand_env(cmd.trim()).to_ascii_lowercase();
    let c = c.strip_prefix(r"\??\").unwrap_or(&c).to_string();
    let c = if let Some(r) = c.strip_prefix(r"\systemroot\") { format!(r"c:\windows\{r}") } else { c };
    let c = if c.starts_with(r"system32\") { format!(r"c:\windows\{c}") } else { c };
    match c.strip_prefix('"') {
        Some(r) => r.split('"').next().unwrap_or("").to_string(),
        None => c,
    }
}

/// None = part of Windows itself (never listed). Some(true) = driver-package software.
fn classify_service(img: &str) -> Option<bool> {
    let windir = util::expand_env("%SystemRoot%").to_ascii_lowercase();
    let windir = if windir.contains('%') { r"c:\windows".to_string() } else { windir };
    if img.contains(r"\driverstore\") {
        return Some(true);
    }
    if img.is_empty() || img.starts_with(&format!(r"{windir}\")) || img.contains(r"\windows defender\") {
        return None;
    }
    Some(false)
}

fn services(catalog: &Catalog) -> Vec<StartupItem> {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut out = vec![];
    for s in service::list_services().unwrap_or_default() {
        if is_protected_service(&s.name) {
            continue;
        }
        let Ok(k) = hk.open_subkey_with_flags(format!(r"SYSTEM\CurrentControlSet\Services\{}", s.name), KEY_READ) else {
            continue;
        };
        if k.get_value::<u32, _>("Start").ok() != Some(2) {
            continue;
        }
        // per-user service templates and instances (SERVICE_USER_SERVICE / _INSTANCE) belong to Windows
        if k.get_value::<u32, _>("Type").unwrap_or(0) & 0xC0 != 0 {
            continue;
        }
        let cmd: String = k.get_value("ImagePath").unwrap_or_default();
        let Some(hardware) = classify_service(&image_path(&cmd)) else { continue };
        out.push(StartupItem {
            kind: "service",
            key: s.name.clone(),
            name: if s.display.is_empty() { s.name.clone() } else { s.display.clone() },
            exe: crate::icons::exe_from_command(&cmd),
            command: cmd,
            location: format!(r"HKLM\SYSTEM\CurrentControlSet\Services\{}", s.name),
            scope: "machine",
            triggers: vec![],
            running: s.running,
            hardware,
            tweak_id: service_tweak(catalog, &s.name),
        });
    }
    out
}

#[derive(Deserialize)]
struct PsTask {
    p: String,
    #[serde(default)]
    t: Vec<String>,
    #[serde(default)]
    a: Vec<String>,
    #[serde(default)]
    s: String,
}

fn trigger_code(cim: &str) -> &'static str {
    match cim {
        "MSFT_TaskLogonTrigger" => "logon",
        "MSFT_TaskBootTrigger" => "boot",
        "MSFT_TaskSessionStateChangeTrigger" => "unlock",
        "MSFT_TaskDailyTrigger" | "MSFT_TaskWeeklyTrigger" | "MSFT_TaskTimeTrigger" => "schedule",
        "MSFT_TaskIdleTrigger" => "idle",
        "MSFT_TaskEventTrigger" => "event",
        _ => "other",
    }
}

/// Tasks under \Microsoft\Windows\ belong to Windows and are never offered.
fn is_windows_task(path: &str) -> bool {
    path.to_ascii_lowercase().starts_with(r"\microsoft\windows\")
}

fn tasks() -> Vec<StartupItem> {
    // ScheduledTasks module: language-independent and gives trigger types directly
    let script = r#"$r = @(Get-ScheduledTask | Where-Object { $_.State -ne 'Disabled' -and $_.TaskPath -notlike '\Microsoft\*' } | ForEach-Object {
  [pscustomobject]@{ p = $_.TaskPath + $_.TaskName; s = [string]$_.State;
    t = @($_.Triggers | Where-Object { $_ -and $_.Enabled -ne $false } | ForEach-Object { $_.CimClass.CimClassName });
    a = @($_.Actions | ForEach-Object { if ($_.Execute) { ("$($_.Execute) $($_.Arguments)").Trim() } else { 'COM handler' } }) } });
ConvertTo-Json -InputObject $r -Compress -Depth 4"#;
    let Ok(json) = util::powershell(script) else { return vec![] };
    let list: Vec<PsTask> = serde_json::from_str(json.trim()).unwrap_or_default();
    list.into_iter()
        .filter(|x| !is_windows_task(&x.p))
        .map(|x| {
            let mut trig: Vec<String> = x.t.iter().map(|c| trigger_code(c).to_string()).collect();
            trig.sort();
            trig.dedup();
            let name = x.p.rsplit('\\').next().unwrap_or(&x.p).to_string();
            StartupItem {
                kind: "task",
                key: x.p.clone(),
                name,
                exe: x.a.first().map(|c| crate::icons::exe_from_command(c)).unwrap_or_default(),
                command: x.a.join(" · "),
                location: x.p.clone(),
                // tasks a user created show up per-user with their SID in the name or path
                scope: if x.p.contains("S-1-5-21-") { "user" } else { "machine" },
                triggers: trig,
                running: x.s == "Running",
                hardware: false,
                tweak_id: None,
            }
        })
        .collect()
}

fn run_entries() -> Vec<StartupItem> {
    let mut out = vec![];
    for (h, list) in [(Hive::HKCU, "Run"), (Hive::HKLM, "Run"), (Hive::HKLM, "Run32")] {
        let Some(path) = sa::run_key(h, list) else { continue };
        let Ok(k) = root(h).open_subkey_with_flags(path, KEY_READ) else { continue };
        for (name, _) in k.enum_values().flatten() {
            if name.is_empty() || sa::is_disabled(h, list, &name) {
                continue;
            }
            let cmd: String = k.get_value(&name).unwrap_or_default();
            out.push(StartupItem {
                kind: "run",
                key: format!("{}|{list}|{name}", hive_name(h)),
                name: name.clone(),
                exe: crate::icons::exe_from_command(&cmd),
                command: cmd,
                location: format!(r"{}\{path}", hive_name(h)),
                scope: if h == Hive::HKLM { "machine" } else { "user" },
                triggers: vec![],
                running: false,
                hardware: false,
                tweak_id: None,
            });
        }
    }
    for h in [Hive::HKCU, Hive::HKLM] {
        let dir = sa::startup_folder(h);
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("desktop.ini") || sa::is_disabled(h, "StartupFolder", &name) {
                continue;
            }
            out.push(StartupItem {
                kind: "folder",
                key: format!("{}|StartupFolder|{name}", hive_name(h)),
                name: name.trim_end_matches(".lnk").to_string(),
                exe: e.path().display().to_string(),
                command: e.path().display().to_string(),
                location: dir.display().to_string(),
                scope: if h == Hive::HKLM { "machine" } else { "user" },
                triggers: vec![],
                running: false,
                hardware: false,
                tweak_id: None,
            });
        }
    }
    out
}

pub fn scan(catalog: &Catalog) -> Vec<StartupItem> {
    let mut all = services(catalog);
    all.extend(tasks());
    all.extend(run_entries());
    for it in all.iter_mut() {
        if it.tweak_id.is_none() {
            let id = match it.kind {
                "task" => task_tweak_id(&it.key),
                _ => match split_key(&it.key) {
                    Some((h, list, name)) => run_tweak_id(h, list, name),
                    None => continue,
                },
            };
            if catalog.get(&id).is_some() {
                it.tweak_id = Some(id);
            }
        }
    }
    all.sort_by(|a, b| a.kind.cmp(b.kind).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    all
}

fn split_key(key: &str) -> Option<(Hive, &str, &str)> {
    let mut p = key.splitn(3, '|');
    let h = parse_hive(p.next()?)?;
    let list = p.next()?;
    let name = p.next()?;
    sa::LISTS.contains(&list).then_some((h, list, name))
}

/// Id of a tweak that already covers this item, if one exists.
pub fn existing_tweak(catalog: &Catalog, kind: &str, key: &str) -> Option<String> {
    let id = match kind {
        "service" => return service_tweak(catalog, key),
        "task" => task_tweak_id(key),
        _ => {
            let (h, list, name) = split_key(key)?;
            run_tweak_id(h, list, name)
        }
    };
    catalog.get(&id).map(|t| t.id.clone())
}

const OFF_WARNING: &str = "It no longer starts on its own. Open the app yourself when you need it, or restore it if something stops working.";

/// Build the custom tweak that turns one startup item off. Never trusts the caller's description:
/// the action is derived from the key and checked against the live system.
pub fn build_tweak(catalog: &Catalog, kind: &str, key: &str, display: &str) -> Result<Tweak, String> {
    let display = display.trim();
    match kind {
        "service" => {
            if is_protected_service(key) {
                return Err(format!("{key} is a protected core service"));
            }
            if let Some(id) = service_tweak(catalog, key) {
                return Err(format!("already covered by tweak {id}"));
            }
            let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
            let k = hk
                .open_subkey_with_flags(format!(r"SYSTEM\CurrentControlSet\Services\{key}"), KEY_READ)
                .map_err(|_| format!("service {key} not found"))?;
            let cmd: String = k.get_value("ImagePath").unwrap_or_default();
            let hardware = classify_service(&image_path(&cmd)).ok_or_else(|| format!("{key} is part of Windows"))?;
            Ok(Tweak {
                id: format!("custom.svc.{}", key.to_ascii_lowercase()),
                category: "custom".into(),
                name: format!("{display} ({key})"),
                description: "Auto-start service, switched to Manual from the Startup tab: it no longer starts at boot, but apps can still start it when they need it.".into(),
                risk: if hardware { Risk::Caution } else { Risk::Moderate },
                warning: Some(if hardware {
                    "This service comes with a device driver package. The device can lose extra features (hotkeys, audio effects, control panel) until you restore it.".into()
                } else {
                    OFF_WARNING.into()
                }),
                // Manual, not Disabled: the app can still start it on demand
                actions: vec![Action::Service { name: key.to_string(), start: StartType::Manual, stop: true }],
                custom: true,
            })
        }
        "task" => {
            if is_windows_task(key) || !key.starts_with('\\') {
                return Err("Windows tasks can't be turned off here".into());
            }
            let a = Action::Task { path: key.to_string() };
            if actions::task::status(key).map_err(|e| e.to_string())? == Status::NotPresent {
                return Err(format!("task {key} not found"));
            }
            Ok(Tweak {
                id: task_tweak_id(key),
                category: "custom".into(),
                name: format!("Task: {display}"),
                description: format!("Scheduled task {key}, disabled from the Startup tab."),
                risk: Risk::Moderate,
                warning: Some(OFF_WARNING.into()),
                actions: vec![a],
                custom: true,
            })
        }
        "run" | "folder" => {
            let (h, list, name) = split_key(key).ok_or("bad startup key")?;
            let a = Action::StartupApproved { hive: h, list: list.into(), name: name.into() };
            crate::catalog::validate_action(&a)?;
            if sa::status(h, list, name).map_err(|e| e.to_string())? == Status::NotPresent {
                return Err(format!("startup entry {name} not found"));
            }
            Ok(Tweak {
                id: run_tweak_id(h, list, name),
                category: "custom".into(),
                name: format!("Startup app: {display}"),
                description: format!("{} startup entry, turned off from the Startup tab (same switch as Task Manager).", hive_name(h)),
                risk: Risk::Moderate,
                warning: Some(OFF_WARNING.into()),
                actions: vec![a],
                custom: true,
            })
        }
        _ => Err(format!("unknown startup item kind {kind}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_paths() {
        assert_eq!(classify_service(&image_path(r#""C:\Program Files\Fortinet\FortiClient\scheduler.exe""#)), Some(false));
        assert_eq!(classify_service(&image_path(r"C:\Windows\system32\svchost.exe -k netsvcs")), None);
        assert_eq!(classify_service(&image_path(r"%SystemRoot%\System32\svchost.exe -k x")), None);
        assert_eq!(classify_service(&image_path(r"system32\DRIVERS\x.sys")), None);
        assert_eq!(
            classify_service(&image_path(r"C:\Windows\System32\DriverStore\FileRepository\a.inf_amd64_1\Svc.exe")),
            Some(true)
        );
        assert_eq!(
            classify_service(&image_path(r#""C:\ProgramData\Microsoft\Windows Defender\Platform\4.18\MsMpEng.exe""#)),
            None
        );
    }

    #[test]
    fn keys_and_ids() {
        let (h, l, n) = split_key("HKCU|Run|Docker Desktop").unwrap();
        assert_eq!((h, l, n), (Hive::HKCU, "Run", "Docker Desktop"));
        assert!(split_key("HKCU|Evil|x").is_none());
        assert!(split_key("HKXX|Run|x").is_none());
        assert_eq!(run_tweak_id(Hive::HKLM, "Run32", "Java"), "custom.run.hklm.run32.java");
        assert!(is_windows_task(r"\Microsoft\Windows\Defrag\ScheduledDefrag"));
        assert!(!is_windows_task(r"\MicrosoftEdgeUpdateTaskMachineCore"));
    }

    #[test]
    fn rejects_windows_items() {
        let c = Catalog::builtin();
        assert!(build_tweak(&c, "service", "RpcSs", "RPC").is_err());
        assert!(build_tweak(&c, "task", r"\Microsoft\Windows\Defrag\ScheduledDefrag", "x").is_err());
        assert!(build_tweak(&c, "folder", "HKCU|StartupFolder|..\\x.lnk", "x").is_err());
        assert!(build_tweak(&c, "nope", "x", "x").is_err());
    }

    /// Read-only: prints what the Startup tab would list on this machine.
    /// Run with: cargo test live_startup -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_startup() {
        for i in scan(&Catalog::builtin()) {
            println!("{:<8} {:<45} hw={} run={} trig={:?} tweak={:?}  {}", i.kind, i.name, i.hardware, i.running, i.triggers, i.tweak_id, i.key);
        }
    }
}
