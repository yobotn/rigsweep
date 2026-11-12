//! Turns a Run-key or Startup-folder entry off the way Task Manager's "Startup apps" does: through
//! Explorer's StartupApproved list. The entry itself is never deleted, so restoring is exact.
use anyhow::{anyhow, Result};
use serde_json::{json, Value};
use std::borrow::Cow;
use std::path::PathBuf;
use winreg::enums::{RegType, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE};
use winreg::{RegKey, RegValue};

use super::Status;
use crate::catalog::Hive;

const APPROVED: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
pub const LISTS: &[&str] = &["Run", "Run32", "StartupFolder"];

fn root(h: Hive) -> RegKey {
    RegKey::predef(match h {
        Hive::HKLM => HKEY_LOCAL_MACHINE,
        Hive::HKCU => HKEY_CURRENT_USER,
    })
}

/// Registry key that holds the Run entries for a list (None for the Startup folder).
pub fn run_key(h: Hive, list: &str) -> Option<&'static str> {
    match (h, list) {
        (_, "Run") => Some(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
        (Hive::HKLM, "Run32") => Some(r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run"),
        _ => None,
    }
}

pub fn startup_folder(h: Hive) -> PathBuf {
    match h {
        Hive::HKCU => PathBuf::from(std::env::var("APPDATA").unwrap_or_default()).join(r"Microsoft\Windows\Start Menu\Programs\Startup"),
        Hive::HKLM => PathBuf::from(std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into()))
            .join(r"Microsoft\Windows\Start Menu\Programs\StartUp"),
    }
}

fn entry_exists(h: Hive, list: &str, name: &str) -> bool {
    if list == "StartupFolder" {
        return startup_folder(h).join(name).exists();
    }
    run_key(h, list)
        .and_then(|p| root(h).open_subkey_with_flags(p, KEY_READ).ok())
        .is_some_and(|k| k.get_raw_value(name).is_ok())
}

fn read_flag(h: Hive, list: &str, name: &str) -> Option<Vec<u8>> {
    let k = root(h).open_subkey_with_flags(format!(r"{APPROVED}\{list}"), KEY_READ).ok()?;
    k.get_raw_value(name).ok().map(|v| v.bytes.into_owned())
}

/// Explorer treats an odd first byte as "disabled" (02 = on, 03 = off, 06/07 for some installers).
pub fn is_disabled(h: Hive, list: &str, name: &str) -> bool {
    read_flag(h, list, name).is_some_and(|b| b.first().is_some_and(|x| x & 1 == 1))
}

pub fn status(h: Hive, list: &str, name: &str) -> Result<Status> {
    if !entry_exists(h, list, name) {
        return Ok(Status::NotPresent);
    }
    Ok(if is_disabled(h, list, name) { Status::Applied } else { Status::NotApplied })
}

pub fn read_current(h: Hive, list: &str, name: &str) -> Result<Value> {
    Ok(match read_flag(h, list, name) {
        None => json!({ "exists": false }),
        Some(b) => json!({ "exists": true, "data": b }),
    })
}

fn write(h: Hive, list: &str, name: &str, bytes: Vec<u8>) -> Result<()> {
    let (k, _) = root(h).create_subkey_with_flags(format!(r"{APPROVED}\{list}"), KEY_WRITE | KEY_READ)?;
    k.set_raw_value(name, &RegValue { bytes: Cow::Owned(bytes), vtype: RegType::REG_BINARY })?;
    Ok(())
}

pub fn apply(h: Hive, list: &str, name: &str) -> Result<()> {
    // 03 00 00 00 + the FILETIME it was turned off (shown by Task Manager)
    let ft = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64 / 100 + 116_444_736_000_000_000)
        .unwrap_or(0);
    let mut b = vec![3u8, 0, 0, 0];
    b.extend_from_slice(&ft.to_le_bytes());
    write(h, list, name, b)
}

pub fn revert(h: Hive, list: &str, name: &str, original: &Value) -> Result<()> {
    if original["exists"] == json!(false) {
        if let Ok(k) = root(h).open_subkey_with_flags(format!(r"{APPROVED}\{list}"), KEY_WRITE) {
            match k.delete_value(name) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.into()),
            }
        }
        return Ok(());
    }
    let bytes: Vec<u8> = original["data"]
        .as_array()
        .ok_or_else(|| anyhow!("backup missing data"))?
        .iter()
        .map(|v| v.as_u64().unwrap_or(0) as u8)
        .collect();
    write(h, list, name, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a throwaway HKCU Run entry, turns it off and back on, then removes it.
    #[test]
    fn roundtrip_on_dummy_entry() {
        let name = "RigSweepSelfTest";
        let run = root(Hive::HKCU).create_subkey(run_key(Hive::HKCU, "Run").unwrap()).unwrap().0;
        run.set_value(name, &"cmd.exe /c exit".to_string()).unwrap();
        assert_eq!(status(Hive::HKCU, "Run", name).unwrap(), Status::NotApplied);
        let orig = read_current(Hive::HKCU, "Run", name).unwrap();
        apply(Hive::HKCU, "Run", name).unwrap();
        assert_eq!(status(Hive::HKCU, "Run", name).unwrap(), Status::Applied);
        revert(Hive::HKCU, "Run", name, &orig).unwrap();
        assert_eq!(status(Hive::HKCU, "Run", name).unwrap(), Status::NotApplied);
        assert!(read_flag(Hive::HKCU, "Run", name).is_none());
        run.delete_value(name).unwrap();
        assert_eq!(status(Hive::HKCU, "Run", name).unwrap(), Status::NotPresent);
    }
}
