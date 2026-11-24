//! Impact: what RigSweep has freed on this PC, by type. One record per thing cleaned, holding only
//! a category and numbers: never a path, project, process or user name. The Dashboard sums these,
//! and the anonymous weekly report (when sharing ships) is built from the same rows.

use serde::{Deserialize, Serialize};

/// One thing cleaned. `area` is "memory" | "disk" | "browser"; `kind` is a fixed category such as
/// "mcp", "dir:node_modules", "cache:npm" or "browser:cache".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Freed {
    pub area: String,
    pub kind: String,
    pub bytes: u64,
    /// processes ended (memory) or folders / caches emptied (disk, browser)
    pub count: u64,
    /// memory rows: the share of the whole CPU those processes were using when ended (%)
    #[serde(default)]
    pub cpu: f32,
    /// memory rows: CPU time those processes had used while they ran (seconds)
    #[serde(default)]
    pub cpu_secs: u64,
    /// memory rows: how long they had been running, added up (seconds)
    #[serde(default)]
    pub run_secs: u64,
}

impl Freed {
    pub fn new(area: &str, kind: impl Into<String>, bytes: u64, count: u64) -> Freed {
        Freed { area: area.into(), kind: kind.into(), bytes, count, cpu: 0.0, cpu_secs: 0, run_secs: 0 }
    }

    pub fn with_cpu(mut self, cpu: f32) -> Freed {
        self.cpu = (cpu.max(0.0) * 10.0).round() / 10.0;
        self
    }

    pub fn with_time(mut self, cpu_secs: u64, run_secs: u64) -> Freed {
        self.cpu_secs = cpu_secs;
        self.run_secs = run_secs;
        self
    }
}

/// The category of an ended dev process tree.
pub fn mem_kind(kind: &str, eco: &str, reasons: &[&str]) -> &'static str {
    if kind == "vm" {
        "vm"
    } else if kind == "agent" {
        "agent"
    } else if reasons.contains(&"mcp") {
        "mcp"
    } else if eco == "browser" {
        "browser"
    } else if eco == "wsl" {
        "wsl"
    } else if eco == "shell" {
        "shell"
    } else if reasons.contains(&"daemon") {
        "daemon"
    } else {
        "server"
    }
}

/// Folder names that are reported as themselves; anything else is "dir:other".
const DIR_KINDS: &[&str] = &[
    "node_modules", ".next", ".nuxt", ".svelte-kit", ".turbo", ".parcel-cache", ".angular", ".docusaurus", ".expo",
    "target", "build", ".gradle", "obj", "bin", ".venv", ".pytest_cache", ".mypy_cache", ".ruff_cache", ".tox",
    "__pycache__",
];

/// The category of a project folder, from its name (a leftover "target.rigsweep-del-123" counts as "target").
pub fn dir_kind(name: &str, leftover_mark: &str) -> String {
    let n = name.to_lowercase();
    let n = n.split(leftover_mark).next().unwrap_or("");
    let n = match n {
        "venv" | "env" | ".env" => ".venv",
        x => x,
    };
    if DIR_KINDS.contains(&n) { format!("dir:{n}") } else { "dir:other".into() }
}

/// Totals for one period.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Totals {
    pub disk: u64,
    pub memory: u64,
    pub browser: u64,
    /// disk freed outside RigSweep after it asked the user's AI (handoff.rs); not in `disk`, not a cleanup
    pub ai: u64,
    pub processes: u64,
    pub cleanups: u64,
    /// CPU the ended processes were using when ended, added up over every cleanup (% of the whole CPU)
    pub cpu: f32,
    /// the most CPU one cleanup released (%)
    pub cpu_peak: f32,
    /// CPU time the ended processes had used while they ran (seconds)
    pub cpu_secs: u64,
    /// how long the ended processes had been running, added up (seconds)
    pub run_secs: u64,
    /// per (area, kind), biggest first
    pub kinds: Vec<Freed>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Summary {
    pub week: Totals,
    pub month: Totals,
    pub all: Totals,
    /// time of the first record, "" when there is none
    pub since: String,
}

/// Sum rows of (run, area, kind, bytes, count) into totals.
pub fn total(rows: &[(i64, Freed)]) -> Totals {
    let mut t = Totals::default();
    let mut runs: Vec<i64> = vec![];
    let mut run_cpu: Vec<(i64, f32)> = vec![];
    for (run, f) in rows {
        if f.area != "ai" && !runs.contains(run) {
            runs.push(*run);
        }
        match f.area.as_str() {
            "memory" => {
                t.memory += f.bytes;
                t.processes += f.count;
                t.cpu += f.cpu;
                t.cpu_secs += f.cpu_secs;
                t.run_secs += f.run_secs;
                match run_cpu.iter_mut().find(|(r, _)| r == run) {
                    Some((_, c)) => *c += f.cpu,
                    None => run_cpu.push((*run, f.cpu)),
                }
            }
            "disk" => t.disk += f.bytes,
            "browser" => t.browser += f.bytes,
            "ai" => t.ai += f.bytes,
            _ => {}
        }
        match t.kinds.iter_mut().find(|k| k.area == f.area && k.kind == f.kind) {
            Some(k) => {
                k.bytes += f.bytes;
                k.count += f.count;
                k.cpu += f.cpu;
                k.cpu_secs += f.cpu_secs;
                k.run_secs += f.run_secs;
            }
            None => t.kinds.push(f.clone()),
        }
    }
    t.cleanups = runs.len() as u64;
    t.cpu = (t.cpu * 10.0).round() / 10.0;
    t.cpu_peak = run_cpu.iter().map(|(_, c)| (c * 10.0).round() / 10.0).fold(0.0, f32::max);
    t.kinds.sort_by(|a, b| b.bytes.cmp(&a.bytes).then(b.count.cmp(&a.count)));
    t
}

/// What old History rows can still tell: "freed 412.3 MB, …" totals for disk and browser
/// cleanups, and one ended process per "ended …" dev-cleanup row. Types are unknown: "earlier".
pub fn from_history(action: &str, detail: &str) -> Option<Freed> {
    match action {
        "disk_clean" | "browser_clean" => {
            let mb: f64 = detail.strip_prefix("freed ")?.split(" MB").next()?.trim().parse().ok()?;
            let area = if action == "disk_clean" { "disk" } else { "browser" };
            Some(Freed::new(area, "earlier", (mb * 1048576.0) as u64, 0))
        }
        "dev_cleanup" if detail.starts_with("ended ") => Some(Freed::new("memory", "earlier", 0, 1)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_carry_no_names() {
        assert_eq!(dir_kind("node_modules", ".rigsweep-del-"), "dir:node_modules");
        assert_eq!(dir_kind("Target.rigsweep-del-1700000000", ".rigsweep-del-"), "dir:target");
        assert_eq!(dir_kind("venv", ".rigsweep-del-"), "dir:.venv");
        assert_eq!(dir_kind("my-secret-project", ".rigsweep-del-"), "dir:other");
        assert_eq!(mem_kind("runtime", "node", &["mcp", "orphan"]), "mcp");
        assert_eq!(mem_kind("agent", "agent", &[]), "agent");
        assert_eq!(mem_kind("runtime", "java", &["idle", "daemon"]), "daemon");
        assert_eq!(mem_kind("runtime", "node", &["idle"]), "server");
    }

    #[test]
    fn totals_add_up_by_area_kind_and_run() {
        let rows = vec![
            (1, Freed::new("memory", "mcp", 100, 2)),
            (1, Freed::new("memory", "mcp", 50, 1)),
            (2, Freed::new("disk", "dir:target", 1000, 1)),
            (2, Freed::new("browser", "browser:cache", 10, 1)),
        ];
        let t = total(&rows);
        assert_eq!((t.memory, t.processes, t.disk, t.browser, t.cleanups), (150, 3, 1000, 10, 2));
        let cpu = total(&[
            (1, Freed::new("memory", "mcp", 1, 1).with_cpu(10.0).with_time(60, 3600)),
            (1, Freed::new("memory", "server", 1, 1).with_cpu(5.0).with_time(30, 600)),
            (2, Freed::new("memory", "mcp", 1, 1).with_cpu(12.0).with_time(10, 100)),
        ]);
        assert_eq!((cpu.cpu, cpu.cpu_peak, cpu.cpu_secs, cpu.run_secs), (27.0, 15.0, 100, 4300));
        assert_eq!(t.kinds[0], Freed::new("disk", "dir:target", 1000, 1));
        assert_eq!(t.kinds.iter().find(|k| k.kind == "mcp").unwrap().count, 3);
    }

    #[test]
    fn history_backfill_reads_only_totals() {
        assert_eq!(from_history("disk_clean", "freed 512.0 MB, 0 locked file(s) left"), Some(Freed::new("disk", "earlier", 536870912, 0)));
        assert_eq!(from_history("disk_clean", "cache npm (12.0 MB)"), None);
        assert_eq!(from_history("dev_cleanup", "ended vite (PID 12, 3 process(es))"), Some(Freed::new("memory", "earlier", 0, 1)));
        assert_eq!(from_history("kill", "ended x"), None);
    }
}
