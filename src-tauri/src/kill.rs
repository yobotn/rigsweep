//! Ending processes without hitting a reused PID.
//!
//! Every process is opened first and its creation time checked on that handle. An open handle
//! keeps the PID from being handed to a new process, so what we checked is what we terminate.
//! (Killing by bare PID, like `taskkill /PID`, leaves a window where the PID can be reused.)

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, TerminateProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
};

/// Seconds between 1601-01-01 (FILETIME epoch) and 1970-01-01.
const EPOCH_DIFF: u64 = 11_644_473_600;

struct Owned(HANDLE);

impl Drop for Owned {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Working set of a process that can't be opened at all (vmmemWSL is a protected minimal process,
/// so sysinfo reads 0 for it). Read from the kernel's process list, like Task Manager does.
pub fn working_set(pid: u32) -> Option<u64> {
    use windows::Wdk::System::SystemInformation::{NtQuerySystemInformation, SystemProcessInformation};
    use windows::Win32::System::WindowsProgramming::SYSTEM_PROCESS_INFORMATION;
    let mut buf: Vec<u64> = vec![0; 1 << 17]; // 1 MiB, 8-byte aligned; grown if too small
    for _ in 0..4 {
        let mut need = 0u32;
        let bytes = (buf.len() * 8) as u32;
        let st = unsafe { NtQuerySystemInformation(SystemProcessInformation, buf.as_mut_ptr() as *mut _, bytes, &mut need) };
        if st.is_ok() {
            let base = buf.as_ptr() as *const u8;
            let mut off = 0usize;
            loop {
                if off + std::mem::size_of::<SYSTEM_PROCESS_INFORMATION>() > bytes as usize {
                    return None;
                }
                let e = unsafe { &*(base.add(off) as *const SYSTEM_PROCESS_INFORMATION) };
                if e.UniqueProcessId.0 as usize == pid as usize {
                    return Some(e.WorkingSetSize as u64);
                }
                if e.NextEntryOffset == 0 {
                    return None;
                }
                off += e.NextEntryOffset as usize;
            }
        }
        // STATUS_INFO_LENGTH_MISMATCH: the list grew; retry with what it asked for plus slack
        buf = vec![0; (need as usize + 65536) / 8 + 1];
    }
    None
}

/// FILETIME (100 ns ticks since 1601) -> unix seconds, the same rounding sysinfo uses for start_time.
pub fn filetime_to_unix(ft: FILETIME) -> u64 {
    let ticks = ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64;
    (ticks / 10_000_000).saturating_sub(EPOCH_DIFF)
}

/// Does a creation time read from the handle match the one recorded at scan time?
/// One second of slack: both sides round down, but from slightly different readings.
pub fn same_start(actual: u64, expected: u64) -> bool {
    expected != 0 && actual.abs_diff(expected) <= 1
}

/// Open `pid` and confirm it is still the process that started at `start_time`.
fn open_verified(pid: u32, start_time: u64) -> Result<Owned, String> {
    if start_time == 0 {
        return Err("start time unknown, can't rule out a reused PID".into());
    }
    let h = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .map_err(|e| format!("can't open it ({})", e.message()))?;
    let h = Owned(h);
    let (mut created, mut exited, mut kernel, mut user) = Default::default();
    unsafe { GetProcessTimes(h.0, &mut created, &mut exited, &mut kernel, &mut user) }
        .map_err(|e| format!("can't read its start time ({})", e.message()))?;
    if !same_start(filetime_to_unix(created), start_time) {
        return Err("no longer running (PID was reused)".into());
    }
    Ok(h)
}

/// End one process, but only if it is still the one that started at `start_time`.
pub fn terminate(pid: u32, start_time: u64) -> Result<(), String> {
    let h = open_verified(pid, start_time)?;
    unsafe { TerminateProcess(h.0, 1) }.map_err(|e| e.message().to_string())
}

/// End a process tree recorded by a scan: `members[0]` is the root, the rest its descendants,
/// each with its own start time. The root must verify; a descendant that already exited or whose
/// PID was reused is skipped. All handles are opened before anything is terminated, so no PID in
/// the set can be reused midway. The root goes first, so a supervisor can't respawn its workers.
/// Returns how many processes were ended.
pub fn terminate_tree(members: &[(u32, u64)]) -> Result<usize, String> {
    let Some(&(root_pid, root_start)) = members.first() else {
        return Err("empty process tree".into());
    };
    let root = open_verified(root_pid, root_start)?;
    let rest: Vec<Owned> = members[1..].iter().filter_map(|&(pid, start)| open_verified(pid, start).ok()).collect();
    unsafe { TerminateProcess(root.0, 1) }.map_err(|e| e.message().to_string())?;
    let mut n = 1;
    for h in &rest {
        // a child may exit on its own once the root is gone; that's not a failure
        if unsafe { TerminateProcess(h.0, 1) }.is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_working_set_from_the_process_list() {
        let ws = working_set(std::process::id()).expect("own process is in the list");
        assert!(ws > 1_000_000, "{ws}");
        assert_eq!(working_set(u32::MAX - 3), None);
    }

    #[test]
    fn converts_filetime_like_sysinfo() {
        // 2024-01-01T00:00:00Z = 1704067200 unix = 133485408000000000 FILETIME ticks
        let t: u64 = 133_485_408_000_000_000 + 9_999_999; // just under one more second
        let ft = FILETIME { dwLowDateTime: t as u32, dwHighDateTime: (t >> 32) as u32 };
        assert_eq!(filetime_to_unix(ft), 1_704_067_200);
    }

    #[test]
    fn start_time_match_has_one_second_slack_and_rejects_unknown() {
        assert!(same_start(100, 100));
        assert!(same_start(101, 100));
        assert!(!same_start(102, 100));
        assert!(!same_start(0, 0));
    }

    /// Spawn a real tree (cmd -> ping), then end it by handle.
    #[test]
    fn ends_a_real_tree_only_with_the_right_start_times() {
        use std::os::windows::process::CommandExt;
        use sysinfo::{Pid, ProcessesToUpdate, System};
        let snap = || {
            let mut s = System::new().unwrap();
            s.refresh_processes(ProcessesToUpdate::All, true);
            s
        };
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 30 127.0.0.1 >nul"])
            .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
            .spawn()
            .unwrap();
        let root = child.id();
        // cmd.exe needs a moment to start ping; on a busy machine that can take seconds, so poll
        let kids_of = |sys: &System| -> Vec<(u32, u64)> {
            sys.processes()
                .iter()
                .filter(|(_, p)| p.parent() == Some(Pid::from_u32(root)))
                .map(|(pid, p)| (pid.as_u32(), p.start_time()))
                .collect()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut sys = snap();
        while kids_of(&sys).is_empty() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(200));
            sys = snap();
        }
        let start = |pid: u32| sys.process(Pid::from_u32(pid)).map(|p| p.start_time()).unwrap();
        let kids = kids_of(&sys);
        assert!(!kids.is_empty(), "ping child not found");

        // wrong root start time: nothing is touched
        let mut wrong = vec![(root, start(root) + 60)];
        wrong.extend(kids.iter().copied());
        assert!(terminate_tree(&wrong).is_err());
        assert!(child.try_wait().unwrap().is_none(), "root was killed despite a wrong start time");

        let mut right = vec![(root, start(root))];
        right.extend(kids.iter().copied());
        assert_eq!(terminate_tree(&right).unwrap(), 1 + kids.len());
        assert!(child.wait().is_ok());
        std::thread::sleep(std::time::Duration::from_millis(300));
        let after = snap();
        for (pid, st) in kids {
            assert!(after.process(Pid::from_u32(pid)).map(|p| p.start_time() != st).unwrap_or(true), "child {pid} survived");
        }
    }

    #[test]
    fn refuses_a_reused_or_unknown_pid() {
        // our own process with a wrong start time must be refused (and must not be killed!)
        let me = std::process::id();
        assert!(open_verified(me, 1).is_err());
        assert!(open_verified(me, 0).is_err());
    }
}
