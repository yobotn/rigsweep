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
    let params = args
        .iter()
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
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
