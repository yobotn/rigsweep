//! Live meter: samples total CPU and RAM every couple of seconds for as long as RigSweep runs
//! (window or tray), keeps the last few minutes, and pushes each sample to the UI. That is what
//! lets a cleanup show how much it recovered the moment it happens, not only in the Boost Report.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use sysinfo::System;
use tauri::{AppHandle, Emitter};

pub const INTERVAL: Duration = Duration::from_secs(2);
/// 5 minutes of history
const KEEP: usize = 150;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Sample {
    /// unix time in ms
    pub t: u64,
    /// total CPU %, all cores
    pub cpu: f32,
    pub mem_used: u64,
    pub mem_total: u64,
}

static HISTORY: Mutex<VecDeque<Sample>> = Mutex::new(VecDeque::new());

fn push(s: Sample) {
    if let Ok(mut h) = HISTORY.lock() {
        if h.len() == KEEP {
            h.pop_front();
        }
        h.push_back(s);
    }
}

pub fn history() -> Vec<Sample> {
    HISTORY.lock().map(|h| h.iter().copied().collect()).unwrap_or_default()
}

/// Its own tiny `System` (CPU + memory only), so it never waits on the analyzer lock and never
/// disturbs the analyzer's per-process CPU sampling.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let Ok(mut sys) = System::new() else { return };
        sys.refresh_cpu_usage();
        loop {
            std::thread::sleep(INTERVAL);
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            let s = Sample {
                t: SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0),
                cpu: sys.global_cpu_usage(),
                mem_used: sys.used_memory(),
                mem_total: sys.total_memory(),
            };
            push(s);
            let _ = app.emit("live", s);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_is_bounded() {
        for i in 0..(KEEP as u64 + 10) {
            push(Sample { t: i, cpu: 0.0, mem_used: i, mem_total: 1 });
        }
        let h = history();
        assert_eq!(h.len(), KEEP);
        assert_eq!(h.last().unwrap().t, KEEP as u64 + 9);
    }
}
