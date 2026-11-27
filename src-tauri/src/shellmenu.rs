// File Explorer's right-click menu: "Free up space with RigSweep" on folders.
//
// The entry is off until the user turns it on in Settings > Dev Disk. It lives in the user's own
// registry hive (HKCU\Software\Classes), so turning it off or uninstalling removes it completely.
// The menu runs `RigSweep.exe --scan-folder "<folder>"`, which opens Dev Disk with only that
// folder scanned. Nothing is cleaned until the user picks rows and confirms, as on the full scan.
//
// When RigSweep already runs (elevated), the menu's copy runs without admin rights and hands the
// folder over through the single-instance window instead of asking for UAC again. Windows blocks
// WM_COPYDATA from a normal process to an elevated one, so the app lets that one message through
// for its own single-instance window (allow_from_unelevated). The only thing taken from such a
// message is a folder path, which must exist, and it only starts a read-only scan.

use std::path::Path;
use std::sync::Mutex;

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, ChangeWindowMessageFilterEx, FindWindowW, GetWindowThreadProcessId, SendMessageTimeoutW,
    MSGFLT_ALLOW, SMTO_ABORTIFHUNG, WM_COPYDATA,
};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

pub const FLAG: &str = "--scan-folder";

/// The app identifier (tauri.conf.json): the single-instance plugin names its window after it.
const APP_ID: &str = "io.github.rigsweep";
/// The plugin's payload kind: the working folder and the arguments, separated by NUL.
const COPYDATA_NUL: usize = 1543;

/// (key, placeholder): a click on a folder passes %1, a click inside an open folder passes %V
const KEYS: [(&str, &str); 2] = [
    (r"Software\Classes\Directory\shell\RigSweep", "%1"),
    (r"Software\Classes\Directory\Background\shell\RigSweep", "%V"),
];

static PENDING: Mutex<Option<String>> = Mutex::new(None);

/// The folder after `--scan-folder`, if it is an existing absolute folder.
pub fn folder_arg(args: &[String]) -> Option<String> {
    let i = args.iter().position(|a| a.eq_ignore_ascii_case(FLAG))?;
    clean_folder(args.get(i + 1)?)
}

fn clean_folder(raw: &str) -> Option<String> {
    // "D:\" on a command line arrives as D:" (the backslash escapes the closing quote)
    let mut s = raw.trim().to_string();
    if s.ends_with('"') {
        s.pop();
        s.push('\\');
    }
    let p = Path::new(&s);
    if s.is_empty() || s.contains('\0') || !p.is_absolute() || !p.is_dir() {
        return None;
    }
    Some(s)
}

pub fn set_pending(folder: String) {
    if let Ok(mut p) = PENDING.lock() {
        *p = Some(folder);
    }
}

pub fn take_pending() -> Option<String> {
    PENDING.lock().ok().and_then(|mut p| p.take())
}

fn names() -> (HSTRING, HSTRING) {
    (HSTRING::from(format!("{APP_ID}-sic")), HSTRING::from(format!("{APP_ID}-siw")))
}

/// Give the folder to a RigSweep that already runs. False when none runs (or it didn't answer).
pub fn forward(folder: &str) -> bool {
    let (class, window) = names();
    let Ok(hwnd) = (unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(window.as_ptr())) }) else { return false };
    if hwnd.is_invalid() {
        return false;
    }
    // let the running app bring its window to the front
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid != 0 {
        let _ = unsafe { AllowSetForegroundWindow(pid) };
    }
    let cwd = std::env::current_dir().unwrap_or_default().to_string_lossy().into_owned();
    let exe = std::env::current_exe().unwrap_or_default().to_string_lossy().into_owned();
    let data = [cwd.as_str(), exe.as_str(), FLAG, folder].join("\0");
    #[repr(C)]
    struct CopyData {
        kind: usize,
        len: u32,
        ptr: *const u8,
    }
    let cds = CopyData { kind: COPYDATA_NUL, len: data.len() as u32, ptr: data.as_ptr() };
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            WPARAM(0),
            LPARAM(&cds as *const CopyData as isize),
            SMTO_ABORTIFHUNG,
            10_000,
            Some(&mut result),
        )
    };
    sent.0 != 0 && result != 0
}

/// Let the menu's copy (not elevated) reach this elevated app's single-instance window.
/// The plugin creates its window during setup: retry for a few seconds in case it isn't there yet.
pub fn allow_from_unelevated() {
    std::thread::spawn(|| {
        let (class, window) = names();
        for _ in 0..50 {
            if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR(window.as_ptr())) } {
                if !hwnd.is_invalid() && owned_by_us(hwnd) {
                    let _ = unsafe { ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW, None) };
                    return;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });
}

fn owned_by_us(hwnd: HWND) -> bool {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    pid == std::process::id()
}

pub fn is_on() -> bool {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    KEYS.iter().any(|(k, _)| hk.open_subkey(format!(r"{k}\command")).is_ok())
}

/// The command a menu entry runs.
fn command(exe: &str, placeholder: &str) -> String {
    format!("\"{exe}\" {FLAG} \"{placeholder}\"")
}

pub fn set(on: bool, label: &str) -> Result<(), String> {
    let hk = RegKey::predef(HKEY_CURRENT_USER);
    if !on {
        for (k, _) in KEYS {
            match hk.delete_subkey_all(k) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        return Ok(());
    }
    let label: String = label.trim().chars().filter(|c| !c.is_control()).take(80).collect();
    let label = if label.is_empty() { "Free up space with RigSweep".to_string() } else { label };
    let exe = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().into_owned();
    for (k, ph) in KEYS {
        let (key, _) = hk.create_subkey(k).map_err(|e| e.to_string())?;
        key.set_value("MUIVerb", &label).map_err(|e| e.to_string())?;
        key.set_value("Icon", &format!("\"{exe}\",0")).map_err(|e| e.to_string())?;
        let (cmd, _) = key.create_subkey("command").map_err(|e| e.to_string())?;
        cmd.set_value("", &command(&exe, ph)).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_arg_needs_an_existing_absolute_folder() {
        let tmp = std::env::temp_dir().to_string_lossy().into_owned();
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(folder_arg(&args(&["--scan-folder", &tmp])), Some(tmp.clone()));
        assert_eq!(folder_arg(&args(&["x.exe", "--SCAN-FOLDER", &tmp])), Some(tmp.clone()));
        assert_eq!(folder_arg(&args(&["--scan-folder"])), None);
        assert_eq!(folder_arg(&args(&["--scan-folder", "relative\\dir"])), None);
        assert_eq!(folder_arg(&args(&["--scan-folder", "C:\\no\\such\\folder\\rigsweep-test"])), None);
        assert_eq!(folder_arg(&args(&["--tray"])), None);
    }

    #[test]
    fn a_drive_root_survives_the_command_line_quote() {
        let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        // what Rust's argument parser makes of "C:\"
        assert_eq!(clean_folder(&format!("{sys}\"")), Some(format!("{sys}\\")));
    }

    #[test]
    fn menu_command_quotes_the_exe_and_the_folder() {
        assert_eq!(
            command(r"C:\Program Files\RigSweep\rigsweep.exe", "%1"),
            r#""C:\Program Files\RigSweep\rigsweep.exe" --scan-folder "%1""#
        );
    }

    #[test]
    fn pending_folder_is_taken_once() {
        set_pending("C:\\x".into());
        assert_eq!(take_pending().as_deref(), Some("C:\\x"));
        assert_eq!(take_pending(), None);
    }
}
