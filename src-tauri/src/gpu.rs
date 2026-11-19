//! GPU / NPU per process, read the way Task Manager reads it: the `GPU Engine`, `GPU Process Memory` and
//! `GPU Adapter Memory` performance counters (WDDM 2.x, no admin, no vendor SDK). Adapter names and the NPU flag
//! come from D3DKMT, since PDH only knows adapters by LUID.
//!
//! Nothing runs in the background: every number is sampled when `sample()` is called. The PDH query stays open
//! between calls because engine utilization is a rate, and a reopened query has no baseline.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use windows::core::{w, PCWSTR};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW,
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};

#[derive(Serialize, Clone, Debug, Default)]
pub struct EngineUtil {
    /// engine type as the driver names it: "3D", "Compute", "Copy", "VideoDecode", ...
    pub kind: String,
    pub util: f32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct Adapter {
    /// "0x00000000_0x0000c2a1" (high_low), the same id the counters and `GpuProc::adapter` use
    pub luid: String,
    /// "" when D3DKMT doesn't name it
    pub name: String,
    /// busiest engine, % (what Task Manager shows as the adapter's GPU %)
    pub util: f32,
    /// busiest engine of each type
    pub engines: Vec<EngineUtil>,
    pub dedicated: u64,
    pub shared: u64,
    /// a compute-only accelerator (NPU), not a graphics card
    pub npu: bool,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct GpuProc {
    pub pid: u32,
    pub name: String,
    /// % of the busiest engine this process uses, any adapter (Task Manager's "GPU" column)
    pub util: f32,
    /// type of that engine, "" when the process only holds memory
    pub engine: String,
    /// luid of the adapter that engine belongs to
    pub adapter: String,
    /// same as `util` but only NPU adapters (Task Manager's "NPU" column); 0 without an NPU
    pub npu_util: f32,
    pub dedicated: u64,
    pub shared: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct GpuReport {
    pub adapters: Vec<Adapter>,
    pub procs: Vec<GpuProc>,
    pub npu: bool,
}

/// Two calls closer than this (the GPU page and the AI page polling together) share one sample: a rate over a
/// few milliseconds is noise.
const REUSE: Duration = Duration::from_millis(500);
/// Adapters only change on driver install / hot plug.
const ADAPTERS_TTL: Duration = Duration::from_secs(60);

struct Pdh {
    query: PDH_HQUERY,
    engine: PDH_HCOUNTER,
    proc_ded: Option<PDH_HCOUNTER>,
    proc_shared: Option<PDH_HCOUNTER>,
    ad_ded: Option<PDH_HCOUNTER>,
    ad_shared: Option<PDH_HCOUNTER>,
}

// the handles are plain PDH cookies, only ever used under STATE's lock
unsafe impl Send for Pdh {}

impl Pdh {
    /// `None` when this PC has no GPU counters (no WDDM 2 driver, a VM, a server without a GPU).
    fn open() -> Option<Pdh> {
        unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let add = |path: PCWSTR| {
                let mut c = PDH_HCOUNTER::default();
                (PdhAddEnglishCounterW(query, path, 0, &mut c) == 0).then_some(c)
            };
            let Some(engine) = add(w!(r"\GPU Engine(*)\Utilization Percentage")) else {
                PdhCloseQuery(query);
                return None;
            };
            Some(Pdh {
                query,
                engine,
                proc_ded: add(w!(r"\GPU Process Memory(*)\Dedicated Usage")),
                proc_shared: add(w!(r"\GPU Process Memory(*)\Shared Usage")),
                ad_ded: add(w!(r"\GPU Adapter Memory(*)\Dedicated Usage")),
                ad_shared: add(w!(r"\GPU Adapter Memory(*)\Shared Usage")),
            })
        }
    }

    fn collect(&self) -> bool {
        unsafe { PdhCollectQueryData(self.query) == 0 }
    }
}

impl Drop for Pdh {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

/// (instance name, value) for every instance with valid data. Instances come and go with processes, so the
/// array is sized fresh each time.
fn read_array(counter: PDH_HCOUNTER) -> Vec<(String, f64)> {
    unsafe {
        for _ in 0..3 {
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                return Vec::new();
            }
            // items + their name strings share one buffer; u64 keeps it aligned for the f64s
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            let rc = PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items));
            // PDH_MORE_DATA: a process started between the two calls
            if rc == 0x800007D2 {
                continue;
            }
            if rc != 0 {
                return Vec::new();
            }
            return std::slice::from_raw_parts(items, count as usize)
                .iter()
                // 0 = valid, 1 = new data; anything else (first sample of a rate, a dead instance) is not a number
                .filter(|it| it.FmtValue.CStatus <= 1 && !it.szName.is_null())
                .map(|it| (it.szName.to_string().unwrap_or_default(), it.FmtValue.Anonymous.doubleValue))
                .filter(|(_, v)| v.is_finite())
                .collect();
        }
        Vec::new()
    }
}

/// Parsed counter instance, e.g. `pid_1234_luid_0x00000000_0x0000C2A1_phys_0_eng_3_engtype_3D`.
#[derive(Debug, Default, PartialEq)]
struct Inst {
    pid: Option<u32>,
    luid: String,
    phys: u32,
    eng: Option<u32>,
    engtype: String,
}

fn parse_inst(s: &str) -> Option<Inst> {
    // the engine type is free text from the driver and may itself contain '_'
    let (head, engtype) = match s.split_once("_engtype_") {
        Some((h, t)) => (h, t.to_string()),
        None => (s, String::new()),
    };
    let t: Vec<&str> = head.split('_').collect();
    let mut inst = Inst { engtype, ..Default::default() };
    let mut i = 0;
    while i < t.len() {
        match t[i] {
            "pid" => inst.pid = t.get(i + 1)?.parse().ok(),
            "luid" => inst.luid = format!("{}_{}", t.get(i + 1)?, t.get(i + 2)?).to_ascii_lowercase(),
            "phys" => inst.phys = t.get(i + 1)?.parse().unwrap_or(0),
            "eng" => inst.eng = t.get(i + 1)?.parse().ok(),
            _ => {}
        }
        i += 1;
    }
    (!inst.luid.is_empty()).then_some(inst)
}

fn luid_key(high: i32, low: u32) -> String {
    format!("0x{:08x}_0x{:08x}", high as u32, low)
}

#[derive(Clone, Debug)]
struct AdapterInfo {
    luid: String,
    name: String,
    npu: bool,
    /// Microsoft Basic Render Driver and the like: the counters list it, Task Manager doesn't
    software: bool,
}

fn utf16(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n]).trim().to_string()
}

/// Names the NPU drivers ship with: Intel "AI Boost", AMD "NPU Compute Accelerator Device", Qualcomm
/// "Hexagon(TM) NPU". Task Manager files these under "NPU" instead of "GPU".
fn npu_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["npu", "neural", "ai boost", "compute accelerator"].iter().any(|k| n.contains(k))
}

/// Every adapter D3DKMT knows, compute-only ones (NPUs) included; software renderers are flagged so `build` drops them.
fn adapters_info() -> Vec<AdapterInfo> {
    use windows::Wdk::Graphics::Direct3D::{
        D3DKMTCloseAdapter, D3DKMTEnumAdapters3, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTERINFO, D3DKMT_ADAPTERREGISTRYINFO,
        D3DKMT_ADAPTERTYPE, D3DKMT_CLOSEADAPTER, D3DKMT_DRIVER_DESCRIPTION, D3DKMT_ENUMADAPTERS3, D3DKMT_QUERYADAPTERINFO,
        KMTQAITYPE_ADAPTERREGISTRYINFO, KMTQAITYPE_ADAPTERTYPE, KMTQAITYPE_DRIVER_DESCRIPTION, KMTQUERYADAPTERINFOTYPE,
    };
    unsafe fn query<T>(h: u32, ty: KMTQUERYADAPTERINFOTYPE, out: &mut T) -> bool {
        let mut q = D3DKMT_QUERYADAPTERINFO {
            hAdapter: h,
            Type: ty,
            pPrivateDriverData: out as *mut T as *mut _,
            PrivateDriverDataSize: std::mem::size_of::<T>() as u32,
        };
        unsafe { D3DKMTQueryAdapterInfo(&mut q).is_ok() }
    }
    let mut out = Vec::new();
    unsafe {
        let mut e = D3DKMT_ENUMADAPTERS3::default();
        // IncludeComputeOnly: without it NPUs (MCDM, compute-only) are not enumerated at all
        e.Filter.Value = 1;
        if D3DKMTEnumAdapters3(&mut e).is_err() || e.NumAdapters == 0 {
            return out;
        }
        let mut list = vec![D3DKMT_ADAPTERINFO::default(); e.NumAdapters as usize];
        e.pAdapters = list.as_mut_ptr();
        if D3DKMTEnumAdapters3(&mut e).is_err() {
            return out;
        }
        list.truncate(e.NumAdapters as usize);
        for a in &list {
            let mut ty = D3DKMT_ADAPTERTYPE::default();
            let bits = if query(a.hAdapter, KMTQAITYPE_ADAPTERTYPE, &mut ty) { ty.Anonymous.Value } else { 0 };
            // D3DKMT_ADAPTERTYPE: bit 0 RenderSupported, bit 2 SoftwareDevice, bit 11 ComputeOnly
            let (render, software, compute_only) = (bits & 1 != 0, bits & 4 != 0, bits & (1 << 11) != 0);
            {
                let mut reg = D3DKMT_ADAPTERREGISTRYINFO::default();
                let mut name = if query(a.hAdapter, KMTQAITYPE_ADAPTERREGISTRYINFO, &mut reg) { utf16(&reg.AdapterString) } else { String::new() };
                if name.is_empty() {
                    // compute-only drivers often leave the registry string empty
                    let mut d = Box::<D3DKMT_DRIVER_DESCRIPTION>::default();
                    if query(a.hAdapter, KMTQAITYPE_DRIVER_DESCRIPTION, &mut *d) {
                        name = utf16(&d.DriverDescription);
                    }
                }
                let npu = (compute_only && !render) || npu_name(&name);
                out.push(AdapterInfo { luid: luid_key(a.AdapterLuid.HighPart, a.AdapterLuid.LowPart), name, npu, software });
            }
            let _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter });
        }
    }
    out
}

struct State {
    pdh: Option<Pdh>,
    last: Option<(Instant, GpuReport)>,
    adapters: Option<(Instant, Vec<AdapterInfo>)>,
}

static STATE: Mutex<State> = Mutex::new(State { pdh: None, last: None, adapters: None });

/// Closes the PDH query (call when the GPU page closes or the monitor is switched off). The next `sample()`
/// opens a new one.
pub fn reset() {
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    st.pdh = None;
    st.last = None;
}

/// pid -> process name, for every running process (names only: the cheapest sysinfo refresh).
pub fn proc_names() -> HashMap<u32, String> {
    let Ok(mut sys) = System::new() else { return HashMap::new() };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes().iter().map(|(pid, p)| (pid.as_u32(), p.name().to_string_lossy().into_owned())).collect()
}

/// One sample. A PC without GPU counters gets an empty report, never an error. The very first call waits
/// ~250 ms for a second sample so the rates aren't all zero.
pub fn sample() -> GpuReport {
    let mut st = STATE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, r)) = &st.last {
        if t.elapsed() < REUSE {
            return r.clone();
        }
    }
    let fresh = st.pdh.is_none();
    if fresh {
        st.pdh = Pdh::open();
    }
    let Some(pdh) = &st.pdh else { return GpuReport::default() };
    if fresh {
        pdh.collect();
        std::thread::sleep(Duration::from_millis(250));
    }
    if !pdh.collect() {
        return GpuReport::default();
    }
    let engines = read_array(pdh.engine);
    let read = |c: Option<PDH_HCOUNTER>| c.map(read_array).unwrap_or_default();
    let (proc_ded, proc_shared) = (read(pdh.proc_ded), read(pdh.proc_shared));
    let (ad_ded, ad_shared) = (read(pdh.ad_ded), read(pdh.ad_shared));

    if st.adapters.as_ref().is_none_or(|(t, _)| t.elapsed() >= ADAPTERS_TTL) {
        st.adapters = Some((Instant::now(), adapters_info()));
    }
    let infos = st.adapters.as_ref().map(|(_, a)| a.clone()).unwrap_or_default();
    let r = build(&infos, &engines, &proc_ded, &proc_shared, &ad_ded, &ad_shared, &proc_names());
    st.last = Some((Instant::now(), r.clone()));
    r
}

/// Turns the raw counter arrays into the report (separate from the PDH plumbing so it can be tested).
fn build(
    infos: &[AdapterInfo],
    engines: &[(String, f64)],
    proc_ded: &[(String, f64)],
    proc_shared: &[(String, f64)],
    ad_ded: &[(String, f64)],
    ad_shared: &[(String, f64)],
    names: &HashMap<u32, String>,
) -> GpuReport {
    let mut adapters: Vec<Adapter> = infos
        .iter()
        .filter(|i| !i.software)
        .map(|i| Adapter { luid: i.luid.clone(), name: i.name.clone(), npu: i.npu, ..Default::default() })
        .collect();
    fn adapter<'a>(list: &'a mut Vec<Adapter>, luid: &str) -> &'a mut Adapter {
        match list.iter().position(|a| a.luid == luid) {
            Some(i) => &mut list[i],
            None => {
                list.push(Adapter { luid: luid.to_string(), ..Default::default() });
                list.last_mut().unwrap()
            }
        }
    }

    // one engine = (luid, phys, eng); a process's usage of it and the engine's total are both sums
    type Eng = (String, u32, u32);
    let mut per_proc: HashMap<u32, HashMap<Eng, (String, f64)>> = HashMap::new();
    let mut per_engine: HashMap<Eng, (String, f64)> = HashMap::new();
    for (name, v) in engines {
        let Some(i) = parse_inst(name) else { continue };
        let Some(eng) = i.eng else { continue };
        if infos.iter().any(|a| a.software && a.luid == i.luid) {
            continue;
        }
        let key = (i.luid.clone(), i.phys, eng);
        let e = per_engine.entry(key.clone()).or_insert_with(|| (i.engtype.clone(), 0.0));
        e.1 += v;
        if let Some(pid) = i.pid.filter(|&p| p != 0) {
            let e = per_proc.entry(pid).or_default().entry(key).or_insert_with(|| (i.engtype.clone(), 0.0));
            e.1 += v;
        }
    }
    for ((luid, _, _), (kind, v)) in &per_engine {
        let a = adapter(&mut adapters, luid);
        if kind.to_ascii_lowercase().contains("neural") || kind.to_ascii_lowercase().contains("npu") {
            a.npu = true;
        }
        let v = v.min(100.0) as f32;
        a.util = a.util.max(v);
        // some drivers leave a few engines unnamed: they count toward the adapter, not in the breakdown
        if kind.is_empty() {
            continue;
        }
        match a.engines.iter_mut().find(|e| &e.kind == kind) {
            Some(e) => e.util = e.util.max(v),
            None => a.engines.push(EngineUtil { kind: kind.clone(), util: v }),
        }
    }
    for (list, dedicated) in [(ad_ded, true), (ad_shared, false)] {
        for (name, v) in list {
            let Some(i) = parse_inst(name) else { continue };
            if infos.iter().any(|a| a.software && a.luid == i.luid) {
                continue;
            }
            let a = adapter(&mut adapters, &i.luid);
            if dedicated {
                a.dedicated += *v as u64;
            } else {
                a.shared += *v as u64;
            }
        }
    }
    let npu_luids: Vec<String> = adapters.iter().filter(|a| a.npu).map(|a| a.luid.clone()).collect();

    let mut procs: HashMap<u32, GpuProc> = HashMap::new();
    for (pid, engs) in &per_proc {
        let p = procs.entry(*pid).or_insert_with(|| GpuProc { pid: *pid, ..Default::default() });
        for ((luid, _, _), (kind, v)) in engs {
            let v = v.min(100.0) as f32;
            if v > p.util || p.engine.is_empty() && v > 0.0 {
                p.util = v;
                p.engine = kind.clone();
                p.adapter = luid.clone();
            }
            if npu_luids.contains(luid) {
                p.npu_util = p.npu_util.max(v);
            }
        }
        if p.util == 0.0 {
            p.engine.clear();
        }
    }
    for (list, dedicated) in [(proc_ded, true), (proc_shared, false)] {
        for (name, v) in list {
            let Some(i) = parse_inst(name) else { continue };
            let Some(pid) = i.pid.filter(|&p| p != 0) else { continue };
            let p = procs.entry(pid).or_insert_with(|| GpuProc { pid, ..Default::default() });
            if dedicated {
                p.dedicated += *v as u64;
            } else {
                p.shared += *v as u64;
            }
        }
    }
    let mut procs: Vec<GpuProc> = procs
        .into_values()
        .filter(|p| p.util > 0.0 || p.dedicated > 0 || p.shared > 0)
        .filter_map(|mut p| {
            // a pid the counters still list but that has exited
            p.name = names.get(&p.pid)?.clone();
            Some(p)
        })
        .collect();
    procs.sort_by(|a, b| b.util.total_cmp(&a.util).then(b.dedicated.cmp(&a.dedicated)).then(a.pid.cmp(&b.pid)));
    for a in &mut adapters {
        a.engines.sort_by(|x, y| y.util.total_cmp(&x.util).then(x.kind.cmp(&y.kind)));
    }
    let npu = adapters.iter().any(|a| a.npu);
    GpuReport { adapters, procs, npu }
}

#[tauri::command]
pub async fn get_gpu() -> Result<GpuReport, String> {
    tauri::async_runtime::spawn_blocking(sample).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn gpu_reset() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(reset).await.map_err(|e| e.to_string())
}

// ---- AI workload ----------------------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct AiModel {
    pub name: String,
    /// bytes of the model held in VRAM (Ollama's `size_vram`)
    pub vram: u64,
    /// total bytes loaded, VRAM + RAM
    pub size: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct AiRuntime {
    pub pid: u32,
    /// process name, "ollama.exe"
    pub name: String,
    /// friendly runtime name, "Ollama", "ComfyUI", ...
    pub label: String,
    /// loaded models, when the runtime tells (Ollama only for now)
    pub models: Vec<AiModel>,
    /// VRAM this runtime holds: the loaded models' VRAM when known, else the process's dedicated GPU memory
    pub vram: u64,
    /// GPU % of its busiest engine
    pub util: f32,
    pub npu_util: f32,
}

/// Exe names worth a closer look (command line, exe path); everything else is skipped cheaply.
const AI_NAMES: &[&str] = &[
    "ollama.exe", "ollama_llama_server.exe", "lm studio.exe", "lms.exe", "llama-server.exe", "llama-cli.exe", "python.exe",
    "pythonw.exe", "jan.exe", "cortex.exe", "cortex-server.exe", "local-ai.exe",
];

fn is_ai_candidate(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    AI_NAMES.contains(&n.as_str()) || n.starts_with("koboldcpp")
}

/// Friendly label for a known AI runtime, from its exe name, exe path and command line (all lowercase).
/// `gpu_dedicated` lets a plain python.exe count when it clearly holds a model on the GPU.
fn ai_label(name: &str, exe: &str, cmd: &str, gpu_dedicated: u64) -> Option<&'static str> {
    let in_lmstudio = exe.contains("lm studio") || exe.contains(".lmstudio") || exe.contains("lm-studio");
    let in_jan = exe.contains("\\jan\\") || exe.contains("\\jan.") || exe.contains("\\jan-");
    Some(match name {
        "ollama.exe" if cmd.contains(" runner") => "Ollama (model runner)",
        "ollama.exe" => "Ollama",
        "ollama_llama_server.exe" => "Ollama (model runner)",
        "lm studio.exe" => "LM Studio",
        "lms.exe" => "LM Studio CLI",
        "llama-server.exe" | "llama-cli.exe" if in_lmstudio => "LM Studio (llama.cpp)",
        "llama-server.exe" | "llama-cli.exe" if in_jan => "Jan (llama.cpp)",
        "llama-server.exe" => "llama.cpp server",
        "llama-cli.exe" => "llama.cpp",
        "jan.exe" => "Jan",
        "cortex.exe" | "cortex-server.exe" => "Jan (Cortex engine)",
        "local-ai.exe" => "LocalAI",
        n if n.starts_with("koboldcpp") => "KoboldCpp",
        "python.exe" | "pythonw.exe" => {
            if cmd.contains("comfyui") {
                "ComfyUI"
            } else if cmd.contains("text-generation-webui") || cmd.contains("text_generation_webui") {
                "text-generation-webui"
            } else if cmd.contains("stable-diffusion-webui") {
                "Stable Diffusion WebUI"
            } else if cmd.contains("vllm") {
                "vLLM"
            } else if cmd.contains("llama_cpp.server") {
                "llama-cpp-python server"
            } else if cmd.contains("koboldcpp") {
                "KoboldCpp"
            } else if cmd.contains("transformers") {
                "Python (Transformers)"
            } else if cmd.contains("torch") {
                "Python (PyTorch)"
            } else if gpu_dedicated >= 512 << 20 {
                // a script's imports aren't on its command line; half a GB of VRAM is a model, not a UI
                "Python on GPU"
            } else {
                return None;
            }
        }
        _ => return None,
    })
}

/// Loaded models from Ollama's local API. A bare HTTP/1.0 GET (the server closes the connection when done),
/// so no HTTP client and no blocking runtime is needed; 300 ms at most per step.
pub fn ollama_models() -> Vec<AiModel> {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    let t = Duration::from_millis(300);
    let addr: SocketAddr = ([127, 0, 0, 1], 11434).into();
    let Ok(mut s) = TcpStream::connect_timeout(&addr, t) else { return Vec::new() };
    let _ = s.set_read_timeout(Some(t));
    let _ = s.set_write_timeout(Some(t));
    if s.write_all(b"GET /api/ps HTTP/1.0\r\nHost: 127.0.0.1:11434\r\nAccept: application/json\r\n\r\n").is_err() {
        return Vec::new();
    }
    let mut resp = Vec::new();
    // a read timeout still leaves whatever arrived
    let _ = s.take(1 << 20).read_to_end(&mut resp);
    parse_ollama_ps(&String::from_utf8_lossy(&resp))
}

fn parse_ollama_ps(resp: &str) -> Vec<AiModel> {
    let Some((head, body)) = resp.split_once("\r\n\r\n") else { return Vec::new() };
    if !head.split_whitespace().nth(1).is_some_and(|c| c == "200") {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body.trim()) else { return Vec::new() };
    v["models"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|m| AiModel {
                    name: m["name"].as_str().or(m["model"].as_str()).unwrap_or("").to_string(),
                    vram: m["size_vram"].as_u64().unwrap_or(0),
                    size: m["size"].as_u64().unwrap_or(0),
                })
                .filter(|m| !m.name.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Known AI runtimes among the running processes, with their GPU share. `sys` needs names and parents for all
/// processes and command line + exe for the candidates (see `get_ai_workload`). Electron apps (LM Studio, Jan)
/// run many copies of their exe: only the top one is listed.
pub fn ai_runtimes(report: &GpuReport, sys: &System) -> Vec<AiRuntime> {
    let gpu: HashMap<u32, &GpuProc> = report.procs.iter().map(|p| (p.pid, p)).collect();
    let mut out: Vec<AiRuntime> = Vec::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().to_ascii_lowercase();
        if !is_ai_candidate(&name) {
            continue;
        }
        if name == "lm studio.exe" || name == "jan.exe" {
            let parent = p.parent().and_then(|pp| sys.process(pp)).map(|q| q.name().to_string_lossy().to_ascii_lowercase());
            if parent.as_deref() == Some(name.as_str()) {
                continue;
            }
        }
        let exe = p.exe().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ").to_ascii_lowercase();
        let g = gpu.get(&pid.as_u32());
        let ded = g.map_or(0, |g| g.dedicated);
        let Some(label) = ai_label(&name, &exe, &cmd, ded) else { continue };
        out.push(AiRuntime {
            pid: pid.as_u32(),
            name: p.name().to_string_lossy().into_owned(),
            label: label.to_string(),
            models: Vec::new(),
            vram: ded,
            util: g.map_or(0.0, |g| g.util),
            npu_util: g.map_or(0.0, |g| g.npu_util),
        });
    }
    // Ollama: models belong to the server process (the runners do the GPU work, but the user thinks of
    // "Ollama"); without a server entry, to the first runner
    if out.iter().any(|r| r.label.starts_with("Ollama")) {
        let models = ollama_models();
        if !models.is_empty() {
            let i = out.iter().position(|r| r.label == "Ollama").or_else(|| out.iter().position(|r| r.label.starts_with("Ollama")));
            if let Some(i) = i {
                out[i].vram = models.iter().map(|m| m.vram).sum();
                out[i].models = models;
            }
        }
    }
    out.sort_by(|a, b| b.vram.cmp(&a.vram).then(b.util.total_cmp(&a.util)).then(a.pid.cmp(&b.pid)));
    out
}

/// GPU sample + a process list with command lines for the AI candidates only.
pub fn ai_workload() -> Vec<AiRuntime> {
    let report = sample();
    let Ok(mut sys) = System::new() else { return Vec::new() };
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let pids: Vec<Pid> = sys.processes().iter().filter(|(_, p)| is_ai_candidate(&p.name().to_string_lossy())).map(|(pid, _)| *pid).collect();
    if !pids.is_empty() {
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            false,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::OnlyIfNotSet).with_exe(UpdateKind::OnlyIfNotSet),
        );
    }
    ai_runtimes(&report, &sys)
}

#[tauri::command]
pub async fn get_ai_workload() -> Result<Vec<AiRuntime>, String> {
    tauri::async_runtime::spawn_blocking(ai_workload).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_counter_instances() {
        let i = parse_inst("pid_1234_luid_0x00000000_0x0000C2A1_phys_0_eng_3_engtype_3D").unwrap();
        assert_eq!(i, Inst { pid: Some(1234), luid: "0x00000000_0x0000c2a1".into(), phys: 0, eng: Some(3), engtype: "3D".into() });
        let i = parse_inst("pid_8_luid_0x00000000_0x0000C2A1_phys_1_eng_12_engtype_Compute_1").unwrap();
        assert_eq!((i.phys, i.eng, i.engtype.as_str()), (1, Some(12), "Compute_1"));
        let i = parse_inst("luid_0x00000000_0x0000C2A1_phys_0").unwrap();
        assert_eq!((i.pid, i.eng, i.luid.as_str()), (None, None, "0x00000000_0x0000c2a1"));
        assert_eq!(luid_key(0, 0xC2A1), "0x00000000_0x0000c2a1");
        assert!(parse_inst("_Total").is_none());
    }

    #[test]
    fn builds_task_manager_numbers() {
        let s = |n: &str, v: f64| (n.to_string(), v);
        let l = "luid_0x00000000_0x00000001_phys_0";
        let npu = "luid_0x00000000_0x00000002_phys_0";
        let engines = vec![
            s(&format!("pid_10_{l}_eng_0_engtype_3D"), 30.0),
            s(&format!("pid_10_{l}_eng_1_engtype_Copy"), 5.0),
            s(&format!("pid_20_{l}_eng_0_engtype_3D"), 20.0),
            s(&format!("pid_20_{l}_eng_4_engtype_VideoDecode"), 40.0),
            s(&format!("pid_20_{npu}_eng_0_engtype_Compute"), 15.0),
            s("pid_10_luid_0x00000000_0x00000003_phys_0_eng_0_engtype_3D", 90.0),
        ];
        let infos = vec![
            AdapterInfo { luid: "0x00000000_0x00000001".into(), name: "Some GPU".into(), npu: false, software: false },
            AdapterInfo { luid: "0x00000000_0x00000002".into(), name: "Intel(R) AI Boost".into(), npu: true, software: false },
            AdapterInfo { luid: "0x00000000_0x00000003".into(), name: "Microsoft Basic Render Driver".into(), npu: false, software: true },
        ];
        let ded = vec![s(&format!("pid_10_{l}"), 1000.0), s(&format!("pid_30_{l}"), 7.0)];
        let names: HashMap<u32, String> = [(10, "game.exe".to_string()), (20, "ai.exe".to_string())].into();
        let r = build(&infos, &engines, &ded, &[], &[s(l, 5000.0)], &[], &names);
        assert!(r.npu);
        // the software renderer is neither listed nor counted
        assert_eq!(r.adapters.len(), 2);
        let a = &r.adapters[0];
        // the 3D engine is shared: 30 + 20
        assert_eq!((a.util, a.dedicated), (50.0, 5000));
        assert_eq!(a.engines[0].kind, "3D");
        let p20 = r.procs.iter().find(|p| p.pid == 20).unwrap();
        assert_eq!((p20.util, p20.engine.as_str(), p20.npu_util), (40.0, "VideoDecode", 15.0));
        let p10 = r.procs.iter().find(|p| p.pid == 10).unwrap();
        assert_eq!((p10.util, p10.engine.as_str(), p10.dedicated), (30.0, "3D", 1000));
        // pid 30 has exited (no name)
        assert!(r.procs.iter().all(|p| p.pid != 30));
        assert_eq!(r.procs[0].pid, 20);
    }

    #[test]
    fn labels_ai_runtimes() {
        assert_eq!(ai_label("ollama.exe", "", "ollama serve", 0), Some("Ollama"));
        assert_eq!(ai_label("ollama.exe", "", "ollama.exe runner --model x", 0), Some("Ollama (model runner)"));
        assert_eq!(ai_label("python.exe", "", "python main.py --listen", 0), None);
        assert_eq!(ai_label("python.exe", "", "python d:\\comfyui\\main.py", 0), Some("ComfyUI"));
        assert_eq!(ai_label("python.exe", "", "python train.py", 1 << 30), Some("Python on GPU"));
        assert_eq!(ai_label("llama-server.exe", "c:\\users\\x\\.lmstudio\\bin\\llama-server.exe", "", 0), Some("LM Studio (llama.cpp)"));
        assert_eq!(ai_label("koboldcpp_cu12.exe", "", "", 0), Some("KoboldCpp"));
        assert!(npu_name("NPU Compute Accelerator Device") && npu_name("Intel(R) AI Boost") && !npu_name("NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn parses_ollama_ps() {
        let resp = "HTTP/1.0 200 OK\r\nContent-Type: application/json\r\n\r\n\
            {\"models\":[{\"name\":\"llama3:8b\",\"model\":\"llama3:8b\",\"size\":6000,\"size_vram\":5000}]}";
        assert_eq!(parse_ollama_ps(resp), vec![AiModel { name: "llama3:8b".into(), vram: 5000, size: 6000 }]);
        assert!(parse_ollama_ps("HTTP/1.0 404 Not Found\r\n\r\n{}").is_empty());
    }

    /// Two samples on this PC; a PC without GPU counters gets an empty report, not an error.
    /// `cargo test gpu_live -- --nocapture` prints what it saw.
    #[test]
    fn gpu_live() {
        let a = sample();
        std::thread::sleep(REUSE);
        let b = sample();
        println!("adapters {:#?}\ntop procs {:#?}\nnpu {}", b.adapters, b.procs.iter().take(5).collect::<Vec<_>>(), b.npu);
        assert!(b.procs.iter().all(|p| (0.0..=100.0).contains(&p.util)));
        let _ = a;
        println!("ai {:#?}", ai_workload());
        reset();
    }
}
