//! Dev Cleanup: find stale Node / AI-agent CLI processes left behind by closed terminals and
//! old sessions, and end them (whole process tree) on request. Nothing is ever killed
//! automatically; the UI pre-selects only what is clearly stale.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use sysinfo::{Pid, Process, System};

use crate::util;

/// JS / build runtimes that pile up across dev sessions.
const RUNTIMES: &[&str] = &["node.exe", "bun.exe", "deno.exe", "esbuild.exe"];
/// Native AI coding-agent CLIs.
const AGENT_EXES: &[&str] = &["claude.exe", "codex.exe", "opencode.exe", "gemini.exe"];
/// Markers in a node command line that mean "this is an agent CLI".
const AGENT_MARKERS: &[&str] = &["@anthropic-ai/claude-code", "@openai/codex", "@google/gemini-cli", "opencode-ai"];
/// GUI apps whose node children (language servers, MCP servers, extension hosts) must not be touched.
const OWNER_APPS: &[&str] = &[
    "code.exe", "code - insiders.exe", "cursor.exe", "windsurf.exe", "zed.exe", "devenv.exe",
    "idea64.exe", "webstorm64.exe", "rider64.exe", "pycharm64.exe", "goland64.exe", "phpstorm64.exe",
    "clion64.exe", "rustrover64.exe", "studio64.exe", "sublime_text.exe",
];

/// A tree with no CPU activity for this long (while RigSweep watched it) counts as idle.
const IDLE_OBSERVED_SECS: u64 = 20 * 60;

#[derive(Serialize, Clone, Debug)]
pub struct DevProc {
    pub pid: u32,
    /// process start time (unix secs); sent back on kill so a reused PID is never hit
    pub start_time: u64,
    pub name: String,
    /// "agent" | "runtime"
    pub kind: &'static str,
    /// friendly label, e.g. "astro", "vite", "claude"
    pub label: String,
    /// project folder guessed from cwd / command line
    pub project: String,
    pub cmd: String,
    pub parent_pid: u32,
    pub parent_name: String,
    pub age_secs: u64,
    pub cpu: f32,
    /// own memory + all descendants
    pub tree_mem: u64,
    pub tree_count: usize,
    pub ports: Vec<u16>,
    /// reason codes for the UI: orphan | idle | serving | owned
    pub reasons: Vec<&'static str>,
    pub stale: bool,
    /// owned by an editor / desktop app (language server, MCP server…): never killable here
    pub protected: bool,
    pub owner: String,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct DevReport {
    pub items: Vec<DevProc>,
    pub total_mem: u64,
    pub stale_mem: u64,
    pub stale_count: usize,
}

#[derive(Deserialize, Clone, Debug)]
pub struct KillTarget {
    pub pid: u32,
    pub start_time: u64,
    /// what the user saw when they ticked the row; re-verified against a fresh scan
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub ports: Vec<u16>,
}

/// Does the freshly scanned row still describe the same thing the user confirmed?
/// Kills are by PID tree (never by port), so this guards against changes between scan and click.
pub fn verify_target(item: &DevProc, t: &KillTarget) -> Result<(), String> {
    if item.pid != t.pid || item.start_time != t.start_time {
        return Err("no longer running (or PID was reused)".into());
    }
    if (!t.label.is_empty() && item.label != t.label) || (!t.project.is_empty() && item.project != t.project) {
        return Err(format!("now looks like {} in {}; rescan before ending it", item.label, item.project));
    }
    if !t.ports.is_empty() && !item.ports.is_empty() && !t.ports.iter().any(|p| item.ports.contains(p)) {
        return Err(format!("ports changed since scan ({:?} -> {:?}); rescan before ending it", t.ports, item.ports));
    }
    Ok(())
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct KillResult {
    pub ended: Vec<String>,
    pub failed: Vec<String>,
    pub freed_mem: u64,
}

/// Remembers when each process tree last used CPU, so "idle" is measured, not guessed.
#[derive(Default)]
pub struct IdleTracker {
    seen: HashMap<u32, (u64, u64, Instant)>, // pid -> (start_time, tree cpu ms, last change)
}

impl IdleTracker {
    fn idle_secs(&mut self, pid: u32, start: u64, cpu_ms: u64) -> Option<u64> {
        let now = Instant::now();
        let e = self.seen.entry(pid).or_insert((start, cpu_ms, now));
        if e.0 != start {
            *e = (start, cpu_ms, now);
            return None;
        }
        // tolerate tiny housekeeping ticks (timers, GC)
        if cpu_ms > e.1 + 250 {
            e.1 = cpu_ms;
            e.2 = now;
        }
        Some(e.2.elapsed().as_secs())
    }

    fn retain(&mut self, alive: &HashSet<u32>) {
        self.seen.retain(|pid, _| alive.contains(pid));
    }
}

fn lower_name(p: &Process) -> String {
    p.name().to_string_lossy().to_ascii_lowercase()
}

fn cmdline(p: &Process) -> String {
    p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ")
}

fn is_desktop_app(p: &Process) -> bool {
    p.exe().map(|e| e.to_string_lossy().to_ascii_lowercase().contains("\\windowsapps\\")).unwrap_or(false)
}

/// Classify a process: Some("agent") / Some("runtime") / None (not a dev process).
fn dev_kind(p: &Process) -> Option<&'static str> {
    let n = lower_name(p);
    if AGENT_EXES.contains(&n.as_str()) {
        // the Claude / ChatGPT desktop apps also ship a claude.exe; only CLIs count
        return if is_desktop_app(p) { None } else { Some("agent") };
    }
    if RUNTIMES.contains(&n.as_str()) {
        let c = cmdline(p).to_ascii_lowercase().replace('\\', "/");
        return Some(if AGENT_MARKERS.iter().any(|m| c.contains(m)) { "agent" } else { "runtime" });
    }
    None
}

/// "C:\x\proj\node_modules\.bin\..\astro\bin\astro.mjs dev" -> ("astro", "proj")
pub fn describe(name: &str, cmd: &str, cwd: &str) -> (String, String) {
    let c = cmd.replace('\\', "/");
    let base = |s: &str| s.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
    let mut label = name.trim_end_matches(".exe").to_string();
    let mut project = String::new();
    let lc = c.to_ascii_lowercase();
    if let Some(i) = lc.rfind("node_modules/") {
        let after = &c[i + "node_modules/".len()..];
        let pkg: Vec<&str> = after.split('/').filter(|s| !s.is_empty() && *s != ".bin" && *s != "..").collect();
        if let Some(first) = pkg.first() {
            label = if first.starts_with('@') && pkg.len() > 1 { format!("{}/{}", first, pkg[1]) } else { first.to_string() };
            label = label.trim_matches('"').to_string();
        }
        if let Some(j) = lc.find("node_modules/") {
            project = base(c[..j].trim_start_matches('"'));
        }
    } else if let Some(script) = c.split_whitespace().skip(1).map(|s| s.trim_matches('"')).find(|s| {
        let s = s.to_ascii_lowercase();
        s.ends_with(".js") || s.ends_with(".mjs") || s.ends_with(".cjs") || s.ends_with(".ts")
    }) {
        label = base(script);
    }
    if !cwd.is_empty() {
        project = base(&cwd.replace('\\', "/"));
    }
    (label, project)
}

/// Listening TCP ports per PID (IPv4 + IPv6), via GetExtendedTcpTable (locale independent).
pub fn listening_ports() -> HashMap<u32, Vec<u16>> {
    use windows::Win32::NetworkManagement::IpHelper::{GetExtendedTcpTable, TCP_TABLE_OWNER_PID_LISTENER};
    let mut out: HashMap<u32, Vec<u16>> = HashMap::new();
    // (address family, row size, offset of local port, offset of owning pid)
    for (af, row, port_off, pid_off) in [(2u32, 24usize, 8usize, 20usize), (23u32, 56usize, 20usize, 52usize)] {
        let mut size = 0u32;
        unsafe { GetExtendedTcpTable(None, &mut size, false, af, TCP_TABLE_OWNER_PID_LISTENER, 0) };
        if size == 0 {
            continue;
        }
        let mut buf = vec![0u8; size as usize + 1024];
        size = buf.len() as u32;
        let rc = unsafe {
            GetExtendedTcpTable(Some(buf.as_mut_ptr() as *mut _), &mut size, false, af, TCP_TABLE_OWNER_PID_LISTENER, 0)
        };
        if rc != 0 {
            continue;
        }
        let rd = |o: usize| u32::from_ne_bytes([buf[o], buf[o + 1], buf[o + 2], buf[o + 3]]);
        let n = rd(0) as usize;
        for i in 0..n {
            let base = 4 + i * row;
            if base + row > buf.len() {
                break;
            }
            let raw = rd(base + port_off);
            let port = u16::from_be((raw & 0xFFFF) as u16);
            let pid = rd(base + pid_off);
            let v = out.entry(pid).or_default();
            if !v.contains(&port) {
                v.push(port);
            }
        }
    }
    for v in out.values_mut() {
        v.sort_unstable();
    }
    out
}

pub fn scan(sys: &System, tracker: &mut IdleTracker, idle_hours: u32) -> DevReport {
    let procs = sys.processes();
    let ncpu = sys.cpus().len().max(1) as f32;
    let ports = listening_ports();
    let me = std::process::id();

    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, p) in procs {
        if let Some(pp) = p.parent() {
            children.entry(pp.as_u32()).or_default().push(pid.as_u32());
        }
    }
    let get = |pid: u32| procs.get(&Pid::from_u32(pid));
    // a parent only counts if it started before the child (otherwise the PID was reused)
    let live_parent = |p: &Process| -> Option<u32> {
        let pp = p.parent()?.as_u32();
        let parent = get(pp)?;
        (parent.start_time() <= p.start_time()).then_some(pp)
    };

    let dev: HashMap<u32, &'static str> =
        procs.iter().filter_map(|(pid, p)| dev_kind(p).map(|k| (pid.as_u32(), k))).collect();

    let mut items = vec![];
    let mut alive = HashSet::new();
    for (&pid, &kind) in &dev {
        let Some(p) = get(pid) else { continue };
        // only list tree roots: a node spawned (even via cmd.exe) by another dev process is
        // shown inside that ancestor's tree, never twice
        let parent = live_parent(p);
        let mut up = parent;
        let mut nested = false;
        for _ in 0..16 {
            let Some(a) = up else { break };
            if dev.contains_key(&a) {
                nested = true;
                break;
            }
            up = get(a).and_then(|ap| live_parent(ap));
        }
        if nested {
            continue;
        }
        alive.insert(pid);

        // walk the whole tree (any process type) for memory / cpu / ports
        let mut stack = vec![pid];
        let mut seen = HashSet::new();
        let (mut mem, mut cpu, mut cpu_ms, mut tree_ports) = (0u64, 0f32, 0u64, vec![]);
        while let Some(x) = stack.pop() {
            if !seen.insert(x) {
                continue;
            }
            if let Some(q) = get(x) {
                if x != pid && q.start_time() < p.start_time() {
                    continue; // reused PID, not really our child
                }
                mem += q.memory();
                cpu += q.cpu_usage() / ncpu;
                cpu_ms += q.accumulated_cpu_time();
                if let Some(pp) = ports.get(&x) {
                    tree_ports.extend(pp.iter().copied());
                }
            }
            if let Some(c) = children.get(&x) {
                stack.extend(c.iter().copied());
            }
        }
        tree_ports.sort_unstable();
        tree_ports.dedup();

        // who owns it? walk ancestors looking for an editor / desktop app
        let mut owner = String::new();
        let mut cur = parent;
        let mut hops = 0;
        while let (Some(a), true) = (cur, hops < 12) {
            let Some(ap) = get(a) else { break };
            let an = lower_name(ap);
            if OWNER_APPS.contains(&an.as_str()) || (is_desktop_app(ap) && an != "windowsterminal.exe") {
                owner = ap.name().to_string_lossy().into_owned();
                break;
            }
            cur = live_parent(ap);
            hops += 1;
        }

        let name = p.name().to_string_lossy().into_owned();
        let cmd = cmdline(p);
        let cwd = p.cwd().map(|c| c.display().to_string()).unwrap_or_default();
        let (label, project) = describe(&name, &cmd, &cwd);
        let age = p.run_time();
        let observed_idle = tracker.idle_secs(pid, p.start_time(), cpu_ms);
        let orphan = parent.is_none();
        let idle = observed_idle.map(|s| s >= IDLE_OBSERVED_SECS).unwrap_or(false)
            || (age >= idle_hours as u64 * 3600 && cpu < 0.2);
        let protected = !owner.is_empty() || pid == me;

        let mut reasons = vec![];
        if orphan {
            reasons.push("orphan");
        }
        if idle {
            reasons.push("idle");
        }
        if !tree_ports.is_empty() {
            reasons.push("serving");
        }
        if protected {
            reasons.push("owned");
        }
        // An orphan is stale even while holding a port: that's the classic zombie dev server.
        // An idle agent CLI in an open terminal is just waiting for you, so it's never pre-selected.
        let stale = !protected && (orphan || (kind == "runtime" && idle && tree_ports.is_empty()));

        items.push(DevProc {
            pid,
            start_time: p.start_time(),
            name,
            kind,
            label,
            project,
            cmd: cmd.chars().take(400).collect(),
            parent_pid: parent.unwrap_or(0),
            parent_name: parent.and_then(get).map(|q| q.name().to_string_lossy().into_owned()).unwrap_or_default(),
            age_secs: age,
            cpu,
            tree_mem: mem,
            tree_count: seen.len(),
            ports: tree_ports,
            reasons,
            stale,
            protected,
            owner,
        });
    }
    tracker.retain(&alive);
    items.sort_by(|a, b| b.stale.cmp(&a.stale).then(b.tree_mem.cmp(&a.tree_mem)));
    DevReport {
        total_mem: items.iter().map(|i| i.tree_mem).sum(),
        stale_mem: items.iter().filter(|i| i.stale).map(|i| i.tree_mem).sum(),
        stale_count: items.iter().filter(|i| i.stale).count(),
        items,
    }
}

/// End each target's whole process tree (only that PID and its own descendants).
/// `fresh` must be a scan taken right before this call; every target is re-verified against it.
pub fn kill(fresh: &DevReport, targets: &[KillTarget]) -> KillResult {
    let mut r = KillResult::default();
    for t in targets {
        let Some(item) = fresh.items.iter().find(|i| i.pid == t.pid && i.start_time == t.start_time) else {
            r.failed.push(format!("PID {}: no longer running (or PID was reused)", t.pid));
            continue;
        };
        if let Err(e) = verify_target(item, t) {
            r.failed.push(format!("{} (PID {}): {e}", t.label, t.pid));
            continue;
        }
        if item.protected {
            r.failed.push(format!("{} (PID {}): owned by {}, skipped", item.label, item.pid, item.owner));
            continue;
        }
        let pid = item.pid.to_string();
        match util::run("taskkill.exe", &["/PID", &pid, "/T", "/F"]) {
            Ok(o) if o.ok => {
                r.freed_mem += item.tree_mem;
                r.ended.push(format!("{} (PID {}, {} process(es))", item.label, item.pid, item.tree_count));
            }
            Ok(o) => r.failed.push(format!("{} (PID {}): {}", item.label, item.pid, o.stderr.trim())),
            Err(e) => r.failed.push(format!("{} (PID {}): {e}", item.label, item.pid)),
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::{describe, verify_target, DevProc, KillTarget};

    fn row(pid: u32, project: &str, ports: Vec<u16>) -> DevProc {
        DevProc {
            pid, start_time: 100, name: "node.exe".into(), kind: "runtime", label: "astro".into(),
            project: project.into(), cmd: String::new(), parent_pid: 0, parent_name: String::new(), age_secs: 0,
            cpu: 0.0, tree_mem: 0, tree_count: 1, ports, reasons: vec![], stale: false, protected: false, owner: String::new(),
        }
    }

    #[test]
    fn two_astro_projects_are_never_confused() {
        let site_a = row(10, "site-a", vec![4321]);
        let site_b = row(20, "site-b", vec![4322]);
        let pick_a = KillTarget { pid: 10, start_time: 100, label: "astro".into(), project: "site-a".into(), ports: vec![4321] };
        assert!(verify_target(&site_a, &pick_a).is_ok());
        // same label, other project / PID: refused
        assert!(verify_target(&site_b, &pick_a).is_err());
        // PID reused by a process in another project: refused
        let reused = row(10, "site-b", vec![4322]);
        assert!(verify_target(&reused, &pick_a).is_err());
        // same tree but it now serves completely different ports: refused
        let moved = row(10, "site-a", vec![9999]);
        assert!(verify_target(&moved, &pick_a).is_err());
        // extra port opened (HMR websocket): still the same server, allowed
        let extra = row(10, "site-a", vec![4321, 24678]);
        assert!(verify_target(&extra, &pick_a).is_ok());
    }

    #[test]
    fn describes_node_commands() {
        let (l, p) = describe(
            "node.exe",
            r#""node" "C:\Users\me\site\node_modules\.bin\\..\astro\bin\astro.mjs" dev"#,
            "",
        );
        assert_eq!((l.as_str(), p.as_str()), ("astro", "site"));
        let (l, _) = describe("node.exe", r"node C:\x\node_modules\@anthropic-ai\claude-code\cli.js", "");
        assert_eq!(l, "@anthropic-ai/claude-code");
        let (l, p) = describe("node.exe", r"node server.js", r"C:\work\api");
        assert_eq!((l.as_str(), p.as_str()), ("server.js", "api"));
        let (l, _) = describe("claude.exe", r"C:\Users\me\.local\bin\claude.exe", "");
        assert_eq!(l, "claude");
    }
}

#[cfg(test)]
mod live_tests {
    /// Read-only: print what Dev Cleanup sees on this machine. Never kills anything.
    /// Run with: cargo test live_dev_scan -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_dev_scan() {
        let mut a = crate::analyzer::Analyzer::new().unwrap();
        let r = a.dev_scan(2);
        for i in &r.items {
            println!(
                "{:<6} {:<8} {:<28} {:<18} age {:>5.1}h  {:>6} MB x{:<3} ports {:?} {:?} stale={} owner={}",
                i.pid, i.kind, i.label, i.project, i.age_secs as f64 / 3600.0, i.tree_mem / 1_048_576, i.tree_count, i.ports, i.reasons, i.stale, i.owner
            );
        }
        println!("total {} MB, stale {} MB ({} trees)", r.total_mem / 1_048_576, r.stale_mem / 1_048_576, r.stale_count);
    }
}
