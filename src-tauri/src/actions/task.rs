use anyhow::Result;
use serde_json::{json, Value};

use super::Status;
use crate::util;

/// None = task doesn't exist. Uses the XML export so it works on any Windows display language.
fn enabled(path: &str) -> Result<Option<bool>> {
    let o = util::run("schtasks.exe", &["/Query", "/TN", path, "/XML"])?;
    if !o.ok {
        return Ok(None);
    }
    let xml = o.stdout;
    let settings = match xml.find("<Settings>") {
        Some(i) => &xml[i..],
        None => return Ok(Some(true)),
    };
    let end = settings.find("</Settings>").unwrap_or(settings.len());
    let settings = &settings[..end];
    // Settings/Enabled defaults to true when absent. Nested elements like IdleSettings
    // don't contain <Enabled>, so the first match belongs to Settings.
    Ok(Some(!settings.contains("<Enabled>false</Enabled>")))
}

pub fn status(path: &str) -> Result<Status> {
    Ok(match enabled(path)? {
        None => Status::NotPresent,
        Some(false) => Status::Applied,
        Some(true) => Status::NotApplied,
    })
}

pub fn read_current(path: &str) -> Result<Value> {
    Ok(match enabled(path)? {
        None => json!({ "exists": false }),
        Some(e) => json!({ "exists": true, "enabled": e }),
    })
}

pub fn set_enabled(path: &str, on: bool) -> Result<()> {
    if enabled(path)?.is_none() {
        return Ok(());
    }
    let flag = if on { "/ENABLE" } else { "/DISABLE" };
    util::run_ok("schtasks.exe", &["/Change", "/TN", path, flag]).map(|_| ())
}

pub fn revert(path: &str, original: &Value) -> Result<()> {
    if original["exists"] == json!(true) {
        set_enabled(path, original["enabled"].as_bool().unwrap_or(true))?;
    }
    Ok(())
}
