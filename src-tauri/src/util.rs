use anyhow::{anyhow, Result};
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct Out {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Run a console tool without flashing a console window.
pub fn run(exe: &str, args: &[&str]) -> Result<Out> {
    let o = Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| anyhow!("failed to run {exe}: {e}"))?;
    Ok(Out {
        ok: o.status.success(),
        stdout: decode(&o.stdout),
        stderr: decode(&o.stderr),
    })
}

pub fn run_ok(exe: &str, args: &[&str]) -> Result<String> {
    let o = run(exe, args)?;
    if o.ok {
        Ok(o.stdout)
    } else {
        let msg = if o.stderr.trim().is_empty() { o.stdout } else { o.stderr };
        Err(anyhow!("{exe} {}: {}", args.first().unwrap_or(&""), msg.trim()))
    }
}

/// Run a PowerShell snippet (non-interactive, no profile).
pub fn powershell(script: &str) -> Result<String> {
    let full = format!("$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; [Console]::OutputEncoding=[Text.Encoding]::UTF8; {script}");
    run_ok(
        "powershell.exe",
        &["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &full],
    )
}

/// Quote a string for a single-quoted PowerShell literal.
pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Console tools may emit UTF-16 (schtasks /XML) or OEM/UTF-8 text.
fn decode(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&u);
    }
    String::from_utf8_lossy(b).into_owned()
}

pub fn expand_env(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            match std::env::var(name) {
                Ok(v) if !name.is_empty() => out.push_str(&v),
                _ => {
                    out.push('%');
                    out.push_str(name);
                    out.push('%');
                }
            }
            rest = &after[end + 1..];
        } else {
            out.push('%');
            rest = after;
        }
    }
    out.push_str(rest);
    out
}

/// Find the first GUID (8-4-4-4-12 hex) in a string; locale independent.
pub fn find_guid(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let groups = [8, 4, 4, 4, 12];
    'outer: for i in 0..b.len() {
        let mut p = i;
        for (gi, g) in groups.iter().enumerate() {
            for _ in 0..*g {
                if p >= b.len() || !b[p].is_ascii_hexdigit() {
                    continue 'outer;
                }
                p += 1;
            }
            if gi < 4 {
                if p >= b.len() || b[p] != b'-' {
                    continue 'outer;
                }
                p += 1;
            }
        }
        return Some(s[i..p].to_ascii_lowercase());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guid() {
        assert_eq!(
            find_guid("Power Scheme GUID: 381B4222-F694-41F0-9685-FF5BB260DF2E  (Balanced)").as_deref(),
            Some("381b4222-f694-41f0-9685-ff5bb260df2e")
        );
        assert_eq!(find_guid("nothing here"), None);
    }
    #[test]
    fn env() {
        std::env::set_var("AIB_TEST", "X");
        assert_eq!(expand_env("%AIB_TEST%\\a%NOPE_XYZ%"), "X\\a%NOPE_XYZ%");
    }
}
