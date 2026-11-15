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

/// Console tools may emit UTF-16 (schtasks /XML with a BOM; wsl.exe without one) or OEM/UTF-8 text.
fn decode(b: &[u8]) -> String {
    let utf16 = |s: &[u8]| {
        let u: Vec<u16> = s.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&u)
    };
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        return utf16(&b[2..]);
    }
    if looks_utf16le(b) {
        return utf16(b);
    }
    String::from_utf8_lossy(b).into_owned()
}

/// BOM-less UTF-16LE: mostly-ASCII text where nearly every odd byte is 0. UTF-8 text never has
/// NUL bytes, so this can't misfire on it.
fn looks_utf16le(b: &[u8]) -> bool {
    if b.len() < 4 || b.len() % 2 != 0 {
        return false;
    }
    let pairs = b.len() / 2;
    let zero_odd = b.iter().skip(1).step_by(2).filter(|&&x| x == 0).count();
    zero_odd * 10 >= pairs * 9
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
    fn decodes_bomless_utf16_from_wsl() {
        let raw: Vec<u8> = "docker-desktop\r\nUbuntu\r\n".encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
        assert_eq!(decode(&raw), "docker-desktop\r\nUbuntu\r\n");
        assert_eq!(decode("plain ascii\n".as_bytes()), "plain ascii\n");
        assert_eq!(decode("héllo wörld".as_bytes()), "héllo wörld");
    }
    #[test]
    fn env() {
        std::env::set_var("AIB_TEST", "X");
        assert_eq!(expand_env("%AIB_TEST%\\a%NOPE_XYZ%"), "X\\a%NOPE_XYZ%");
    }
}
