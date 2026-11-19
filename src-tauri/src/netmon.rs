//! Network: traffic per interface and connections per process. All sampled on call, nothing in the background.
//!
//! Bytes per second *per process* are not here on purpose: Windows only exposes them through ETW (the
//! Kernel-Network provider, what Resource Monitor uses), which needs a running trace session and admin. The
//! interface counters below are free and need neither.

use serde::Serialize;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Mutex;
use std::time::Instant;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

// ---- interface totals --------------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
pub struct NetIface {
    /// "Wi-Fi", "Ethernet 2"
    pub name: String,
    /// adapter description, "Intel(R) Wi-Fi 6E AX211 160MHz"
    pub description: String,
    /// "ethernet", "wifi", "other"
    pub kind: String,
    /// a physical adapter; virtual switches (Hyper-V, WSL) repeat traffic a physical one also carries
    pub hardware: bool,
    /// link speed, bits/s
    pub link_speed: u64,
    /// bytes/s since the previous call, 0 on the first
    pub in_per_sec: u64,
    pub out_per_sec: u64,
    /// bytes since the adapter came up
    pub in_total: u64,
    pub out_total: u64,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct NetTotals {
    pub ifaces: Vec<NetIface>,
    /// sum over hardware interfaces only, so traffic through a virtual switch isn't counted twice
    pub in_per_sec: u64,
    pub out_per_sec: u64,
}

/// (when, luid -> (in octets, out octets)) of the previous call
static LAST_IF: Mutex<Option<(Instant, HashMap<u64, (u64, u64)>)>> = Mutex::new(None);

/// IF_TYPE_SOFTWARE_LOOPBACK, IF_TYPE_TUNNEL
const SKIP_TYPES: [u32; 2] = [24, 131];

pub fn net_totals() -> NetTotals {
    use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
    use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    let mut rows: Vec<(u64, NetIface)> = Vec::new();
    unsafe {
        let mut t: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut t).is_err() || t.is_null() {
            return NetTotals::default();
        }
        let n = (*t).NumEntries as usize;
        let list = std::slice::from_raw_parts((*t).Table.as_ptr(), n);
        for r in list {
            let bits = r.InterfaceAndOperStatusFlags._bitfield;
            // bit 0 HardwareInterface, bit 1 FilterInterface (an LWF layer on top of another row: same bytes again)
            let (hardware, filter) = (bits & 1 != 0, bits & 2 != 0);
            if filter || SKIP_TYPES.contains(&r.Type) || r.OperStatus != IfOperStatusUp {
                continue;
            }
            let name = utf16(&r.Alias);
            // the hidden "...-QoS Packet Scheduler-0000"-style rows that aren't flagged as filters
            if name.contains("-0000") || name.is_empty() {
                continue;
            }
            // WAN miniports and idle virtual switches: up, but nothing ever went through them
            if !hardware && r.InOctets == 0 && r.OutOctets == 0 {
                continue;
            }
            let kind = match r.Type {
                6 => "ethernet",
                71 => "wifi",
                _ => "other",
            };
            rows.push((
                r.InterfaceLuid.Value,
                NetIface {
                    name,
                    description: utf16(&r.Description),
                    kind: kind.to_string(),
                    hardware,
                    link_speed: r.TransmitLinkSpeed.max(r.ReceiveLinkSpeed),
                    in_per_sec: 0,
                    out_per_sec: 0,
                    in_total: r.InOctets,
                    out_total: r.OutOctets,
                },
            ));
        }
        FreeMibTable(t as *const _);
    }
    let now = Instant::now();
    let mut last = LAST_IF.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, prev)) = last.as_ref() {
        let secs = now.duration_since(*t).as_secs_f64();
        if secs > 0.05 {
            for (luid, i) in &mut rows {
                if let Some(&(pin, pout)) = prev.get(luid) {
                    // a counter that went backwards is an adapter reset, not negative traffic
                    i.in_per_sec = (i.in_total.saturating_sub(pin) as f64 / secs) as u64;
                    i.out_per_sec = (i.out_total.saturating_sub(pout) as f64 / secs) as u64;
                }
            }
        }
    }
    *last = Some((now, rows.iter().map(|(l, i)| (*l, (i.in_total, i.out_total))).collect()));
    drop(last);
    let mut ifaces: Vec<NetIface> = rows.into_iter().map(|(_, i)| i).collect();
    ifaces.sort_by(|a, b| b.hardware.cmp(&a.hardware).then(b.in_total.cmp(&a.in_total)));
    let hw = ifaces.iter().filter(|i| i.hardware);
    NetTotals {
        in_per_sec: hw.clone().map(|i| i.in_per_sec).sum(),
        out_per_sec: hw.map(|i| i.out_per_sec).sum(),
        ifaces,
    }
}

fn utf16(s: &[u16]) -> String {
    let n = s.iter().position(|&c| c == 0).unwrap_or(s.len());
    String::from_utf16_lossy(&s[..n]).trim().to_string()
}

#[tauri::command]
pub async fn get_net_totals() -> Result<NetTotals, String> {
    tauri::async_runtime::spawn_blocking(net_totals).await.map_err(|e| e.to_string())
}

// ---- connections ---------------------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Conn {
    pub pid: u32,
    pub name: String,
    /// "tcp" or "udp"
    pub proto: &'static str,
    pub ipv6: bool,
    /// "127.0.0.1:5173", "[::]:443"
    pub local: String,
    pub local_port: u16,
    /// "" for UDP and listening sockets
    pub remote: String,
    /// "LISTEN", "ESTABLISHED", "TIME_WAIT", ... ; "" for UDP
    pub state: &'static str,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct RemoteHost {
    pub host: String,
    pub count: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ProcNet {
    pub pid: u32,
    pub name: String,
    /// TCP ports in LISTEN
    pub listening: Vec<u16>,
    /// UDP ports bound
    pub udp: Vec<u16>,
    pub established: u32,
    /// all TCP sockets in any state
    pub tcp: u32,
    /// most-connected remote addresses (loopback left out), at most 5
    pub remotes: Vec<RemoteHost>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct ConnReport {
    pub procs: Vec<ProcNet>,
    pub conns: Vec<Conn>,
}

const TCP_STATES: [&str; 13] = [
    "", "CLOSED", "LISTEN", "SYN_SENT", "SYN_RCVD", "ESTABLISHED", "FIN_WAIT1", "FIN_WAIT2", "CLOSE_WAIT", "CLOSING",
    "LAST_ACK", "TIME_WAIT", "DELETE_TCB",
];

/// Fetches one IP Helper table, growing the buffer while the table grows between the size and the read call.
fn fetch(get: impl Fn(Option<*mut core::ffi::c_void>, &mut u32) -> u32) -> Option<Vec<u8>> {
    let mut size = 0u32;
    get(None, &mut size);
    for _ in 0..4 {
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize + 4096];
        size = buf.len() as u32;
        match get(Some(buf.as_mut_ptr() as *mut _), &mut size) {
            0 => return Some(buf),
            // ERROR_INSUFFICIENT_BUFFER: `size` now holds the new need
            122 => continue,
            _ => return None,
        }
    }
    None
}

/// Every TCP socket (IPv4 + IPv6, any state) and every UDP socket, with its owning PID; no names yet.
/// Row layouts are MIB_TCPROW_OWNER_PID / MIB_TCP6ROW_OWNER_PID / MIB_UDPROW_OWNER_PID / MIB_UDP6ROW_OWNER_PID.
fn sockets() -> Vec<Conn> {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, GetExtendedUdpTable, TCP_TABLE_OWNER_PID_ALL, UDP_TABLE_OWNER_PID,
    };
    let mut out = Vec::new();
    let rd = |b: &[u8], o: usize| u32::from_ne_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
    // ports are stored big-endian in the low 16 bits
    let port = |b: &[u8], o: usize| u16::from_be_bytes([b[o], b[o + 1]]);
    let v4 = |b: &[u8], o: usize| IpAddr::V4(Ipv4Addr::new(b[o], b[o + 1], b[o + 2], b[o + 3]));
    let v6 = |b: &[u8], o: usize| {
        let mut a = [0u8; 16];
        a.copy_from_slice(&b[o..o + 16]);
        IpAddr::V6(Ipv6Addr::from(a))
    };
    let sa = |ip: IpAddr, p: u16| SocketAddr::new(ip, p).to_string();

    // (af, row size, local addr, local port, remote addr, remote port, state, pid)
    for (af, row, la, lp, ra, rp, st, pid) in [(2u32, 24usize, 4usize, 8usize, 12usize, 16usize, 0usize, 20usize), (23, 56, 0, 20, 24, 44, 48, 52)] {
        let get = |p: Option<*mut core::ffi::c_void>, s: &mut u32| unsafe { GetExtendedTcpTable(p, s, false, af, TCP_TABLE_OWNER_PID_ALL, 0) };
        let Some(b) = fetch(get) else { continue };
        let ipv6 = af == 23;
        let addr = |o: usize| if ipv6 { v6(&b, o) } else { v4(&b, o) };
        for i in 0..rd(&b, 0) as usize {
            let base = 4 + i * row;
            if base + row > b.len() {
                break;
            }
            let state = TCP_STATES.get(rd(&b, base + st) as usize).copied().unwrap_or("");
            let lport = port(&b, base + lp);
            let remote = if state == "LISTEN" { String::new() } else { sa(addr(base + ra), port(&b, base + rp)) };
            out.push(Conn {
                pid: rd(&b, base + pid),
                name: String::new(),
                proto: "tcp",
                ipv6,
                local: sa(addr(base + la), lport),
                local_port: lport,
                remote,
                state,
            });
        }
    }
    // (af, row size, local addr, local port, pid)
    for (af, row, la, lp, pid) in [(2u32, 12usize, 0usize, 4usize, 8usize), (23, 28, 0, 20, 24)] {
        let get = |p: Option<*mut core::ffi::c_void>, s: &mut u32| unsafe { GetExtendedUdpTable(p, s, false, af, UDP_TABLE_OWNER_PID, 0) };
        let Some(b) = fetch(get) else { continue };
        let ipv6 = af == 23;
        for i in 0..rd(&b, 0) as usize {
            let base = 4 + i * row;
            if base + row > b.len() {
                break;
            }
            let lport = port(&b, base + lp);
            let ip = if ipv6 { v6(&b, base + la) } else { v4(&b, base + la) };
            out.push(Conn {
                pid: rd(&b, base + pid),
                name: String::new(),
                proto: "udp",
                ipv6,
                local: sa(ip, lport),
                local_port: lport,
                remote: String::new(),
                state: "",
            });
        }
    }
    out
}

/// The address part of "1.2.3.4:443" / "[::1]:443".
fn host_of(remote: &str) -> Option<IpAddr> {
    remote.parse::<SocketAddr>().ok().map(|s| s.ip())
}

fn summarize(conns: &[Conn]) -> Vec<ProcNet> {
    let mut by: HashMap<u32, (ProcNet, HashMap<String, u32>)> = HashMap::new();
    for c in conns {
        // PID 0 owns the TIME_WAIT leftovers of connections whose process already closed them
        if c.pid == 0 {
            continue;
        }
        let (p, remotes) = by.entry(c.pid).or_insert_with(|| (ProcNet { pid: c.pid, name: c.name.clone(), ..Default::default() }, HashMap::new()));
        match (c.proto, c.state) {
            ("udp", _) => {
                if !p.udp.contains(&c.local_port) {
                    p.udp.push(c.local_port);
                }
            }
            (_, "LISTEN") => {
                p.tcp += 1;
                if !p.listening.contains(&c.local_port) {
                    p.listening.push(c.local_port);
                }
            }
            (_, st) => {
                p.tcp += 1;
                if st == "ESTABLISHED" {
                    p.established += 1;
                }
                if let Some(ip) = host_of(&c.remote).filter(|ip| !ip.is_loopback() && !ip.is_unspecified()) {
                    *remotes.entry(ip.to_string()).or_default() += 1;
                }
            }
        }
    }
    let mut out: Vec<ProcNet> = by
        .into_values()
        .map(|(mut p, remotes)| {
            p.listening.sort_unstable();
            p.udp.sort_unstable();
            let mut r: Vec<RemoteHost> = remotes.into_iter().map(|(host, count)| RemoteHost { host, count }).collect();
            r.sort_by(|a, b| b.count.cmp(&a.count).then(a.host.cmp(&b.host)));
            r.truncate(5);
            p.remotes = r;
            p
        })
        .collect();
    out.sort_by(|a, b| b.established.cmp(&a.established).then(b.listening.len().cmp(&a.listening.len())).then(a.pid.cmp(&b.pid)));
    out
}

pub fn connections() -> ConnReport {
    let mut conns = sockets();
    let names = crate::gpu::proc_names();
    for c in &mut conns {
        c.name = names.get(&c.pid).cloned().unwrap_or_else(|| if c.pid == 4 { "System".into() } else { String::new() });
    }
    conns.sort_by(|a, b| a.pid.cmp(&b.pid).then(a.proto.cmp(b.proto)).then(a.local_port.cmp(&b.local_port)));
    ConnReport { procs: summarize(&conns), conns }
}

#[tauri::command]
pub async fn get_connections() -> Result<ConnReport, String> {
    tauri::async_runtime::spawn_blocking(connections).await.map_err(|e| e.to_string())
}

// ---- what holds port N --------------------------------------------------------------------------------

#[derive(Serialize, Clone, Debug, Default)]
pub struct PortOwner {
    pub pid: u32,
    pub name: String,
    /// command line, or the exe path when the command line can't be read; "" when neither can
    pub cmd: String,
    /// "tcp", "udp" or "tcp+udp"
    pub proto: String,
    /// states of its sockets on that port, "LISTEN", "ESTABLISHED", ...
    pub states: Vec<String>,
}

/// Processes with a socket bound to local port `port`, listening ones first. TIME_WAIT leftovers (PID 0) are
/// left out: nothing holds them.
pub fn port_owner(port: u16) -> Vec<PortOwner> {
    let mut found: Vec<PortOwner> = Vec::new();
    for c in sockets().into_iter().filter(|c| c.local_port == port && c.pid != 0) {
        let i = match found.iter().position(|o| o.pid == c.pid) {
            Some(i) => i,
            None => {
                found.push(PortOwner { pid: c.pid, ..Default::default() });
                found.len() - 1
            }
        };
        let o = &mut found[i];
        if !o.proto.split('+').any(|p| p == c.proto) {
            o.proto = if o.proto.is_empty() { c.proto.to_string() } else { "tcp+udp".to_string() };
        }
        if !c.state.is_empty() && !o.states.iter().any(|s| s == c.state) {
            o.states.push(c.state.to_string());
        }
    }
    if found.is_empty() {
        return found;
    }
    if let Ok(mut sys) = System::new() {
        let pids: Vec<Pid> = found.iter().map(|o| Pid::from_u32(o.pid)).collect();
        sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&pids),
            true,
            ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always).with_exe(UpdateKind::Always),
        );
        for o in &mut found {
            let Some(p) = sys.process(Pid::from_u32(o.pid)) else { continue };
            o.name = p.name().to_string_lossy().into_owned();
            o.cmd = p.cmd().iter().map(|s| s.to_string_lossy()).collect::<Vec<_>>().join(" ");
            if o.cmd.is_empty() {
                o.cmd = p.exe().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
            }
        }
    }
    for o in &mut found {
        if o.pid == 4 && o.name.is_empty() {
            // http.sys / SMB ports belong to the kernel
            o.name = "System".into();
        }
    }
    found.sort_by_key(|o| (!o.states.iter().any(|s| s == "LISTEN") && o.proto != "udp", o.pid));
    found
}

#[tauri::command]
pub async fn get_port_owner(port: u16) -> Result<Vec<PortOwner>, String> {
    tauri::async_runtime::spawn_blocking(move || port_owner(port)).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_our_own_listener() {
        let me = std::process::id();
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let owners = port_owner(port);
        let o = owners.iter().find(|o| o.pid == me).expect("our pid holds the port");
        assert_eq!(o.proto, "tcp");
        assert!(o.states.iter().any(|s| s == "LISTEN"));
        assert!(!o.name.is_empty());

        let u = std::net::UdpSocket::bind("[::1]:0").unwrap();
        let uport = u.local_addr().unwrap().port();
        assert!(port_owner(uport).iter().any(|o| o.pid == me && o.proto == "udp"));

        // a connection to ourselves shows up as established with a loopback remote (not counted as a host)
        let _c = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        let r = connections();
        let p = r.procs.iter().find(|p| p.pid == me).unwrap();
        assert!(p.listening.contains(&port) && p.udp.contains(&uport));
        assert!(p.established >= 1 && p.remotes.iter().all(|h| h.host != "127.0.0.1"));
        assert!(r.conns.iter().any(|c| c.pid == me && c.state == "ESTABLISHED" && c.remote == format!("127.0.0.1:{port}")));
    }

    #[test]
    fn interface_rates() {
        let a = net_totals();
        std::thread::sleep(std::time::Duration::from_millis(200));
        let b = net_totals();
        println!("{:#?}", b);
        assert_eq!(a.ifaces.len(), a.ifaces.iter().filter(|i| !i.name.is_empty()).count());
        assert!(b.ifaces.iter().all(|i| i.kind != ""));
    }
}
