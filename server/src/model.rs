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

/// One thing cleaned, as recorded on the PC (report v2).
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Event {
    /// the PC's own row number, increasing; lets a resent report be stored once
    pub seq: u64,
    /// UTC, to the minute: "2026-11-17T14:05Z"
    pub at: String,
    /// rows of one cleanup share a run (negative for records rebuilt from old History)
    pub run: i64,
    pub source: String,
    pub area: String,
    pub kind: String,
    pub bytes: u64,
    pub count: u64,
    /// memory rows: CPU (% of the whole CPU) the ended processes were using; absent before 0.8.2
    #[serde(default)]
    pub cpu: f32,
}

/// v1 (0.8.0 test builds): totals for the last 7 days in `processes`, `cleanups`, `freed`.
/// v2: every new cleanup since the previous report in `events`, nothing summed.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Report {
    pub v: u32,
    pub install_id: String,
    pub app_version: String,
    pub windows_build: String,
    /// v2: when the previous accepted report was sent (UTC), absent on the first one
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub events: Vec<Event>,
    #[serde(default)]
    pub processes: Option<u64>,
    #[serde(default)]
    pub cleanups: Option<u64>,
    #[serde(default)]
    pub freed: Vec<Freed>,
    pub machine: Machine,
    pub top_cpu: Vec<AppUse>,
    pub top_mem: Vec<AppUse>,
    pub tweaks: Vec<String>,
    /// vendor software on the PC (src-tauri/src/discovery.rs), at most once a day
    #[serde(default)]
    pub seen: Vec<Seen>,
}

/// One service, startup entry, scheduled task or program whose own name or vendor folder matches a
/// keyword in discovery.json. Names only: anything that looks like a path is refused.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Seen {
    pub kind: String,
    pub name: String,
    pub display: String,
    pub exe: String,
    pub folder: String,
    pub keyword: String,
    pub running: bool,
    pub startup: bool,
    pub disabled: bool,
    pub cpu: f32,
    pub mem: u64,
}

pub const SEEN_KINDS: &[&str] = &["service", "task", "run", "folder", "process"];

#[derive(Deserialize)]
struct KeywordList {
    keywords: Vec<String>,
}

/// The vendor keywords, the same file the app reads.
pub fn keywords() -> &'static [String] {
    static K: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    K.get_or_init(|| {
        let l: KeywordList = serde_json::from_str(include_str!("../discovery.json")).expect("discovery.json");
        l.keywords.into_iter().map(|k| k.to_lowercase()).collect()
    })
}

/// Words of `s`, split on symbols and camelCase, lower-cased (same rule as the app).
pub fn words(s: &str) -> Vec<String> {
    let mut out = vec![];
    for part in s.split(|c: char| !c.is_alphanumeric()).filter(|p| !p.is_empty()) {
        let cs: Vec<char> = part.chars().collect();
        let mut cur = String::new();
        for i in 0..cs.len() {
            let c = cs[i];
            let boundary = i > 0
                && ((c.is_uppercase() && cs[i - 1].is_lowercase())
                    || (c.is_uppercase() && cs[i - 1].is_uppercase() && cs.get(i + 1).is_some_and(|n| n.is_lowercase()))
                    || (c.is_ascii_digit() != cs[i - 1].is_ascii_digit()));
            if boundary && !cur.is_empty() {
                out.push(std::mem::take(&mut cur).to_lowercase());
            }
            cur.push(c);
        }
        if !cur.is_empty() {
            out.push(cur.to_lowercase());
        }
    }
    out
}

/// Does `keyword` (from the list) really occur in these texts? Short keywords must be a whole word.
pub fn keyword_matches(keyword: &str, texts: &[&str]) -> bool {
    keywords().iter().any(|k| k == keyword)
        && texts.iter().flat_map(|t| words(t)).any(|w| w == keyword || (keyword.len() > 4 && w.starts_with(keyword)))
}

impl Seen {
    fn ok(&self) -> bool {
        let text_ok = |s: &str, empty_ok: bool| {
            (empty_ok || !s.is_empty()) && s.chars().count() <= 120 && !s.chars().any(|c| c.is_control() || c == '\\' || c == '/')
        };
        SEEN_KINDS.contains(&self.kind.as_str())
            && text_ok(&self.name, false)
            && text_ok(&self.display, true)
            && text_ok(&self.exe, true)
            && text_ok(&self.folder, true)
            && keyword_matches(&self.keyword, &[&self.name, &self.display, &self.exe, &self.folder])
            && (0.0..=100_000.0).contains(&self.cpu)
            && self.mem <= 16 * TB
    }
}

pub const SOURCES: &[&str] = &["gui", "auto", "watchdog", "cli", "import"];

/// "2026-11-17T14:05Z" (or with seconds), not in the future by more than a day.
pub fn minute_ok(s: &str) -> bool {
    let t = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%MZ").or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ"));
    match t {
        Ok(t) => {
            let t = t.and_utc();
            t.timestamp() > 1_577_836_800 && t < chrono::Utc::now() + chrono::Duration::days(1) // after 2020
        }
        Err(_) => false,
    }
}

/// The report of the RigSweep Tabs extension (chrome-extension/stats.js): counts since its last report.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ExtReport {
    pub v: u32,
    pub install_id: String,
    pub ext_version: String,
    /// when the previous accepted report was sent (UTC), absent on the first one
    #[serde(default)]
    pub since: Option<String>,
    pub browser: String,
    pub browser_major: u32,
    /// counts since the last report
    pub tabs_slept: u64,
    pub dupes_closed: u64,
    pub matches_closed: u64,
    /// open right now
    pub tabs_open: u32,
    pub windows_open: u32,
    pub auto_sleep: bool,
}

pub const BROWSERS: &[&str] = &["Chrome", "Edge", "Brave", "Opera", "Vivaldi", "Chromium", "Other"];

impl ExtReport {
    pub fn validate(&self) -> Result<(), String> {
        let bad = |what: &str| Err(format!("invalid {what}"));
        if self.v != 1 {
            return bad("version");
        }
        if !install_id_ok(&self.install_id) {
            return bad("install_id");
        }
        if self.since.as_deref().is_some_and(|s| !minute_ok(s)) {
            return bad("since");
        }
        if !chars_ok(&self.ext_version, 24, |c| c.is_ascii_digit() || c == '.') {
            return bad("ext_version");
        }
        if !BROWSERS.contains(&self.browser.as_str()) || self.browser_major > 1000 {
            return bad("browser");
        }
        if self.tabs_slept > 1_000_000 || self.dupes_closed > 1_000_000 || self.matches_closed > 1_000_000 {
            return bad("counts");
        }
        if self.tabs_open > 100_000 || self.windows_open > 10_000 {
            return bad("open");
        }
        Ok(())
    }
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
        match self.v {
            1 if self.events.is_empty() && self.since.is_none() && self.processes.is_some() && self.cleanups.is_some() => {}
            2 if self.freed.is_empty() && self.processes.is_none() && self.cleanups.is_none() => {}
            _ => return bad("version"),
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
        if self.processes.unwrap_or(0) > 1_000_000 || self.cleanups.unwrap_or(0) > 100_000 {
            return bad("counts");
        }
        if self.since.as_deref().is_some_and(|s| !minute_ok(s)) {
            return bad("since");
        }
        if self.events.len() > 5000
            || self.events.iter().any(|e| {
                e.seq > i64::MAX as u64
                    || !minute_ok(&e.at)
                    || !SOURCES.contains(&e.source.as_str())
                    || !matches!(e.area.as_str(), "memory" | "disk" | "browser")
                    || !kind_ok(&e.kind)
                    || e.bytes > 64 * TB
                    || e.count > 1_000_000
                    || !(0.0..=100_000.0).contains(&e.cpu)
            })
        {
            return bad("events");
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
        if self.seen.len() > 300 || self.seen.iter().any(|s| !s.ok()) {
            return bad("seen");
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

    pub fn sample_v2() -> Report {
        serde_json::from_value(serde_json::json!({
            "v": 2, "install_id": "0123456789abcdef0123456789abcdef", "app_version": "0.8.0", "windows_build": "26300",
            "since": "2026-11-17T13:00Z",
            "events": [{"seq": 7, "at": "2026-11-17T13:42Z", "run": 3, "source": "gui", "area": "disk", "kind": "dir:target", "bytes": 1000, "count": 1},
                       {"seq": 8, "at": "2026-11-17T13:42Z", "run": 3, "source": "gui", "area": "memory", "kind": "mcp", "bytes": 50, "count": 2}],
            "machine": {"ram_total": 34359738368u64, "ram_used_avg": 17179869184u64, "cores": 16},
            "top_cpu": [], "top_mem": [], "tweaks": []
        }))
        .unwrap()
    }

    #[test]
    fn accepts_a_real_report() {
        assert_eq!(sample().validate(), Ok(()));
        assert_eq!(sample_v2().validate(), Ok(()));
    }

    #[test]
    fn seen_rows_must_really_name_a_vendor() {
        let row = |name: &str, folder: &str, keyword: &str| Seen {
            kind: "process".into(), name: name.into(), display: String::new(), exe: name.into(), folder: folder.into(),
            keyword: keyword.into(), running: true, startup: false, disabled: false, cpu: 1.0, mem: 10,
        };
        let mut r = sample_v2();
        r.seen = vec![row("ArmouryCrate.Service.exe", "ASUS", "armoury"), row("helper.exe", "Logitech", "logitech")];
        assert_eq!(r.validate(), Ok(()));
        for bad in [
            row("secret-client-tool.exe", "", "asus"),        // keyword claimed but not in the text
            row("php-cgi.exe", "", "hp"),                      // hp is not a whole word there
            row(r"C:\Users\me\asus\x.exe", "", "asus"),    // a path
            row("AsusTool.exe", "", "notakeyword"),            // not on the list
        ] {
            let mut r = sample_v2();
            r.seen = vec![bad.clone()];
            assert!(r.validate().is_err(), "{:?} should be refused", bad.name);
        }
    }

    #[test]
    fn v2_events_are_checked() {
        let mut r = sample_v2();
        r.events[0].at = "yesterday".into();
        assert!(r.validate().is_err());
        let mut r = sample_v2();
        r.events[0].source = "my-script".into();
        assert!(r.validate().is_err());
        let mut r = sample_v2();
        r.events[0].kind = r"C:\work\client-x".into();
        assert!(r.validate().is_err());
        // v1 and v2 fields can't be mixed
        let mut r = sample_v2();
        r.processes = Some(1);
        assert!(r.validate().is_err());
        let mut r = sample();
        r.events = sample_v2().events;
        assert!(r.validate().is_err());
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
    fn extension_reports_hold_counts_only() {
        let ok = serde_json::json!({"v": 1, "install_id": "0123456789abcdef0123456789abcdef", "ext_version": "1.1.0",
            "browser": "Chrome", "browser_major": 131, "tabs_slept": 40, "dupes_closed": 3, "matches_closed": 2,
            "tabs_open": 57, "windows_open": 3, "auto_sleep": true});
        let r: ExtReport = serde_json::from_value(ok.clone()).unwrap();
        assert_eq!(r.validate(), Ok(()));
        let mut url = ok.clone();
        url["top_site"] = "github.com".into();
        assert!(serde_json::from_value::<ExtReport>(url).is_err());
        let mut b = ok;
        b["browser"] = "Chrome 131 on my-laptop".into();
        assert!(serde_json::from_value::<ExtReport>(b).unwrap().validate().is_err());
    }

    #[test]
    fn refuses_unknown_fields() {
        let mut v = serde_json::to_value(serde_json::json!({"hostname": "my-pc"})).unwrap();
        v["v"] = 1.into();
        assert!(serde_json::from_value::<Report>(v).is_err());
    }
}
