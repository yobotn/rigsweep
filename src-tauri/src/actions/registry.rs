use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::RegKey;

use super::Status;
use crate::catalog::{Hive, RegKind};

fn root(h: Hive) -> RegKey {
    RegKey::predef(match h {
        Hive::HKLM => HKEY_LOCAL_MACHINE,
        Hive::HKCU => HKEY_CURRENT_USER,
    })
}

enum Current {
    Missing,
    Dword(u32),
    Str(String),
    Other,
}

fn read(h: Hive, path: &str, value: &str) -> Result<Current> {
    let key = match root(h).open_subkey_with_flags(path, KEY_READ) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Current::Missing),
        Err(e) => return Err(e.into()),
    };
    if let Ok(v) = key.get_value::<u32, _>(value) {
        return Ok(Current::Dword(v));
    }
    if let Ok(v) = key.get_value::<String, _>(value) {
        return Ok(Current::Str(v));
    }
    match key.get_raw_value(value) {
        Ok(_) => Ok(Current::Other),
        Err(_) => Ok(Current::Missing),
    }
}

pub fn status(h: Hive, path: &str, value: &str, kind: RegKind, data: &Value) -> Result<Status> {
    let cur = read(h, path, value)?;
    let applied = match (kind, cur) {
        (RegKind::Dword, Current::Dword(v)) => Some(v as u64) == data.as_u64(),
        (RegKind::String, Current::Str(s)) => Some(s.as_str()) == data.as_str(),
        _ => false,
    };
    Ok(if applied { Status::Applied } else { Status::NotApplied })
}

pub fn read_current(h: Hive, path: &str, value: &str) -> Result<Value> {
    Ok(match read(h, path, value)? {
        Current::Missing => json!({ "exists": false }),
        Current::Dword(v) => json!({ "exists": true, "kind": "dword", "data": v }),
        Current::Str(s) => json!({ "exists": true, "kind": "string", "data": s }),
        Current::Other => return Err(anyhow!("{value} has an unsupported registry type; not touching it")),
    })
}

fn write(h: Hive, path: &str, value: &str, kind: &str, data: &Value) -> Result<()> {
    let (key, _) = root(h).create_subkey_with_flags(path, KEY_WRITE | KEY_READ)?;
    match kind {
        "dword" => {
            let v = data.as_u64().ok_or_else(|| anyhow!("dword data must be a number"))? as u32;
            key.set_value(value, &v)?;
        }
        _ => {
            let v = data.as_str().ok_or_else(|| anyhow!("string data must be a string"))?;
            key.set_value(value, &v.to_string())?;
        }
    }
    Ok(())
}

pub fn apply(h: Hive, path: &str, value: &str, kind: RegKind, data: &Value) -> Result<()> {
    let k = match kind {
        RegKind::Dword => "dword",
        RegKind::String => "string",
    };
    write(h, path, value, k, data)
}

pub fn revert(h: Hive, path: &str, value: &str, original: &Value) -> Result<()> {
    if original["exists"] == json!(false) {
        if let Ok(key) = root(h).open_subkey_with_flags(path, KEY_WRITE) {
            match key.delete_value(value) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        return Ok(());
    }
    let kind = original["kind"].as_str().unwrap_or("dword");
    write(h, path, value, kind, &original["data"])
}
