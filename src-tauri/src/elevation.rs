use windows::core::{HSTRING, PCWSTR};
use windows::Win32::UI::Shell::IsUserAnAdmin;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, SW_SHOWNORMAL};

pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin().as_bool() }
}

/// Relaunch this exe with a UAC prompt. Returns true if the elevated copy started.
///
/// Debug builds wait for the elevated copy to exit: `tauri dev` serves the UI from a local dev
/// server and stops it as soon as the process it launched exits, which would leave the elevated
/// window with a page that never loads.
pub fn relaunch_elevated(args: &[String]) -> bool {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};

    let Ok(exe) = std::env::current_exe() else { return false };
    let params = args.iter().map(|a| quote_arg(a)).collect::<Vec<_>>().join(" ");
    let (verb, file, params) = (HSTRING::from("runas"), HSTRING::from(exe.as_os_str()), HSTRING::from(params));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    if unsafe { ShellExecuteExW(&mut info) }.is_err() {
        return false;
    }
    if !info.hProcess.is_invalid() {
        if cfg!(debug_assertions) {
            unsafe { WaitForSingleObject(info.hProcess, INFINITE) };
        }
        let _ = unsafe { CloseHandle(info.hProcess) };
    }
    true
}

pub fn error_box(msg: &str) {
    unsafe {
        MessageBoxW(None, &HSTRING::from(msg), &HSTRING::from("RigSweep"), MB_OK | MB_ICONERROR);
    }
}

/// One argument as the C runtime reads a command line back: quoted when it has a space, a tab or
/// a quote, with the backslashes before a quote (and at the end, "D:\\") doubled.
pub fn quote_arg(a: &str) -> String {
    if !a.is_empty() && !a.contains([' ', '\t', '"']) {
        return a.to_string();
    }
    let mut out = String::from("\"");
    let mut slashes = 0;
    for c in a.chars() {
        match c {
            '\\' => slashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(slashes * 2 + 1));
                out.push('"');
                slashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(slashes));
                out.push(c);
                slashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(slashes * 2));
    out.push('"');
    out
}

#[cfg(test)]
mod quote_tests {
    use super::quote_arg;

    #[test]
    fn quotes_like_the_c_runtime_reads_it() {
        assert_eq!(quote_arg("--tray"), "--tray");
        assert_eq!(quote_arg(r"D:\"), r"D:\");
        assert_eq!(quote_arg(""), r#""""#);
        assert_eq!(quote_arg(r"C:\My Projects\"), r#""C:\My Projects\\""#);
        assert_eq!(quote_arg(r#"a "b""#), r#""a \"b\"""#);
        assert_eq!(quote_arg(r"C:\x y\z"), r#""C:\x y\z""#);
    }
}
