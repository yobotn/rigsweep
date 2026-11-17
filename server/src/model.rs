//! The report the app sends (see src-tauri/src/report.rs) and the checks it must pass. Anything
//! that doesn't look like a category, a number or a known app name is refused, so free text
//! (a path, a project name) can't end up in the database by accident.

use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Freed {
    pub area: String,
    pub kind: String,
    pub bytes: u64,
    pub count: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct AppUse {
    pub app: String,
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Machine {
    pub ram_total: u64,
    pub ram_used_avg: u64,
    pub cores: u32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub v: u32,
    pub install_id: String,
    pub app_version: String,
    pub windows_build: String,
    pub processes: u64,
    pub cleanups: u64,
    pub freed: Vec<Freed>,
    pub machine: Machine,
    pub top_cpu: Vec<AppUse>,
    pub top_mem: Vec<AppUse>,
    pub tweaks: Vec<String>,
}

const TB: u64 = 1 << 40;

fn chars_ok(s: &str, max: usize, ok: impl Fn(char) -> bool) -> bool {
    !s.is_empty() && s.len() <= max && s.chars().all(ok)
}

pub fn install_id_ok(id: &str) -> bool {
    id.len() == 32 && id.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

fn kind_ok(k: &str) -> bool {
    chars_ok(k, 48, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.:+-".contains(c))
}

fn app_ok(a: &str) -> bool {
    chars_ok(a, 40, |c| c.is_ascii_alphanumeric() || " ./+-#".contains(c))
}

impl Report {
    /// Ok, or what's wrong (sent back as the 400 body).
    pub fn validate(&self) -> Result<(), String> {
        let bad = |what: &str| Err(format!("invalid {what}"));
        if self.v != 1 {
            return bad("version");
        }
        if !install_id_ok(&self.install_id) {
            return bad("install_id");
        }
        if !chars_ok(&self.app_version, 24, |c| c.is_ascii_alphanumeric() || ".+-".contains(c)) {
            return bad("app_version");
        }
        if !chars_ok(&self.windows_build, 10, |c| c.is_ascii_digit()) {
            return bad("windows_build");
        }
        if self.processes > 1_000_000 || self.cleanups > 100_000 {
            return bad("counts");
        }
        if self.freed.len() > 120
            || self.freed.iter().any(|f| {
                !matches!(f.area.as_str(), "memory" | "disk" | "browser") || !kind_ok(&f.kind) || f.bytes > 64 * TB || f.count > 1_000_000
            })
        {
            return bad("freed");
        }
        let m = &self.machine;
        if m.ram_total > 16 * TB || m.ram_used_avg > m.ram_total.max(1) * 2 || m.cores > 4096 {
            return bad("machine");
        }
        for list in [&self.top_cpu, &self.top_mem] {
            if list.len() > 10 || list.iter().any(|a| !app_ok(&a.app) || !(0.0..=100_000.0).contains(&a.cpu) || a.mem > 16 * TB) {
                return bad("apps");
            }
        }
        if self.tweaks.len() > 300 || self.tweaks.iter().any(|t| !chars_ok(t, 64, |c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c))) {
            return bad("tweaks");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample() -> Report {
        serde_json::from_value(serde_json::json!({
            "v": 1, "install_id": "0123456789abcdef0123456789abcdef", "app_version": "0.8.0", "windows_build": "26300",
            "processes": 3, "cleanups": 2,
            "freed": [{"area": "disk", "kind": "dir:target", "bytes": 1000, "count": 1}, {"area": "memory", "kind": "mcp", "bytes": 50, "count": 2}],
            "machine": {"ram_total": 34359738368u64, "ram_used_avg": 17179869184u64, "cores": 16},
            "top_cpu": [{"app": "VS Code", "cpu": 12.5, "mem": 900}], "top_mem": [{"app": "Chrome", "cpu": 3.0, "mem": 4000}],
            "tweaks": ["telemetry.diagtrack"]
        }))
        .unwrap()
    }

    #[test]
    fn accepts_a_real_report() {
        assert_eq!(sample().validate(), Ok(()));
    }

    #[test]
    fn refuses_free_text_where_categories_belong() {
        let mut r = sample();
        r.freed[0].kind = r"C:\Users\me\secret-project".into();
        assert!(r.validate().is_err());
        let mut r = sample();
        r.top_cpu[0].app = "acme_internal<script>".into();
        assert!(r.validate().is_err());
        let mut r = sample();
        r.install_id = "ABC".into();
        assert!(r.validate().is_err());
        let mut r = sample();
        r.freed[0].area = "files".into();
        assert!(r.validate().is_err());
    }

    #[test]
    fn refuses_unknown_fields() {
        let mut v = serde_json::to_value(serde_json::json!({"hostname": "my-pc"})).unwrap();
        v["v"] = 1.into();
        assert!(serde_json::from_value::<Report>(v).is_err());
    }
}
