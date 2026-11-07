use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use super::{Ctx, Status};
use crate::util;

const HIGH: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const ULTIMATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";

fn base_guid(scheme: &str) -> &'static str {
    if scheme == "ultimate" { ULTIMATE } else { HIGH }
}

pub fn active() -> Result<String> {
    let out = util::run_ok("powercfg.exe", &["/getactivescheme"])?;
    util::find_guid(&out).ok_or_else(|| anyhow!("can't parse active power scheme"))
}

fn exists(guid: &str) -> Result<bool> {
    let out = util::run_ok("powercfg.exe", &["/list"])?;
    Ok(out.to_ascii_lowercase().contains(guid))
}

pub fn status(scheme: &str, ctx: &Ctx) -> Result<Status> {
    let a = active()?;
    let ok = a == base_guid(scheme) || a == HIGH || a == ULTIMATE || ctx.power_dup_guid.as_deref() == Some(a.as_str());
    Ok(if ok { Status::Applied } else { Status::NotApplied })
}

pub fn read_current() -> Result<Value> {
    Ok(json!({ "guid": active()? }))
}

/// Returns the GUID of a duplicated scheme if one had to be created (stored by the caller).
pub fn apply(scheme: &str, ctx: &Ctx) -> Result<Option<String>> {
    let base = base_guid(scheme);
    if exists(base)? {
        util::run_ok("powercfg.exe", &["/setactive", base])?;
        return Ok(None);
    }
    if let Some(dup) = &ctx.power_dup_guid {
        if exists(dup)? {
            util::run_ok("powercfg.exe", &["/setactive", dup])?;
            return Ok(None);
        }
    }
    // Hidden on Modern Standby machines: duplicate it to make it selectable.
    let out = util::run_ok("powercfg.exe", &["-duplicatescheme", base])?;
    let dup = util::find_guid(&out).ok_or_else(|| anyhow!("powercfg didn't return a new scheme GUID"))?;
    util::run_ok("powercfg.exe", &["/setactive", &dup])?;
    Ok(Some(dup))
}

pub fn revert(original: &Value) -> Result<()> {
    let g = original["guid"].as_str().ok_or_else(|| anyhow!("backup missing power scheme"))?;
    util::run_ok("powercfg.exe", &["/setactive", g]).map(|_| ())
}
