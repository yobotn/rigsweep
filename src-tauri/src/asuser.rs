// Run a program as the signed-in user, without admin rights.
//
// The app runs elevated, but some programs it starts read files any program of the user's can
// write: git reads the repository's .git/config (core.fsmonitor, filters run commands), and an AI
// CLI reads its own config, MCP servers and hooks. Run as admin, those would hand admin rights to
// whatever wrote them. So the app starts a copy of itself with the desktop's token
// (`RigSweep.exe --as-user`, the token of the Explorer that owns the shell window), and that copy
// runs the program with std::process::Command (its own argument quoting, .cmd files included).
// Requests and answers go over the copy's stdin/stdout, one JSON line each. Closing its stdin
// stops whatever it is running and ends it.
//
// When the app isn't elevated (the CLI, tests) the program is simply run directly.

use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const FLAG: &str = "--as-user";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Req {
    pub exe: String,
    pub args: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env_remove: Vec<String>,
    /// written to the program's stdin, then closed (None: no stdin)
    #[serde(default)]
    pub stdin: Option<String>,
    pub timeout_secs: u64,
    /// bytes kept of stdout and of stderr each
    pub max_output: usize,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Resp {
    /// None when it was stopped, timed out or couldn't start (see `error`)
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub error: Option<String>,
}

impl Resp {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }
}

fn fail(e: String) -> Resp {
    Resp { error: Some(e), ..Default::default() }
}

/// Run the program here, in this process's own account. `stop` is polled while it runs.
fn exec(req: &Req, stop: &dyn Fn() -> bool) -> Resp {
    let mut c = Command::new(&req.exe);
    c.args(&req.args)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(if req.stdin.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(d) = &req.cwd {
        c.current_dir(d);
    }
    for k in &req.env_remove {
        c.env_remove(k);
    }
    let mut child = match c.spawn() {
        Ok(ch) => ch,
        Err(e) => return fail(format!("could not start {}: {e}", req.exe)),
    };
    if let (Some(mut si), Some(body)) = (child.stdin.take(), req.stdin.clone()) {
        std::thread::spawn(move || {
            let _ = si.write_all(body.as_bytes());
        });
    }
    let max = req.max_output as u64;
    let read = |s: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut b = vec![];
            if let Some(mut s) = s {
                let _ = s.by_ref().take(max).read_to_end(&mut b);
                let _ = std::io::copy(&mut s, &mut std::io::sink());
            }
            String::from_utf8_lossy(&b).into_owned()
        })
    };
    let t_out = read(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let t_err = read(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let start = Instant::now();
    let limit = Duration::from_secs(req.timeout_secs.max(1));
    let (code, timed_out, error) = loop {
        match child.try_wait() {
            Ok(Some(s)) => break (Some(s.code().unwrap_or(-1)), false, None),
            Ok(None) if stop() => {
                let _ = child.kill();
                let _ = child.wait();
                break (None, false, Some("stopped".to_string()));
            }
            Ok(None) if start.elapsed() > limit => {
                let _ = child.kill();
                let _ = child.wait();
                break (None, true, None);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => break (None, false, Some(e.to_string())),
        }
    };
    Resp { code, stdout: t_out.join().unwrap_or_default(), stderr: t_err.join().unwrap_or_default(), timed_out, error }
}

/// `RigSweep.exe --as-user`: answer requests from the elevated app until its stdin closes.
pub fn serve() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static CLOSED: AtomicBool = AtomicBool::new(false);
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(l) = line else { break };
            if tx.send(l).is_err() {
                break;
            }
        }
        CLOSED.store(true, Ordering::SeqCst);
    });
    let mut out = std::io::stdout();
    while let Ok(line) = rx.recv() {
        let resp = match serde_json::from_str::<Req>(&line) {
            // the app closing the pipe means stop
            Ok(req) => exec(&req, &|| CLOSED.load(Ordering::SeqCst)),
            Err(e) => fail(format!("bad request: {e}")),
        };
        let Ok(s) = serde_json::to_string(&resp) else { break };
        if writeln!(out, "{s}").and_then(|_| out.flush()).is_err() {
            break;
        }
    }
}

/// One `--as-user` copy, with a thread reading its answers.
struct Helper {
    stdin: std::fs::File,
    answers: Receiver<String>,
    process: windows::Win32::Foundation::HANDLE,
}

// the process handle is only waited on and closed
unsafe impl Send for Helper {}

impl Drop for Helper {
    fn drop(&mut self) {
        // its stdin closes with self.stdin: it stops what it runs and exits
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.process);
        }
    }
}

static IDLE: Mutex<Vec<Helper>> = Mutex::new(Vec::new());
const MAX_IDLE: usize = 4;

/// Run a program as the signed-in user. `stop` is polled while it runs (Stop in a cleanup).
/// Elevated, it fails when there's no desktop to take the user's token from (Explorer isn't
/// running): the program is never run with admin rights instead.
pub fn run(req: &Req, stop: &dyn Fn() -> bool) -> Resp {
    if !crate::elevation::is_elevated() {
        return exec(req, stop);
    }
    let line = match serde_json::to_string(req) {
        Ok(l) => l,
        Err(e) => return fail(e.to_string()),
    };
    let mut h = match IDLE.lock().ok().and_then(|mut v| v.pop()) {
        Some(h) => h,
        None => match spawn_helper() {
            Ok(h) => h,
            Err(e) => return fail(format!("could not run {} without admin rights: {e}", req.exe)),
        },
    };
    if writeln!(h.stdin, "{line}").and_then(|_| h.stdin.flush()).is_err() {
        // an idle copy that died: try once with a new one
        h = match spawn_helper() {
            Ok(h) => h,
            Err(e) => return fail(format!("could not run {} without admin rights: {e}", req.exe)),
        };
        if writeln!(h.stdin, "{line}").and_then(|_| h.stdin.flush()).is_err() {
            return fail("the helper without admin rights stopped".into());
        }
    }
    // its own timeout, plus a margin for starting up
    let limit = Instant::now() + Duration::from_secs(req.timeout_secs.max(1) + 30);
    loop {
        match h.answers.recv_timeout(Duration::from_millis(100)) {
            Ok(a) => {
                let resp = serde_json::from_str::<Resp>(&a).unwrap_or_else(|e| fail(format!("bad answer: {e}")));
                if let Ok(mut v) = IDLE.lock() {
                    if v.len() < MAX_IDLE {
                        v.push(h);
                    }
                }
                return resp;
            }
            Err(RecvTimeoutError::Timeout) if stop() => return fail("stopped".into()),
            Err(RecvTimeoutError::Timeout) if Instant::now() > limit => return Resp { timed_out: true, ..Default::default() },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return fail("the helper without admin rights stopped".into()),
        }
    }
}

/// Start `RigSweep.exe --as-user` with the token of the Explorer that owns the desktop.
fn spawn_helper() -> Result<Helper, String> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, IntoRawHandle};
    use windows::core::{HSTRING, PWSTR};
    use windows::Win32::Foundation::{CloseHandle, SetHandleInformation, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS};
    use windows::Win32::Security::{DuplicateTokenEx, SecurityImpersonation, TokenPrimary, TOKEN_ACCESS_MASK, TOKEN_ADJUST_DEFAULT, TOKEN_ADJUST_SESSIONID, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_QUERY};
    use windows::Win32::System::Threading::{
        CreateProcessWithTokenW, OpenProcess, OpenProcessToken, CREATE_NO_WINDOW as NO_WINDOW, CREATE_UNICODE_ENVIRONMENT, CREATE_PROCESS_LOGON_FLAGS, PROCESS_INFORMATION,
        PROCESS_QUERY_LIMITED_INFORMATION, STARTF_USESTDHANDLES, STARTUPINFOW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetShellWindow, GetWindowThreadProcessId};

    // tests run the release build's copy: the test binary itself can't serve
    let exe = match std::env::var_os("RIGSWEEP_AS_USER_EXE").filter(|_| cfg!(test)) {
        Some(p) => std::path::PathBuf::from(p),
        None => std::env::current_exe().map_err(|e| e.to_string())?,
    };
    let (in_r, in_w) = std::io::pipe().map_err(|e| e.to_string())?;
    let (out_r, out_w) = std::io::pipe().map_err(|e| e.to_string())?;
    unsafe {
        let shell = GetShellWindow();
        if shell.is_invalid() {
            return Err("no desktop (Explorer isn't running)".into());
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(shell, Some(&mut pid));
        if pid == 0 {
            return Err("no desktop process".into());
        }
        let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).map_err(|e| e.to_string())?;
        let mut tok = HANDLE::default();
        let opened = OpenProcessToken(proc, TOKEN_DUPLICATE | TOKEN_QUERY, &mut tok);
        let _ = CloseHandle(proc);
        opened.map_err(|e| e.to_string())?;
        let mut primary = HANDLE::default();
        let access = TOKEN_ACCESS_MASK(TOKEN_QUERY.0 | TOKEN_DUPLICATE.0 | TOKEN_ASSIGN_PRIMARY.0 | TOKEN_ADJUST_DEFAULT.0 | TOKEN_ADJUST_SESSIONID.0);
        let dup = DuplicateTokenEx(tok, access, None, SecurityImpersonation, TokenPrimary, &mut primary);
        let _ = CloseHandle(tok);
        dup.map_err(|e| e.to_string())?;

        // the copy's ends of the pipes are inherited; ours stay private
        let child_in = HANDLE(in_r.as_raw_handle());
        let child_out = HANDLE(out_w.as_raw_handle());
        for h in [child_in, child_out] {
            SetHandleInformation(h, HANDLE_FLAG_INHERIT.0, HANDLE_FLAG_INHERIT).map_err(|e| e.to_string())?;
        }
        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            dwFlags: STARTF_USESTDHANDLES,
            hStdInput: child_in,
            hStdOutput: child_out,
            hStdError: child_out,
            ..Default::default()
        };
        let app = HSTRING::from(exe.as_os_str());
        let mut cmdline: Vec<u16> = format!("\"{}\" {FLAG}", exe.display()).encode_utf16().chain(Some(0)).collect();
        let mut pi = PROCESS_INFORMATION::default();
        // no environment block: Windows builds the user's own (their PATH, their %TEMP%)
        let created = CreateProcessWithTokenW(
            primary,
            CREATE_PROCESS_LOGON_FLAGS(0),
            &app,
            Some(PWSTR(cmdline.as_mut_ptr())),
            NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            None,
            None,
            &si,
            &mut pi,
        );
        let _ = CloseHandle(primary);
        let _ = SetHandleInformation(child_in, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        let _ = SetHandleInformation(child_out, HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0));
        // the copy has its own handles now
        drop(in_r);
        drop(out_w);
        created.map_err(|e| e.to_string())?;
        let _ = CloseHandle(pi.hThread);

        let (tx, answers) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(out_r).lines() {
                let Ok(l) = line else { break };
                if tx.send(l).is_err() {
                    break;
                }
            }
        });
        let stdin = std::fs::File::from_raw_handle(in_w.into_raw_handle());
        Ok(Helper { stdin, answers, process: pi.hProcess })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(exe: &str, args: &[&str]) -> Req {
        Req { exe: exe.into(), args: args.iter().map(|s| s.to_string()).collect(), timeout_secs: 20, max_output: 1 << 16, ..Default::default() }
    }

    #[test]
    fn runs_and_captures_output() {
        let r = exec(&req("cmd.exe", &["/d", "/c", "echo hello"]), &|| false);
        assert!(r.success(), "{r:?}");
        assert_eq!(r.stdout.trim(), "hello");
        let r = exec(&req("cmd.exe", &["/d", "/c", "exit 3"]), &|| false);
        assert_eq!(r.code, Some(3));
    }

    #[test]
    fn feeds_stdin_and_caps_output() {
        let mut q = req("findstr.exe", &["x"]);
        q.stdin = Some("axb\nnope\n".into());
        let r = exec(&q, &|| false);
        assert_eq!(r.stdout.trim(), "axb");
        let mut q = req("cmd.exe", &["/d", "/c", "echo 0123456789"]);
        q.max_output = 4;
        assert_eq!(exec(&q, &|| false).stdout, "0123");
    }

    #[test]
    fn stops_and_times_out() {
        let r = exec(&req("ping.exe", &["-n", "30", "127.0.0.1"]), &|| true);
        assert_eq!(r.error.as_deref(), Some("stopped"));
        let mut q = req("ping.exe", &["-n", "30", "127.0.0.1"]);
        q.timeout_secs = 1;
        let r = exec(&q, &|| false);
        assert!(r.timed_out && r.code.is_none());
        assert!(exec(&req("no-such-program-rigsweep.exe", &[]), &|| false).error.is_some());
    }

    /// The real thing, elevated only. Build the release exe, then in an admin terminal (src-tauri):
    /// $env:RIGSWEEP_AS_USER_EXE="target\release\rigsweep.exe"; cargo test --release as_user_live -- --ignored
    #[test]
    #[ignore]
    fn as_user_live() {
        assert!(crate::elevation::is_elevated(), "run it from an admin terminal");
        // S-1-16-8192 is the medium (normal) integrity level, S-1-16-12288 the high (admin) one
        let r = run(&req("whoami.exe", &["/groups"]), &|| false);
        assert!(r.success(), "{r:?}");
        assert!(r.stdout.contains("S-1-16-8192") && !r.stdout.contains("S-1-16-12288"), "{}", r.stdout);
        // the copy is reused, and stdin reaches the program
        let mut q = req("findstr.exe", &["x"]);
        q.stdin = Some("axb\n".into());
        assert_eq!(run(&q, &|| false).stdout.trim(), "axb");
        // Stop ends what it runs
        let t = Instant::now();
        let r = run(&req("ping.exe", &["-n", "30", "127.0.0.1"]), &|| t.elapsed() > Duration::from_secs(2));
        assert_eq!(r.error.as_deref(), Some("stopped"));
        assert!(t.elapsed() < Duration::from_secs(10));
        // and the next request still works, with a new copy
        assert!(run(&req("whoami.exe", &[]), &|| false).success());
    }

    #[test]
    fn requests_round_trip_as_json() {
        let s = serde_json::to_string(&req("git", &["status"])).unwrap();
        let back: Req = serde_json::from_str(&s).unwrap();
        assert_eq!(back.args, vec!["status"]);
        // older fields left out are fine
        let r: Req = serde_json::from_str(r#"{"exe":"git","args":[],"timeout_secs":5,"max_output":10}"#).unwrap();
        assert!(r.stdin.is_none() && r.cwd.is_none());
    }
}
