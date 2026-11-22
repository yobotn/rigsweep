//! Console windows that flash open and close. Something starts `cmd`, PowerShell or another
//! console program with a visible window, it closes a moment later, and nobody can tell what ran.
//!
//! Two sources, both live and both cheap:
//! - an ETW kernel session (system logger mode, process events only) sees every process start
//!   and exit with its full command line and parent, even ones that live for a few milliseconds;
//! - a WinEvent hook sees each console window being shown (a conhost window, or a new Windows
//!   Terminal window when that is the default terminal).
//!
//! A console program whose window was shown and that ended within a minute is a "flash". It is
//! saved with what started it (parent chain, the scheduled task or service behind it) and what
//! ran inside it (its child processes). Hidden consoles (`CREATE_NO_WINDOW`, the git calls an
//! editor makes, ...) never show a window, so they never land in the list.
//!
//! The ETW session needs admin; RigSweep runs elevated. Nothing here changes the system.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

const SESSION: &str = "RigSweep Console Watch";
/// a console that stays open longer than this is something the user opened, not a flash
const MAX_FLASH_MS: u64 = 60_000;
/// ended processes stay known this long, so late window events and parent chains still resolve
const KEEP_EXITED_MS: u64 = 120_000;
const SHELLS: &[&str] = &["cmd.exe", "powershell.exe", "pwsh.exe", "conhost.exe", "wscript.exe", "cscript.exe", "openconsole.exe"];

#[derive(Clone, Debug)]
struct Proc {
    pid: u32,
    ppid: u32,
    name: String,
    cmd: String,
    /// unix ms; 0 = it was already running when the watch started
    start: u64,
    end: Option<u64>,
    exit: Option<i32>,
    shown: bool,
    recorded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum WinKind {
    Console,
    Terminal,
}

#[derive(Clone, Debug)]
struct Show {
    pid: u32,
    t: u64,
    kind: WinKind,
    tries: u8,
}

#[derive(Default)]
struct Tracker {
    procs: HashMap<u32, Proc>,
    shows: Vec<Show>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Node {
    pub pid: u32,
    pub name: String,
    pub cmd: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Flash {
    /// unix ms the console program started
    pub t: u64,
    /// how long it ran
    pub ms: u64,
    pub pid: u32,
    pub name: String,
    pub cmd: String,
    pub exit: Option<i32>,
    /// parent first, then its parent, ...
    pub chain: Vec<Node>,
    /// programs that ran inside it, in start order
    pub children: Vec<Node>,
    /// task | service | explorer | app | unknown
    pub source: String,
    /// the service name or the app that started it
    pub source_name: String,
    /// scheduled task path when one matched, e.g. `\Vendor\Updater`
    pub task: Option<String>,
    /// a task outside `\Microsoft\` that can be turned off from Startup
    pub task_user: bool,
}

static STATE: Mutex<Option<Tracker>> = Mutex::new(None);
static ON: AtomicBool = AtomicBool::new(false);
static WANT: AtomicBool = AtomicBool::new(false);
/// a session thread exists (starting, running or stopping): never two at once
static RUNNING: AtomicBool = AtomicBool::new(false);
static SINCE: AtomicU64 = AtomicU64::new(0);
static ERROR: Mutex<Option<String>> = Mutex::new(None);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn with_state<T>(f: impl FnOnce(&mut Tracker) -> T) -> T {
    let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Tracker::default))
}

#[derive(Serialize)]
pub struct Status {
    pub on: bool,
    pub since: u64,
    pub error: Option<String>,
}

pub fn status() -> Status {
    Status {
        on: ON.load(Relaxed),
        since: SINCE.load(Relaxed),
        error: ERROR.lock().ok().and_then(|e| e.clone()),
    }
}

// ---------- parsing the kernel process event ----------

#[derive(Debug, PartialEq)]
struct Ev {
    pid: u32,
    ppid: u32,
    exit: i32,
    image: String,
    cmd: String,
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// Process_V3/V4_TypeGroup1 (MSNT_SystemTrace): UniqueProcessKey (ptr), ProcessId, ParentId,
/// SessionId, ExitStatus, DirectoryTableBase (ptr), Flags (V4+), UserSID (TOKEN_USER + SID),
/// ImageFileName (ANSI), CommandLine (UTF-16), ...
fn parse(d: &[u8], version: u8, ptr: usize) -> Option<Ev> {
    if version < 3 {
        return None;
    }
    let mut o = ptr;
    let pid = u32_at(d, o)?;
    let ppid = u32_at(d, o + 4)?;
    let exit = u32_at(d, o + 12).unwrap_or(0) as i32;
    o += 16 + ptr;
    if version >= 4 {
        o += 4;
    }
    let mut ev = Ev { pid, ppid, exit, image: String::new(), cmd: String::new() };
    // the SID: a NULL TOKEN_USER is a lone 0 ULONG
    if u32_at(d, o) == Some(0) {
        o += 4;
    } else {
        let Some(&n) = d.get(o + 2 * ptr + 1) else { return Some(ev) };
        o += 2 * ptr + 8 + 4 * n as usize;
    }
    let Some(rest) = d.get(o..) else { return Some(ev) };
    let Some(z) = rest.iter().position(|&b| b == 0) else { return Some(ev) };
    ev.image = rest[..z].iter().map(|&b| b as char).collect();
    o += z + 1;
    let w: Vec<u16> = d
        .get(o..)
        .unwrap_or(&[])
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    ev.cmd = String::from_utf16_lossy(&w).trim().to_string();
    Some(ev)
}

/// `\??\C:\WINDOWS\system32\conhost.exe 0xffffffff` → `conhost.exe`; the kernel's
/// ImageFileName is cut to 15 characters, so the command line wins when there is one.
fn display_name(image: &str, cmd: &str) -> String {
    let exe = crate::icons::exe_from_command(cmd.trim_start_matches(r"\??\"));
    let base = exe.rsplit(['\\', '/']).next().unwrap_or("").trim();
    let n = if base.to_ascii_lowercase().ends_with(".exe") || (!base.is_empty() && image.is_empty()) { base } else { image };
    n.to_ascii_lowercase()
}

fn on_start(ev: Ev, t: u64, rundown: bool) {
    with_state(|st| {
        if rundown && st.procs.contains_key(&ev.pid) {
            return;
        }
        let name = display_name(&ev.image, &ev.cmd);
        st.procs.insert(
            ev.pid,
            Proc { pid: ev.pid, ppid: ev.ppid, name, cmd: ev.cmd, start: if rundown { 0 } else { t }, end: None, exit: None, shown: false, recorded: false },
        );
    });
}

fn on_end(pid: u32, exit: i32, t: u64) {
    with_state(|st| {
        if let Some(p) = st.procs.get_mut(&pid) {
            if p.end.is_none() && p.start <= t {
                p.end = Some(t);
                p.exit = Some(exit);
            }
        }
    });
}

// ---------- the ETW session ----------

mod etw {
    use super::*;
    use windows::core::{PCWSTR, PWSTR};
    use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_SUCCESS};
    use windows::Win32::System::Diagnostics::Etw::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// EVENT_TRACE_PROPERTIES followed by room for the session name, 8-byte aligned
    fn props() -> Vec<u64> {
        let base = std::mem::size_of::<EVENT_TRACE_PROPERTIES>();
        let total = base + 1024;
        let mut buf = vec![0u64; total.div_ceil(8)];
        let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        unsafe {
            (*p).Wnode.BufferSize = total as u32;
            (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*p).Wnode.ClientContext = 1; // QPC; ProcessTrace hands out FILETIMEs
            (*p).LogFileMode = EVENT_TRACE_REAL_TIME_MODE | EVENT_TRACE_SYSTEM_LOGGER_MODE;
            (*p).FlushTimer = 1;
            (*p).EnableFlags = EVENT_TRACE_FLAG_PROCESS;
            (*p).LoggerNameOffset = base as u32;
        }
        buf
    }

    pub fn stop() {
        let name = wide(SESSION);
        let mut buf = props();
        unsafe {
            let _ = ControlTraceW(CONTROLTRACE_HANDLE { Value: 0 }, PCWSTR(name.as_ptr()), buf.as_mut_ptr() as *mut _, EVENT_TRACE_CONTROL_STOP);
        }
    }

    fn ms_of(filetime: i64) -> u64 {
        (filetime.saturating_sub(116_444_736_000_000_000) / 10_000).max(0) as u64
    }

    unsafe extern "system" fn on_event(rec: *mut EVENT_RECORD) {
        let Some(r) = (unsafe { rec.as_ref() }) else { return };
        if r.EventHeader.ProviderId != ProcessGuid || r.UserData.is_null() {
            return;
        }
        let op = r.EventHeader.EventDescriptor.Opcode;
        // 1 start, 2 end, 3 already running when the session started; 4 (rundown at stop) is ignored
        if !(1..=3).contains(&op) {
            return;
        }
        let data = unsafe { std::slice::from_raw_parts(r.UserData as *const u8, r.UserDataLength as usize) };
        let ptr = if r.EventHeader.Flags as u32 & EVENT_HEADER_FLAG_64_BIT_HEADER != 0 { 8 } else { 4 };
        let Some(ev) = parse(data, r.EventHeader.EventDescriptor.Version, ptr) else { return };
        let t = ms_of(r.EventHeader.TimeStamp);
        match op {
            1 => on_start(ev, t, false),
            3 => on_start(ev, t, true),
            _ => on_end(ev.pid, ev.exit, t),
        }
    }

    /// Runs the session on this thread until `stop()`; Err(text) when it can't start.
    pub fn run() -> Result<(), String> {
        let name = wide(SESSION);
        let mut buf = props();
        let mut handle = CONTROLTRACE_HANDLE::default();
        let mut rc = unsafe { StartTraceW(&mut handle, PCWSTR(name.as_ptr()), buf.as_mut_ptr() as *mut _) };
        if rc == ERROR_ALREADY_EXISTS {
            // left over from a RigSweep that didn't get to stop it
            stop();
            buf = props();
            rc = unsafe { StartTraceW(&mut handle, PCWSTR(name.as_ptr()), buf.as_mut_ptr() as *mut _) };
        }
        if rc == ERROR_ACCESS_DENIED {
            return Err("admin".into());
        }
        if rc != ERROR_SUCCESS {
            return Err(format!("StartTrace failed ({})", rc.0));
        }
        let mut lname = name.clone();
        let mut log = EVENT_TRACE_LOGFILEW::default();
        log.LoggerName = PWSTR(lname.as_mut_ptr());
        log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        log.Anonymous2.EventRecordCallback = Some(on_event);
        let th = unsafe { OpenTraceW(&mut log) };
        if th.Value == u64::MAX {
            stop();
            return Err("OpenTrace failed".into());
        }
        ON.store(true, Relaxed);
        SINCE.store(now_ms(), Relaxed);
        let _ = unsafe { ProcessTrace(&[th], None, None) };
        unsafe {
            let _ = CloseTrace(th);
        }
        Ok(())
    }
}

// ---------- console windows being shown ----------

mod hook {
    use super::*;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Accessibility::{SetWinEventHook, HWINEVENTHOOK};
    use windows::Win32::UI::WindowsAndMessaging::*;

    unsafe extern "system" fn on_show(_h: HWINEVENTHOOK, event: u32, hwnd: HWND, idobject: i32, idchild: i32, _thread: u32, _time: u32) {
        if !ON.load(Relaxed) || event != EVENT_OBJECT_SHOW || idobject != OBJID_WINDOW.0 || idchild != 0 {
            return;
        }
        unsafe {
            if GetAncestor(hwnd, GA_ROOT) != hwnd {
                return;
            }
            let mut buf = [0u16; 64];
            let n = GetClassNameW(hwnd, &mut buf).max(0) as usize;
            let kind = match String::from_utf16_lossy(&buf[..n]).as_str() {
                "ConsoleWindowClass" => WinKind::Console,
                "CASCADIA_HOSTING_WINDOW_CLASS" => WinKind::Terminal,
                _ => return,
            };
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let t = now_ms();
            with_state(|st| st.shows.push(Show { pid, t, kind, tries: 0 }));
        }
    }

    pub fn spawn() {
        std::thread::spawn(|| unsafe {
            let h = SetWinEventHook(EVENT_OBJECT_SHOW, EVENT_OBJECT_SHOW, None, Some(on_show), 0, 0, WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS);
            if h.is_invalid() {
                return;
            }
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }
}

// ---------- turning window events and processes into flashes ----------

/// Which console program a shown window belongs to, once the process events have caught up.
fn resolve(st: &Tracker, s: &Show) -> Option<u32> {
    match s.kind {
        WinKind::Console => {
            let p = st.procs.get(&s.pid)?;
            // the window belongs to conhost (or reports its client); conhost's parent is the client
            if p.name == "conhost.exe" {
                st.procs.contains_key(&p.ppid).then_some(p.ppid)
            } else {
                Some(p.pid)
            }
        }
        // a new Windows Terminal window: the console that just got a (non-headless) conhost
        // ...unless you just opened Windows Terminal yourself (not the -Embedding hand-off)
        WinKind::Terminal if st.procs.values().any(|w| {
            w.name == "windowsterminal.exe" && w.start + 5000 >= s.t && w.start <= s.t + 500 && !w.cmd.to_ascii_lowercase().contains("-embedding")
        }) => None,
        WinKind::Terminal => st
            .procs
            .values()
            .filter(|c| c.name == "conhost.exe" && c.start + 5000 >= s.t && c.start <= s.t + 500 && !c.cmd.to_ascii_lowercase().contains("headless"))
            .max_by_key(|c| c.start)
            .and_then(|c| st.procs.contains_key(&c.ppid).then_some(c.ppid)),
    }
}

fn node(p: &Proc) -> Node {
    Node { pid: p.pid, name: p.name.clone(), cmd: p.cmd.chars().take(2000).collect() }
}

fn chain_of(st: &Tracker, p: &Proc) -> Vec<Node> {
    let mut out = vec![];
    let (mut cur, mut child_start) = (p.ppid, p.start);
    for _ in 0..8 {
        let Some(q) = st.procs.get(&cur) else { break };
        // a parent can't be younger than its child: that pid was reused
        if q.start > child_start && child_start != 0 {
            break;
        }
        out.push(node(q));
        if q.ppid == 0 || q.ppid == q.pid || q.pid <= 4 {
            break;
        }
        child_start = q.start;
        cur = q.ppid;
    }
    out
}

fn descends(st: &Tracker, c: &Proc, root: u32) -> bool {
    let mut cur = c.ppid;
    for _ in 0..10 {
        if cur == root {
            return true;
        }
        match st.procs.get(&cur) {
            Some(q) if q.ppid != cur => cur = q.ppid,
            _ => return false,
        }
    }
    false
}

fn children_of(st: &Tracker, p: &Proc) -> Vec<Node> {
    let mut v: Vec<&Proc> = st.procs.values().filter(|c| c.start >= p.start && c.name != "conhost.exe" && descends(st, c, p.pid)).collect();
    v.sort_by_key(|c| c.start);
    v.into_iter().take(20).map(node).collect()
}

/// `svchost.exe -k netsvcs -p -s Schedule` → `Schedule`
fn svc_name(cmd: &str) -> Option<String> {
    let mut it = cmd.split_whitespace();
    while let Some(a) = it.next() {
        if a.eq_ignore_ascii_case("-s") {
            return it.next().map(str::to_string);
        }
    }
    None
}

/// (source, name, the process the scheduler or service manager launched)
fn classify(p: &Proc, chain: &[Node]) -> (String, String, Option<Node>) {
    let me = node(p);
    for (i, n) in chain.iter().enumerate() {
        let launched = if i == 0 { me.clone() } else { chain[i - 1].clone() };
        let low = n.cmd.to_ascii_lowercase();
        match n.name.as_str() {
            "svchost.exe" if low.contains("schedule") => return ("task".into(), String::new(), Some(launched)),
            "taskhostw.exe" | "taskeng.exe" => return ("task".into(), String::new(), Some(launched)),
            "services.exe" => {
                let name = svc_name(&launched.cmd).unwrap_or_else(|| launched.name.clone());
                return ("service".into(), name, None);
            }
            "svchost.exe" => return ("service".into(), svc_name(&n.cmd).unwrap_or_else(|| n.name.clone()), None),
            "explorer.exe" => return ("explorer".into(), "explorer.exe".into(), None),
            _ => {}
        }
    }
    match chain.iter().find(|n| !SHELLS.contains(&n.name.as_str())) {
        Some(n) => ("app".into(), n.name.clone(), None),
        None => ("unknown".into(), String::new(), None),
    }
}

fn build(st: &Tracker, p: &Proc, ms: u64) -> (Flash, Option<Node>) {
    let chain = chain_of(st, p);
    let (source, source_name, launched) = classify(p, &chain);
    let f = Flash {
        t: p.start,
        ms,
        pid: p.pid,
        name: p.name.clone(),
        cmd: p.cmd.chars().take(4000).collect(),
        exit: p.exit,
        children: children_of(st, p),
        chain,
        source,
        source_name,
        task: None,
        task_user: false,
    };
    (f, launched)
}

// ---------- which scheduled task ran it ----------

#[derive(Clone, Debug)]
struct TaskDef {
    path: String,
    command: String,
    args: String,
}

static TASKS: Mutex<Option<(Instant, Vec<TaskDef>)>> = Mutex::new(None);

fn xml_tag(s: &str, tag: &str) -> String {
    let (open, close) = (format!("<{tag}>"), format!("</{tag}>"));
    let Some(a) = s.find(&open) else { return String::new() };
    let rest = &s[a + open.len()..];
    let v = rest.find(&close).map(|b| &rest[..b]).unwrap_or("");
    v.replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&").trim().to_string()
}

fn read_task_xml(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let w: Vec<u16> = bytes[2..].chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&w)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn scan_tasks(dir: &std::path::Path, rel: &str, out: &mut Vec<TaskDef>, depth: u8) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let path = format!(r"{rel}\{name}");
        match e.file_type() {
            Ok(ft) if ft.is_dir() && depth < 8 => scan_tasks(&e.path(), &path, out, depth + 1),
            Ok(ft) if ft.is_file() => {
                let Ok(b) = std::fs::read(e.path()) else { continue };
                let x = read_task_xml(&b);
                let command = xml_tag(&x, "Command");
                if !command.is_empty() {
                    out.push(TaskDef { path, command, args: xml_tag(&x, "Arguments") });
                }
            }
            _ => {}
        }
    }
}

fn tasks() -> Vec<TaskDef> {
    let mut g = TASKS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, v)) = g.as_ref() {
        if at.elapsed() < Duration::from_secs(300) {
            return v.clone();
        }
    }
    let root = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into())).join(r"System32\Tasks");
    let mut v = vec![];
    scan_tasks(&root, "", &mut v, 0);
    *g = Some((Instant::now(), v.clone()));
    v
}

fn match_task<'a>(defs: &'a [TaskDef], launched: &Node) -> Option<&'a TaskDef> {
    let cmd = launched.cmd.to_ascii_lowercase();
    defs.iter()
        .filter_map(|d| {
            let exe = crate::icons::exe_from_command(&d.command);
            let base = exe.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
            let base = if base.ends_with(".exe") || base.contains('.') { base } else { format!("{base}.exe") };
            if base != launched.name {
                return None;
            }
            let args = crate::util::expand_env(&d.args).to_ascii_lowercase();
            let score = if args.is_empty() { 1 } else if cmd.contains(args.trim()) { 3 } else { 0 };
            Some((score, d))
        })
        .max_by_key(|(s, _)| *s)
        .map(|(_, d)| d)
}

// ---------- the loop ----------

fn tick(app: &AppHandle) {
    let now = now_ms();
    let found: Vec<(Flash, Option<Node>)> = with_state(|st| {
        let shows = std::mem::take(&mut st.shows);
        for mut s in shows {
            match resolve(st, &s) {
                Some(pid) => {
                    if let Some(p) = st.procs.get_mut(&pid) {
                        p.shown = true;
                    }
                }
                None if s.tries < 10 => {
                    s.tries += 1;
                    st.shows.push(s);
                }
                None => {}
            }
        }
        let due: Vec<u32> = st
            .procs
            .values()
            .filter(|p| p.shown && !p.recorded && (p.end.is_some() || now.saturating_sub(p.start) > MAX_FLASH_MS))
            .map(|p| p.pid)
            .collect();
        let mut out = vec![];
        for pid in due {
            let Some(p) = st.procs.get(&pid).cloned() else { continue };
            if let Some(q) = st.procs.get_mut(&pid) {
                q.recorded = true;
            }
            if let Some(end) = p.end {
                let ms = end.saturating_sub(p.start);
                if p.start > 0 && ms <= MAX_FLASH_MS {
                    out.push(build(st, &p, ms));
                }
            }
        }
        st.procs.retain(|_, p| p.end.is_none_or(|e| now.saturating_sub(e) < KEEP_EXITED_MS));
        out
    });
    for (mut f, launched) in found {
        if f.source == "task" {
            if let Some(l) = launched {
                let defs = tasks();
                if let Some(d) = match_task(&defs, &l) {
                    f.task_user = !d.path.to_ascii_lowercase().starts_with(r"\microsoft\");
                    f.task = Some(d.path.clone());
                }
            }
        }
        if let Ok(db) = crate::db::Db::open() {
            if let Ok(json) = serde_json::to_string(&f) {
                let _ = db.add_flash(f.t as i64, &json);
            }
        }
        let _ = app.emit("flash", &f);
    }
}

/// Everything already running, so the first flashes have their parents.
fn seed() {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let Ok(mut sys) = System::new() else { return };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always));
    with_state(|st| {
        for (pid, p) in sys.processes() {
            let cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
            let name = p.name().to_string_lossy().to_ascii_lowercase();
            st.procs.entry(pid.as_u32()).or_insert(Proc {
                pid: pid.as_u32(),
                ppid: p.parent().map(|x| x.as_u32()).unwrap_or(0),
                name,
                cmd,
                start: 0,
                end: None,
                exit: None,
                shown: false,
                recorded: false,
            });
        }
    });
}

fn start_session() {
    std::thread::spawn(|| {
        seed();
        if let Ok(mut e) = ERROR.lock() {
            *e = None;
        }
        let r = etw::run();
        ON.store(false, Relaxed);
        with_state(|st| {
            st.procs.clear();
            st.shows.clear();
        });
        let failed = r.is_err();
        if let Err(e) = r {
            if let Ok(mut g) = ERROR.lock() {
                *g = Some(e);
            }
        }
        RUNNING.store(false, Relaxed);
        // switched off and back on while it was stopping: start again (not after a failure)
        if !failed && WANT.load(Relaxed) && !RUNNING.swap(true, Relaxed) {
            start_session();
        }
    });
}

/// Called once at startup with the saved switch.
pub fn spawn(app: AppHandle, on: bool) {
    hook::spawn();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_millis(700));
        if ON.load(Relaxed) {
            tick(&app);
        }
    });
    set_enabled(on);
}

pub fn set_enabled(on: bool) {
    WANT.store(on, Relaxed);
    if on {
        if !RUNNING.swap(true, Relaxed) {
            start_session();
        }
    } else if RUNNING.load(Relaxed) {
        etw::stop();
    }
}

/// On exit: a real-time session outlives its process otherwise.
pub fn shutdown() {
    WANT.store(false, Relaxed);
    if RUNNING.load(Relaxed) {
        etw::stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proc(pid: u32, ppid: u32, name: &str, cmd: &str, start: u64) -> Proc {
        Proc { pid, ppid, name: name.into(), cmd: cmd.into(), start, end: None, exit: None, shown: false, recorded: false }
    }

    fn event_v4(pid: u32, ppid: u32, image: &str, cmd: &str, sid: bool) -> Vec<u8> {
        let mut d = vec![0u8; 8];
        for v in [pid, ppid, 1, 0] {
            d.extend(v.to_le_bytes());
        }
        d.extend([0u8; 8]); // DirectoryTableBase
        d.extend([0u8; 4]); // Flags
        if sid {
            d.extend([1u8; 16]); // TOKEN_USER
            d.extend([1u8, 2, 0, 0, 0, 0, 0, 5]); // SID: rev 1, 2 sub-authorities
            d.extend([0u8; 8]);
        } else {
            d.extend([0u8; 4]);
        }
        d.extend(image.as_bytes());
        d.push(0);
        for c in cmd.encode_utf16() {
            d.extend(c.to_le_bytes());
        }
        d.extend([0, 0]);
        d
    }

    #[test]
    fn parses_process_start() {
        let d = event_v4(1234, 99, "cmd.exe", r#"C:\WINDOWS\system32\cmd.exe /c "echo hi""#, true);
        let ev = parse(&d, 4, 8).unwrap();
        assert_eq!((ev.pid, ev.ppid, ev.image.as_str()), (1234, 99, "cmd.exe"));
        assert_eq!(ev.cmd, r#"C:\WINDOWS\system32\cmd.exe /c "echo hi""#);
        let d = event_v4(7, 8, "conhost.exe", r"\??\C:\WINDOWS\system32\conhost.exe 0xffffffff -ForceV1", false);
        let ev = parse(&d, 4, 8).unwrap();
        assert_eq!(display_name(&ev.image, &ev.cmd), "conhost.exe");
        assert!(parse(&d, 2, 8).is_none());
        assert_eq!(parse(&d[..20], 4, 8).unwrap().pid, 7); // truncated: ids still come through
    }

    #[test]
    fn long_names_come_from_the_command_line() {
        assert_eq!(display_name("WindowsTermina", r#""C:\Program Files\WindowsApps\x\WindowsTerminal.exe" -Embedding"#), "windowsterminal.exe");
        assert_eq!(display_name("System", ""), "system");
    }

    #[test]
    fn a_task_flash_is_traced_back_to_its_task() {
        let mut st = Tracker::default();
        st.procs.insert(900, proc(900, 4, "svchost.exe", "C:\\WINDOWS\\system32\\svchost.exe -k netsvcs -p -s Schedule", 0));
        st.procs.insert(1000, proc(1000, 900, "cmd.exe", r#"C:\WINDOWS\system32\cmd.exe /c "C:\Vendor\sync.bat""#, 10_000));
        st.procs.insert(1001, proc(1001, 1000, "conhost.exe", r"\??\C:\WINDOWS\system32\conhost.exe 0xffffffff", 10_005));
        st.procs.insert(1002, proc(1002, 1000, "curl.exe", "curl https://example.com", 10_050));
        // the console window shows: its owner is conhost, resolved to cmd
        assert_eq!(resolve(&st, &Show { pid: 1001, t: 10_010, kind: WinKind::Console, tries: 0 }), Some(1000));
        assert_eq!(resolve(&st, &Show { pid: 1001, t: 10_200, kind: WinKind::Terminal, tries: 0 }), Some(1000));
        let p = st.procs[&1000].clone();
        let (f, launched) = build(&st, &p, 300);
        assert_eq!(f.source, "task");
        assert_eq!(f.chain[0].name, "svchost.exe");
        assert_eq!(f.children.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["curl.exe"]);
        let defs = vec![
            TaskDef { path: r"\Other".into(), command: "cmd.exe".into(), args: "/c other.bat".into() },
            TaskDef { path: r"\Vendor\Sync".into(), command: r"%windir%\system32\cmd.exe".into(), args: r#"/c "C:\Vendor\sync.bat""#.into() },
        ];
        assert_eq!(match_task(&defs, &launched.unwrap()).unwrap().path, r"\Vendor\Sync");
    }

    #[test]
    fn services_and_apps() {
        let mut st = Tracker::default();
        st.procs.insert(600, proc(600, 4, "services.exe", "", 0));
        st.procs.insert(700, proc(700, 600, "updater.exe", r#""C:\Program Files\Vendor\updater.exe" /service"#, 1));
        st.procs.insert(800, proc(800, 700, "cmd.exe", "cmd /c x", 5));
        let (f, _) = build(&st, &st.procs[&800].clone(), 50);
        assert_eq!((f.source.as_str(), f.source_name.as_str()), ("service", "updater.exe"));
        st.procs.insert(50, proc(50, 1, "node.exe", "node build.js", 1));
        st.procs.insert(51, proc(51, 50, "cmd.exe", "cmd /c a", 2));
        st.procs.insert(52, proc(52, 51, "powershell.exe", "powershell -c b", 3));
        let (f, _) = build(&st, &st.procs[&52].clone(), 50);
        assert_eq!((f.source.as_str(), f.source_name.as_str()), ("app", "node.exe"));
    }

    #[test]
    fn a_terminal_you_opened_is_not_a_flash() {
        let mut st = Tracker::default();
        st.procs.insert(70, proc(70, 1, "node.exe", "node", 0));
        st.procs.insert(71, proc(71, 70, "conhost.exe", r"\??\C:\WINDOWS\system32\conhost.exe 0xffffffff -ForceV1", 9_000));
        let show = Show { pid: 99, t: 10_000, kind: WinKind::Terminal, tries: 0 };
        assert_eq!(resolve(&st, &show), Some(70));
        st.procs.insert(99, proc(99, 5, "windowsterminal.exe", r#""C:\Program Files\WindowsApps\WT\WindowsTerminal.exe""#, 9_800));
        assert_eq!(resolve(&st, &show), None);
        st.procs.get_mut(&99).unwrap().cmd.push_str(" -Embedding");
        assert_eq!(resolve(&st, &show), Some(70));
    }

    #[test]
    fn reused_parent_pid_is_not_followed() {
        let mut st = Tracker::default();
        st.procs.insert(10, proc(10, 0, "newer.exe", "", 50_000));
        st.procs.insert(20, proc(20, 10, "cmd.exe", "cmd", 40_000));
        assert!(chain_of(&st, &st.procs[&20].clone()).is_empty());
    }
}
