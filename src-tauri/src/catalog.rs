use serde::{Deserialize, Serialize};
use serde_json::Value;

const BUILTIN: &str = include_str!("../catalog/tweaks.json");

/// Services that must never be modified, even through custom tweaks.
pub const PROTECTED_SERVICES: &[&str] = &[
    "rpcss", "dcomlaunch", "rpceptmapper", "lsm", "samss", "plugplay", "power", "bfe",
    "mpssvc", "windefend", "wdnissvc", "securityhealthservice", "sense", "eventlog",
    "schedule", "cryptsvc", "dhcp", "dnscache", "nsi", "audiosrv", "audioendpointbuilder",
    "winmgmt", "profsvc", "usermanager", "brokerinfrastructure", "coremessagingregistrar",
    "staterepository", "trustedinstaller", "wuauserv", "waasmedicsvc", "usosvc", "bits",
    "lanmanworkstation", "netprofm", "nlasvc", "wlansvc", "eventsystem", "sens", "themes",
    "textinputmanagementservice", "tabletinputservice", "appinfo", "gpsvc", "keyiso",
    "vaultsvc", "seclogon", "wscsvc", "lsass", "csrss", "systemeventsbroker", "timebrokersvc",
    "dispbrokerdesktopsvc", "fontcache", "shellhwdetection", "wpnservice", "camsvc",
    "w32time", "netlogon", "ncbservice", "msiserver", "cbdhsvc", "deviceinstall",
];

/// Processes that can never be killed from the analyzer.
pub const PROTECTED_PROCESSES: &[&str] = &[
    "system", "idle", "system idle process", "registry", "smss.exe", "csrss.exe",
    "wininit.exe", "winlogon.exe", "lsass.exe", "lsaiso.exe", "services.exe", "svchost.exe",
    "dwm.exe", "fontdrvhost.exe", "memory compression", "secure system", "explorer.exe",
    "msmpeng.exe", "sihost.exe", "ctfmon.exe", "conhost.exe", "rigsweep.exe", "aiboost.exe",
];

pub fn is_protected_service(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    // per-user service instances look like "CDPUserSvc_1a2b3"; never touch those
    n.contains('_') || PROTECTED_SERVICES.contains(&n.as_str())
}

pub fn is_protected_process(name: &str) -> bool {
    PROTECTED_PROCESSES.contains(&name.to_ascii_lowercase().as_str())
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    Safe,
    Moderate,
    Caution,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum StartType {
    Disabled,
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hive {
    HKLM,
    HKCU,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RegKind {
    Dword,
    String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    Service {
        name: String,
        start: StartType,
        #[serde(default)]
        stop: bool,
    },
    Task {
        path: String,
    },
    Registry {
        hive: Hive,
        path: String,
        value: String,
        kind: RegKind,
        data: Value,
    },
    DefenderExclusion {
        #[serde(default)]
        processes: Vec<String>,
        #[serde(default)]
        use_dev_folders: bool,
    },
    PowerPlan {
        scheme: String,
    },
}

impl Action {
    /// Human-readable target, also used as the backup key description.
    pub fn target(&self) -> String {
        match self {
            Action::Service { name, start, .. } => format!("service {name} -> {start:?}"),
            Action::Task { path } => format!("task {path} -> disabled"),
            Action::Registry { hive, path, value, data, .. } => {
                format!("{hive:?}\\{path}\\{value} = {data}")
            }
            Action::DefenderExclusion { processes, use_dev_folders } => {
                if *use_dev_folders {
                    "defender exclusions: dev folders".into()
                } else {
                    format!("defender exclusions: {}", processes.join(", "))
                }
            }
            Action::PowerPlan { scheme } => format!("power plan -> {scheme}"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Tweak {
    pub id: String,
    pub category: String,
    pub name: String,
    pub description: String,
    pub risk: Risk,
    #[serde(default)]
    pub warning: Option<String>,
    pub actions: Vec<Action>,
    #[serde(default)]
    pub custom: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Catalog {
    pub categories: Vec<Category>,
    pub tweaks: Vec<Tweak>,
}

impl Catalog {
    pub fn builtin() -> Catalog {
        let c: Catalog = serde_json::from_str(BUILTIN).expect("built-in catalog is valid JSON");
        c
    }

    /// Built-in catalog merged with user custom tweaks.
    pub fn with_custom(custom: Vec<Tweak>) -> Catalog {
        let mut c = Self::builtin();
        for t in custom {
            if !c.tweaks.iter().any(|x| x.id == t.id) {
                c.tweaks.push(t);
            }
        }
        c
    }

    pub fn get(&self, id: &str) -> Option<&Tweak> {
        self.tweaks.iter().find(|t| t.id == id)
    }

    #[allow(dead_code)] // used by tests / catalog CI check
    pub fn validate(&self) -> Result<(), String> {
        let mut seen = std::collections::HashSet::new();
        for t in &self.tweaks {
            if !seen.insert(&t.id) {
                return Err(format!("duplicate tweak id {}", t.id));
            }
            if !self.categories.iter().any(|c| c.id == t.category) {
                return Err(format!("tweak {} has unknown category {}", t.id, t.category));
            }
            if t.risk == Risk::Caution && t.warning.is_none() {
                return Err(format!("caution tweak {} has no warning", t.id));
            }
            for a in &t.actions {
                validate_action(a).map_err(|e| format!("{}: {e}", t.id))?;
            }
        }
        Ok(())
    }
}

pub fn validate_action(a: &Action) -> Result<(), String> {
    match a {
        Action::Service { name, .. } if is_protected_service(name) => {
            Err(format!("service {name} is protected"))
        }
        Action::Registry { kind: RegKind::Dword, data, .. } if data.as_u64().map_or(true, |v| v > u32::MAX as u64) => {
            Err("dword data must be an unsigned 32-bit number".into())
        }
        Action::Registry { kind: RegKind::String, data, .. } if !data.is_string() => {
            Err("string data must be a string".into())
        }
        _ => Ok(()),
    }
}

/// Build a custom tweak that disables a single service.
pub fn custom_service_tweak(name: &str, display: &str) -> Result<Tweak, String> {
    if is_protected_service(name) {
        return Err(format!("{name} is a protected core service and can't be disabled"));
    }
    Ok(Tweak {
        id: format!("custom.svc.{}", name.to_ascii_lowercase()),
        category: "custom".into(),
        name: format!("{display} ({name})"),
        description: "Added from the Process Analyzer.".into(),
        risk: Risk::Caution,
        warning: Some(format!(
            "'{display}' is not in the curated catalog. Disabling unknown services can break features; restore it if anything stops working."
        )),
        actions: vec![Action::Service { name: name.to_string(), start: StartType::Disabled, stop: true }],
        custom: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_catalog_is_valid() {
        Catalog::builtin().validate().unwrap();
    }

    #[test]
    fn protected_services_rejected() {
        assert!(custom_service_tweak("RpcSs", "RPC").is_err());
        assert!(custom_service_tweak("CDPUserSvc_abc12", "x").is_err());
        assert!(custom_service_tweak("Fax", "Fax").is_ok());
    }

    #[test]
    fn protected_process() {
        assert!(is_protected_process("LSASS.exe"));
        assert!(!is_protected_process("node.exe"));
    }
}
