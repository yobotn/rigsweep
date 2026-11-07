use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{json, Value};
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::System::Services::*;
use winreg::enums::HKEY_LOCAL_MACHINE;
use winreg::RegKey;

use super::Status;
use crate::catalog::StartType;
use crate::util;

// Registry "Start" values
const AUTO: u32 = 2;
const DEMAND: u32 = 3;
const DISABLED: u32 = 4;

#[derive(Serialize, Clone, Debug)]
pub struct SvcInfo {
    pub name: String,
    pub display: String,
    pub pid: u32,
    pub running: bool,
}

struct Handle(SC_HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

/// (Start, DelayedAutostart) read from the service's registry key. None = service doesn't exist.
fn read_start(name: &str) -> Result<Option<(u32, bool)>> {
    let hk = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hk.open_subkey(format!("SYSTEM\\CurrentControlSet\\Services\\{name}")) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let start: u32 = match key.get_value("Start") {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let delayed: u32 = key.get_value("DelayedAutostart").unwrap_or(0);
    Ok(Some((start, delayed == 1)))
}

pub fn is_running(name: &str) -> bool {
    unsafe {
        let Ok(scm) = OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) else {
            return false;
        };
        let scm = Handle(scm);
        let Ok(svc) = OpenServiceW(scm.0, &HSTRING::from(name), SERVICE_QUERY_STATUS) else {
            return false;
        };
        let svc = Handle(svc);
        let mut st = SERVICE_STATUS::default();
        QueryServiceStatus(svc.0, &mut st).is_ok() && st.dwCurrentState == SERVICE_RUNNING
    }
}

/// All Win32 services with their hosting PID.
pub fn list_services() -> Result<Vec<SvcInfo>> {
    unsafe {
        let scm = Handle(OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_ENUMERATE_SERVICE)?);
        let mut needed = 0u32;
        let mut count = 0u32;
        let mut resume = 0u32;
        // first call to size the buffer
        let _ = EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            None,
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        );
        let mut buf: Vec<u64> = vec![0; (needed as usize / 8) + 1024];
        let bytes = std::slice::from_raw_parts_mut(buf.as_mut_ptr() as *mut u8, buf.len() * 8);
        resume = 0;
        EnumServicesStatusExW(
            scm.0,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            Some(bytes),
            &mut needed,
            &mut count,
            Some(&mut resume),
            PCWSTR::null(),
        )?;
        let items = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, count as usize);
        Ok(items
            .iter()
            .map(|e| SvcInfo {
                name: e.lpServiceName.to_string().unwrap_or_default(),
                display: e.lpDisplayName.to_string().unwrap_or_default(),
                pid: e.ServiceStatusProcess.dwProcessId,
                running: e.ServiceStatusProcess.dwCurrentState == SERVICE_RUNNING,
            })
            .collect())
    }
}

fn target_value(t: StartType) -> u32 {
    match t {
        StartType::Disabled => DISABLED,
        StartType::Manual => DEMAND,
    }
}

pub fn status(name: &str, target: StartType) -> Result<Status> {
    Ok(match read_start(name)? {
        None => Status::NotPresent,
        Some((start, _)) if start == target_value(target) => Status::Applied,
        Some(_) => Status::NotApplied,
    })
}

pub fn read_current(name: &str) -> Result<Value> {
    match read_start(name)? {
        None => Ok(json!({ "exists": false })),
        Some((start, delayed)) => Ok(json!({
            "exists": true, "start": start, "delayed": delayed, "running": is_running(name)
        })),
    }
}

fn sc_config(name: &str, start: &str) -> Result<()> {
    util::run_ok("sc.exe", &["config", name, "start=", start]).map(|_| ())
}

pub fn apply(name: &str, target: StartType, stop: bool) -> Result<()> {
    let start = match target {
        StartType::Disabled => "disabled",
        StartType::Manual => "demand",
    };
    sc_config(name, start)?;
    if stop && is_running(name) {
        // best effort: some services refuse to stop until reboot
        let _ = util::run("sc.exe", &["stop", name]);
    }
    Ok(())
}

pub fn revert(name: &str, original: &Value) -> Result<()> {
    if original["exists"] == json!(false) {
        return Ok(());
    }
    let start = original["start"].as_u64().ok_or_else(|| anyhow!("backup missing start type"))? as u32;
    let delayed = original["delayed"].as_bool().unwrap_or(false);
    let s = match (start, delayed) {
        (AUTO, true) => "delayed-auto",
        (AUTO, false) => "auto",
        (DEMAND, _) => "demand",
        (DISABLED, _) => "disabled",
        (other, _) => return Err(anyhow!("refusing to restore unusual start type {other}")),
    };
    sc_config(name, s)?;
    if original["running"].as_bool().unwrap_or(false) && !is_running(name) {
        let _ = util::run("sc.exe", &["start", name]);
    }
    Ok(())
}
