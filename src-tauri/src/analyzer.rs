use serde::Serialize;
use std::collections::HashMap;
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

use crate::actions::service;
use crate::catalog::{is_protected_process, is_protected_service, Action, Catalog};
use crate::devclean::{self, DevReport, IdleTracker};

#[derive(Serialize, Clone, Debug)]
pub struct HostedService {
    pub name: String,
    pub display: String,
    /// catalog tweak that covers this service, if any
    pub tweak_id: Option<String>,
    pub protected: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub exe: String,
    pub cpu: f32,
    pub mem: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub services: Vec<HostedService>,
    pub protected: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct Snapshot {
    pub cpu_total: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub process_count: usize,
    pub service_running: usize,
    pub interval_secs: f32,
    pub processes: Vec<ProcInfo>,
}

/// Point-in-time measurements used by the Boost Report (before / after comparison).
#[derive(Serialize, serde::Deserialize, Clone, Debug, Default)]
pub struct Metrics {
    pub mem_used: u64,
    pub mem_total: u64,
    /// average total CPU over a ~1.5 s sample
    pub cpu_avg: f32,
    pub process_count: usize,
    pub service_running: usize,
    pub chrome_mem: u64,
    pub chrome_procs: usize,
    pub dev_mem: u64,
    pub dev_procs: usize,
    pub dev_stale_mem: u64,
}

pub struct Analyzer {
    sys: System,
    last: Instant,
    idle: IdleTracker,
    pub last_dev: DevReport,
}

impl Analyzer {
    pub fn new() -> Result<Analyzer, String> {
        let mut sys = System::new().map_err(|e| format!("sysinfo init failed: {e:?}"))?;
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Ok(Analyzer { sys, last: Instant::now(), idle: IdleTracker::default(), last_dev: DevReport::default() })
    }

    pub fn snapshot(&mut self, catalog: &Catalog) -> Snapshot {
        // CPU% and disk deltas need two samples at least ~200ms apart
        let since = self.last.elapsed();
        if since.as_millis() < 300 {
            std::thread::sleep(std::time::Duration::from_millis(300 - since.as_millis() as u64));
        }
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let interval = self.last.elapsed().as_secs_f32().max(0.001);
        self.last = Instant::now();

        let ncpu = self.sys.cpus().len().max(1) as f32;

        let tweak_for: HashMap<String, String> = catalog
            .tweaks
            .iter()
            .flat_map(|t| {
                t.actions.iter().filter_map(move |a| match a {
                    Action::Service { name, .. } => Some((name.to_ascii_lowercase(), t.id.clone())),
                    _ => None,
                })
            })
            .collect();

        let services = service::list_services().unwrap_or_default();
        let service_running = services.iter().filter(|s| s.running).count();
        let mut by_pid: HashMap<u32, Vec<HostedService>> = HashMap::new();
        for s in services.into_iter().filter(|s| s.running && s.pid != 0) {
            by_pid.entry(s.pid).or_default().push(HostedService {
                tweak_id: tweak_for.get(&s.name.to_ascii_lowercase()).cloned(),
                protected: is_protected_service(&s.name),
                name: s.name,
                display: s.display,
            });
        }

        let mut processes: Vec<ProcInfo> = self
            .sys
            .processes()
            .iter()
            .map(|(pid, p)| {
                let name = p.name().to_string_lossy().into_owned();
                let du = p.disk_usage();
                let pid_u = pid.as_u32();
                ProcInfo {
                    pid: pid_u,
                    protected: pid_u <= 4 || is_protected_process(&name) || pid_u == std::process::id(),
                    exe: p.exe().map(|e| e.display().to_string()).unwrap_or_default(),
                    cpu: p.cpu_usage() / ncpu,
                    mem: p.memory(),
                    disk_read: (du.read_bytes as f32 / interval) as u64,
                    disk_write: (du.written_bytes as f32 / interval) as u64,
                    services: by_pid.remove(&pid_u).unwrap_or_default(),
                    name,
                }
            })
            .collect();
        processes.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal));

        Snapshot {
            cpu_total: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            process_count: processes.len(),
            service_running,
            interval_secs: interval,
            processes,
        }
    }

    /// Refresh everything dev-cleanup needs (command lines + working dirs only for dev processes).
    pub fn dev_scan(&mut self, idle_minutes: u32) -> DevReport {
        let since = self.last.elapsed();
        if since.as_millis() < 300 {
            std::thread::sleep(std::time::Duration::from_millis(300 - since.as_millis() as u64));
        }
        self.sys.refresh_cpu_usage();
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        self.last = Instant::now();
        let pids: Vec<Pid> = self
            .sys
            .processes()
            .iter()
            .filter(|(_, p)| {
                let n = p.name().to_string_lossy().to_ascii_lowercase();
                devclean::SCAN_NAMES.contains(&n.as_str()) || devclean::WRAPPER_NAMES.contains(&n.as_str())
            })
            .map(|(pid, _)| *pid)
            .collect();
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            false,
            ProcessRefreshKind::nothing()
                .with_cmd(UpdateKind::OnlyIfNotSet)
                .with_cwd(UpdateKind::OnlyIfNotSet)
                .with_exe(UpdateKind::OnlyIfNotSet),
        );
        let r = devclean::scan(&self.sys, &mut self.idle, idle_minutes);
        self.last_dev = r.clone();
        r
    }

    /// Measure the machine for the Boost Report. Takes ~1.5 s (CPU needs a sampling window).
    pub fn metrics(&mut self, idle_minutes: u32) -> Metrics {
        self.sys.refresh_cpu_usage();
        let mut samples = vec![];
        for _ in 0..3 {
            std::thread::sleep(std::time::Duration::from_millis(500));
            self.sys.refresh_cpu_usage();
            samples.push(self.sys.global_cpu_usage());
        }
        self.sys.refresh_memory();
        let dev = self.dev_scan(idle_minutes);
        let chrome: Vec<_> = self
            .sys
            .processes()
            .values()
            .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case("chrome.exe"))
            .collect();
        let service_running = service::list_services().map(|v| v.iter().filter(|s| s.running).count()).unwrap_or(0);
        Metrics {
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            cpu_avg: samples.iter().sum::<f32>() / samples.len().max(1) as f32,
            process_count: self.sys.processes().len(),
            service_running,
            chrome_mem: chrome.iter().map(|p| p.memory()).sum(),
            chrome_procs: chrome.len(),
            dev_mem: dev.total_mem,
            dev_procs: dev.items.iter().map(|i| i.tree_count).sum(),
            dev_stale_mem: dev.stale_mem,
        }
    }

    pub fn kill(&mut self, pid: u32) -> Result<String, String> {
        self.sys.refresh_processes(ProcessesToUpdate::Some(&[Pid::from_u32(pid)]), true);
        let p = self.sys.process(Pid::from_u32(pid)).ok_or("process no longer exists")?;
        let name = p.name().to_string_lossy().into_owned();
        if pid <= 4 || pid == std::process::id() || is_protected_process(&name) {
            return Err(format!("{name} is a protected system process"));
        }
        if p.kill() {
            Ok(name)
        } else {
            Err(format!("couldn't end {name} (access denied or already exiting)"))
        }
    }
}
