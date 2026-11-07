//! Re-apply at every logon via an elevated Task Scheduler task.
//! (A Run-key entry can't start elevated, so Task Scheduler is the only clean option.)

use anyhow::{anyhow, Result};

use crate::util;

pub const TASK_NAME: &str = "RigSweep-Reapply";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn current_user() -> String {
    let user = std::env::var("USERNAME").unwrap_or_default();
    match std::env::var("USERDOMAIN") {
        Ok(d) if !d.is_empty() => format!("{d}\\{user}"),
        _ => user,
    }
}

/// Task name used before the rename; removed whenever the new task is (un)registered.
const LEGACY_TASK: &str = "aiBoost-Reapply";

fn remove_legacy() {
    if util::run("schtasks.exe", &["/Query", "/TN", LEGACY_TASK]).map(|o| o.ok).unwrap_or(false) {
        let _ = util::run("schtasks.exe", &["/Delete", "/TN", LEGACY_TASK, "/F"]);
    }
}

pub fn is_registered() -> bool {
    util::run("schtasks.exe", &["/Query", "/TN", TASK_NAME]).map(|o| o.ok).unwrap_or(false)
}

pub fn register(stay_in_tray: bool) -> Result<()> {
    remove_legacy();
    let exe = std::env::current_exe()?;
    let args = if stay_in_tray { "--apply --tray" } else { "--apply" };
    let user = xml_escape(&current_user());
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>RigSweep: re-apply your optimization profile after logon / Windows updates.</Description></RegistrationInfo>
  <Triggers>
    <LogonTrigger><Enabled>true</Enabled><UserId>{user}</UserId><Delay>PT30S</Delay></LogonTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>true</StartWhenAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec><Command>{}</Command><Arguments>{args}</Arguments></Exec>
  </Actions>
</Task>"#,
        xml_escape(&exe.display().to_string())
    );
    // schtasks wants UTF-16LE with BOM
    let mut bytes = vec![0xFF, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    let path = std::env::temp_dir().join("rigsweep-task.xml");
    std::fs::write(&path, bytes)?;
    let r = util::run_ok(
        "schtasks.exe",
        &["/Create", "/TN", TASK_NAME, "/XML", &path.display().to_string(), "/F"],
    );
    let _ = std::fs::remove_file(&path);
    r.map(|_| ()).map_err(|e| anyhow!("couldn't register startup task: {e}"))
}

pub fn unregister() -> Result<()> {
    remove_legacy();
    if !is_registered() {
        return Ok(());
    }
    util::run_ok("schtasks.exe", &["/Delete", "/TN", TASK_NAME, "/F"]).map(|_| ())
}
