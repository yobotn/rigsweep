//! Hardware tab: what CPU-Z, GPU-Z and CrystalDiskInfo show, read from Windows' own inventory
//! (CIM / the Storage Management API) without any third-party driver, plus a simple disk speed test.
//!
//! The inventory is one PowerShell call (about a second), cached until the user asks for a refresh.
//! Each section is wrapped in its own try/catch, so a PC that hides one part still shows the rest.

use serde::Serialize;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::OpenOptionsExt;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SCRIPT: &str = r#"
$o = [ordered]@{}
try {
  $cs = Get-CimInstance Win32_ComputerSystem; $os = Get-CimInstance Win32_OperatingSystem
  $o.system = [ordered]@{ maker = "$($cs.Manufacturer)".Trim(); model = "$($cs.Model)".Trim(); os = "$($os.Caption)".Trim();
    build = "$($os.BuildNumber)"; boot = $os.LastBootUpTime.ToString('o') }
} catch {}
try {
  $o.cpu = @(Get-CimInstance Win32_Processor | ForEach-Object { [ordered]@{ name = "$($_.Name)".Trim(); cores = $_.NumberOfCores;
    threads = $_.NumberOfLogicalProcessors; max_mhz = $_.MaxClockSpeed; l2_kb = $_.L2CacheSize; l3_kb = $_.L3CacheSize;
    socket = "$($_.SocketDesignation)";
    # with Hyper-V / WSL 2 running, the CPU flag reads False although virtualization is on
    virt = if ($cs.HypervisorPresent) { $true } else { $_.VirtualizationFirmwareEnabled } } })
} catch {}
try {
  $b = Get-CimInstance Win32_BaseBoard | Select-Object -First 1; $bios = Get-CimInstance Win32_BIOS | Select-Object -First 1
  $o.board = [ordered]@{ maker = "$($b.Manufacturer)".Trim(); model = "$($b.Product)".Trim(); bios = "$($bios.SMBIOSBIOSVersion)".Trim();
    bios_date = if ($bios.ReleaseDate) { $bios.ReleaseDate.ToString('yyyy-MM-dd') } else { '' } }
} catch {}
try {
  $o.ram = @(Get-CimInstance Win32_PhysicalMemory | ForEach-Object { [ordered]@{ bytes = [uint64]$_.Capacity; mhz = $_.ConfiguredClockSpeed;
    rated = $_.Speed; type = $_.SMBIOSMemoryType; maker = "$($_.Manufacturer)".Trim(); part = "$($_.PartNumber)".Trim(); slot = "$($_.DeviceLocator)".Trim();
    form_factor = $_.FormFactor } })
  $o.ram_slots = [int](Get-CimInstance Win32_PhysicalMemoryArray | Measure-Object MemoryDevices -Sum).Sum
} catch {}
try {
  # Win32_VideoController.AdapterRAM stops at 4 GB; the driver's registry key has the real size
  $vram = @{}
  Get-ChildItem 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}' -ErrorAction SilentlyContinue | ForEach-Object {
    $p = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
    if ($p.DriverDesc -and $p.'HardwareInformation.qwMemorySize') { $vram[$p.DriverDesc] = [uint64]$p.'HardwareInformation.qwMemorySize' }
  }
  $o.gpu = @(Get-CimInstance Win32_VideoController | Where-Object { $_.PNPDeviceID -like 'PCI*' } | ForEach-Object { [ordered]@{
    name = "$($_.Name)".Trim(); driver = "$($_.DriverVersion)"; driver_date = if ($_.DriverDate) { $_.DriverDate.ToString('yyyy-MM-dd') } else { '' };
    vram = if ($vram[$_.Name]) { $vram[$_.Name] } else { [uint64]$_.AdapterRAM };
    width = $_.CurrentHorizontalResolution; height = $_.CurrentVerticalResolution; hz = $_.CurrentRefreshRate } })
} catch {}
#DISKS#
try {
  # root\wmi battery classes come from the ACPI battery driver; some PCs hide them from non-admins, so fall back to Win32_Battery
  $bats = @()
  try {
    $stat = @(Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData -ErrorAction Stop)
    $cyc = @(Get-CimInstance -Namespace root\wmi -ClassName BatteryCycleCount -ErrorAction SilentlyContinue)
    $bats = @(Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity -ErrorAction Stop | ForEach-Object {
      $id = $_.InstanceName; $s = $stat | Where-Object InstanceName -eq $id | Select-Object -First 1
      $c = $cyc | Where-Object InstanceName -eq $id | Select-Object -First 1
      # Chemistry is four ASCII letters packed in a uint32 ("LION", "LiP")
      $chem = ''; try { if ($s.Chemistry) { $chem = [Text.Encoding]::ASCII.GetString([BitConverter]::GetBytes([uint32]$s.Chemistry)).Trim([char]0).Trim() } } catch {}
      [ordered]@{ full = $_.FullChargedCapacity; design = $s.DesignedCapacity; full_mwh = $_.FullChargedCapacity; design_mwh = $s.DesignedCapacity;
        cycles = $c.CycleCount; chemistry = $chem; name = "$($s.DeviceName)".Trim() } })
  } catch {}
  if (-not $bats.Count) {
    $chems = @{ 1 = 'Other'; 3 = 'Lead acid'; 4 = 'NiCd'; 5 = 'NiMH'; 6 = 'Li-ion'; 7 = 'Zinc air'; 8 = 'Li-polymer' }
    $bats = @(Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | ForEach-Object { [ordered]@{ full = $_.FullChargeCapacity; design = $_.DesignCapacity;
      full_mwh = $_.FullChargeCapacity; design_mwh = $_.DesignCapacity; cycles = $null; chemistry = "$($chems[[int]$_.Chemistry])"; name = "$($_.Name)".Trim() } })
  }
  if ($bats.Count) { $o.battery = $bats }
} catch {}
$o | ConvertTo-Json -Depth 5 -Compress
"#;

/// The disk section on its own, so the background health check is one short PowerShell call.
const DISKS: &str = r#"
try {
  $letters = @{}
  Get-Partition -ErrorAction SilentlyContinue | Where-Object DriveLetter | ForEach-Object { $letters["$($_.DiskNumber)"] += @("$($_.DriveLetter):") }
  $o.disks = @(Get-PhysicalDisk | Sort-Object { [int]$_.DeviceId } | ForEach-Object {
    $r = $null; try { $r = $_ | Get-StorageReliabilityCounter -ErrorAction Stop } catch {}
    [ordered]@{ id = "$($_.DeviceId)"; model = "$($_.FriendlyName)".Trim(); bytes = [uint64]$_.Size; media = "$($_.MediaType)"; bus = "$($_.BusType)";
      health = "$($_.HealthStatus)"; status = (@($_.OperationalStatus) -join ', '); firmware = "$($_.FirmwareVersion)".Trim();
      letters = @($letters["$($_.DeviceId)"] | Where-Object { $_ });
      temp = $r.Temperature; temp_max = $r.TemperatureMax; wear = $r.Wear; hours = $r.PowerOnHours;
      read_err = $r.ReadErrorsUncorrected; write_err = $r.WriteErrorsUncorrected } })
} catch {}
"#;

static CACHE: Mutex<Option<Value>> = Mutex::new(None);

/// The hardware inventory; `fresh` re-reads it instead of returning the cached one.
pub fn inventory(fresh: bool) -> Result<Value, String> {
    if !fresh {
        if let Some(v) = CACHE.lock().ok().and_then(|c| c.clone()) {
            let mut v = v;
            // the clock moves; everything else in the cache doesn't
            if let (Some(d), Some(mhz)) = (v.get_mut("cpu_detail"), clocks(0).map(|c| c.0)) {
                d["cur_mhz"] = mhz.into();
            }
            return Ok(v);
        }
    }
    let out = crate::util::powershell(&SCRIPT.replace("#DISKS#", DISKS)).map_err(|e| e.to_string())?;
    let mut v: Value = serde_json::from_str(out.trim()).map_err(|e| format!("hardware: {e}"))?;
    enrich(&mut v);
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(v.clone());
    }
    Ok(v)
}

/// Adds what Rust can read or decode better than the script: CPU detail, RAM names, battery health.
fn enrich(v: &mut Value) {
    if !v.is_object() {
        return;
    }
    v["cpu_detail"] = serde_json::to_value(cpu_detail()).unwrap_or(Value::Null);
    if let Some(ram) = v.get_mut("ram").and_then(Value::as_array_mut) {
        for m in ram.iter_mut().filter(|m| m.is_object()) {
            let t = mem_type_name(m["type"].as_u64().unwrap_or(0));
            m["type_name"] = t.into();
            m["form"] = form_factor_name(m["form_factor"].as_u64().unwrap_or(0)).into();
        }
        let n = ram.len() as u64;
        // soldered LPDDR lists one entry per package or channel, so the module count says nothing about channels
        let lp = ram.iter().any(|m| m["type_name"].as_str().is_some_and(|t| t.starts_with("LP")));
        v["ram_populated"] = n.into();
        // a guess: consumer boards run 2+ modules in dual channel; 8+ modules is a workstation with 4+ channels
        v["ram_channels"] = match n {
            _ if lp => Value::Null,
            0 => Value::Null,
            1 => 1.into(),
            2..=7 => 2.into(),
            _ => (n / 2).into(),
        };
    }
    if let Some(bats) = v.get_mut("battery").and_then(Value::as_array_mut) {
        for b in bats.iter_mut().filter(|b| b.is_object()) {
            let (full, design) = (b["full_mwh"].as_f64().unwrap_or(0.0), b["design_mwh"].as_f64().unwrap_or(0.0));
            b["health_pct"] = if full > 0.0 && design > 0.0 { ((full / design * 100.0).round().min(100.0) as u64).into() } else { Value::Null };
        }
    }
}

/// Win32_PhysicalMemory.SMBIOSMemoryType (SMBIOS spec table 7.18.2) to a name; unknown → "".
pub fn mem_type_name(t: u64) -> &'static str {
    match t {
        15 => "SDRAM",
        18 => "DDR",
        19 => "DDR2",
        20 => "DDR2 FB-DIMM",
        24 => "DDR3",
        25 => "FBD2",
        26 => "DDR4",
        27 => "LPDDR",
        28 => "LPDDR2",
        29 => "LPDDR3",
        30 => "LPDDR4",
        31 => "NVDIMM",
        32 => "HBM",
        33 => "HBM2",
        34 => "DDR5",
        35 => "LPDDR5",
        36 => "HBM3",
        _ => "",
    }
}

/// Win32_PhysicalMemory.FormFactor to a name; unknown → "".
pub fn form_factor_name(f: u64) -> &'static str {
    match f {
        6 => "Proprietary",
        7 => "SIMM",
        8 => "DIMM",
        11 => "RIMM",
        12 => "SODIMM",
        13 => "SRIMM",
        21 | 22 => "BGA",
        _ => "",
    }
}

// ---------- CPU detail (cpuid, no driver) ----------

#[derive(Serialize, Clone, Debug, Default)]
pub struct CpuCache {
    pub level: u8,
    /// "data" | "instruction" | "unified"
    pub kind: &'static str,
    pub size_kb: u32,
    /// 0 = fully associative
    pub ways: u32,
    pub line: u32,
    /// how many of these the chip has; None when only cpuid (one core's view) answered
    pub count: Option<u32>,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CoreClass {
    /// Windows' EfficiencyClass: higher = faster cores
    pub class: u8,
    pub cores: u32,
    pub threads: u32,
}

#[derive(Serialize, Clone, Debug, Default)]
pub struct CpuDetail {
    pub vendor: String,
    pub brand: String,
    /// display family / model, extended parts already folded in (as CPU-Z shows "Ext. Family / Ext. Model")
    pub family: u32,
    pub model: u32,
    pub stepping: u32,
    pub ext_family: u32,
    pub ext_model: u32,
    /// "" when the chip isn't in the table
    pub codename: &'static str,
    pub uarch: &'static str,
    pub features: Vec<&'static str>,
    /// running under a hypervisor (Hyper-V, VBS, WSL 2): the root partition then sees VT-x/AMD-V hidden
    pub hypervisor: bool,
    pub caches: Vec<CpuCache>,
    pub cores: u32,
    pub threads: u32,
    pub hybrid: bool,
    pub p_cores: Option<u32>,
    pub e_cores: Option<u32>,
    pub core_classes: Vec<CoreClass>,
    /// what the power manager reports; Windows often caps this reading near the base clock
    pub cur_mhz: Option<u32>,
    pub base_mhz: Option<u32>,
}

#[cfg(target_arch = "x86_64")]
#[allow(unused_unsafe)]
fn cpuid(leaf: u32, sub: u32) -> [u32; 4] {
    let r = unsafe { std::arch::x86_64::__cpuid_count(leaf, sub) };
    [r.eax, r.ebx, r.ecx, r.edx]
}

/// Everything CPU-Z's CPU tab shows that doesn't need a driver.
pub fn cpu_detail() -> CpuDetail {
    let mut d = CpuDetail::default();
    #[cfg(target_arch = "x86_64")]
    read_cpuid(&mut d);
    let cores = core_info();
    d.cores = cores.iter().map(|c| c.cores).sum();
    d.threads = cores.iter().map(|c| c.threads).sum();
    if cores.len() > 1 {
        d.hybrid = true;
        let top = cores.iter().map(|c| c.class).max().unwrap_or(0);
        d.p_cores = Some(cores.iter().filter(|c| c.class == top).map(|c| c.cores).sum());
        d.e_cores = Some(cores.iter().filter(|c| c.class != top).map(|c| c.cores).sum());
    }
    d.core_classes = cores;
    // Windows sees every core's caches (P and E cores differ); cpuid only the core it ran on
    let win = cache_info();
    if !win.is_empty() {
        d.caches = win;
    }
    if let Some((cur, base)) = clocks(d.threads as usize) {
        d.cur_mhz = Some(cur);
        d.base_mhz = Some(base);
    }
    d
}

#[cfg(target_arch = "x86_64")]
fn read_cpuid(d: &mut CpuDetail) {
    let bit = |r: u32, b: u32| r >> b & 1 == 1;
    let l0 = cpuid(0, 0);
    let max = l0[0];
    d.vendor = [l0[1], l0[3], l0[2]].iter().flat_map(|r| r.to_le_bytes()).map(char::from).collect::<String>().trim().to_string();
    let ext_max = cpuid(0x8000_0000, 0)[0];
    if ext_max >= 0x8000_0004 {
        let b: Vec<u8> = (0x8000_0002..=0x8000_0004u32).flat_map(|l| cpuid(l, 0)).flat_map(|r| r.to_le_bytes()).collect();
        d.brand = String::from_utf8_lossy(&b).trim_matches(char::from(0)).trim().to_string();
    }
    let l1 = if max >= 1 { cpuid(1, 0) } else { [0; 4] };
    let (base_fam, base_model) = (l1[0] >> 8 & 0xf, l1[0] >> 4 & 0xf);
    d.ext_family = l1[0] >> 20 & 0xff;
    d.ext_model = l1[0] >> 16 & 0xf;
    d.stepping = l1[0] & 0xf;
    d.family = if base_fam == 0xf { base_fam + d.ext_family } else { base_fam };
    d.model = if base_fam == 6 || base_fam == 0xf { base_model + (d.ext_model << 4) } else { base_model };
    (d.codename, d.uarch) = codename(&d.vendor, d.family, d.model, &d.brand);
    d.hypervisor = bit(l1[2], 31);

    let l7 = if max >= 7 { cpuid(7, 0) } else { [0; 4] };
    let l7s1 = if max >= 7 && l7[0] >= 1 { cpuid(7, 1) } else { [0; 4] };
    let e1 = if ext_max >= 0x8000_0001 { cpuid(0x8000_0001, 0) } else { [0; 4] };
    let flags: [(bool, &'static str); 30] = [
        (bit(l1[3], 23), "MMX"),
        (bit(l1[3], 25), "SSE"),
        (bit(l1[3], 26), "SSE2"),
        (bit(l1[2], 0), "SSE3"),
        (bit(l1[2], 9), "SSSE3"),
        (bit(l1[2], 19), "SSE4.1"),
        (bit(l1[2], 20), "SSE4.2"),
        (bit(e1[2], 6), "SSE4A"),
        (bit(e1[3], 29), "x86-64"),
        (bit(l1[2], 23), "POPCNT"),
        (bit(l1[2], 25), "AES"),
        (bit(l1[2], 1), "PCLMULQDQ"),
        (bit(l1[2], 28), "AVX"),
        (bit(l1[2], 29), "F16C"),
        (bit(l1[2], 12), "FMA3"),
        (bit(l7[1], 5), "AVX2"),
        (bit(l7[1], 3), "BMI1"),
        (bit(l7[1], 8), "BMI2"),
        (bit(l7[1], 19), "ADX"),
        (bit(l7[1], 29), "SHA"),
        (bit(l7[2], 9), "VAES"),
        (bit(l7[1], 16), "AVX-512F"),
        (bit(l7[2], 11), "AVX-512 VNNI"),
        (bit(l7s1[0], 5), "AVX-512 BF16"),
        (bit(l7s1[0], 4), "AVX-VNNI"),
        (bit(l7[3], 24), "AMX-TILE"),
        (bit(l7[3], 25), "AMX-INT8"),
        (bit(l7[3], 22), "AMX-BF16"),
        (bit(l1[2], 5), "VT-x"),
        (bit(e1[2], 2), "AMD-V"),
    ];
    d.features = flags.iter().filter(|f| f.0).map(|f| f.1).collect();

    // deterministic cache parameters: Intel leaf 4, AMD 0x8000001D (needs TopologyExtensions); same register layout
    let amd = d.vendor == "AuthenticAMD" || d.vendor == "HygonGenuine";
    let leaf = if amd && ext_max >= 0x8000_001D && bit(e1[2], 22) { Some(0x8000_001D) } else if !amd && max >= 4 { Some(4) } else { None };
    if let Some(leaf) = leaf {
        for sub in 0..16 {
            let r = cpuid(leaf, sub);
            let kind = match r[0] & 0x1f {
                0 => break,
                1 => "data",
                2 => "instruction",
                3 => "unified",
                _ => continue,
            };
            let (ways, parts, line, sets) = ((r[1] >> 22) + 1, (r[1] >> 12 & 0x3ff) + 1, (r[1] & 0xfff) + 1, r[2] as u64 + 1);
            d.caches.push(CpuCache {
                level: (r[0] >> 5 & 7) as u8,
                kind,
                size_kb: (ways as u64 * parts as u64 * line as u64 * sets / 1024) as u32,
                ways: if bit(r[0], 9) { 0 } else { ways },
                line,
                count: None,
            });
        }
    }
}

/// Codename and microarchitecture by vendor + display family + model; the brand string only splits dies that share a model
/// number (EPYC vs Ryzen, Hawk Point vs Phoenix, HX parts). Unknown → ("", ""): never guess.
pub fn codename(vendor: &str, family: u32, model: u32, brand: &str) -> (&'static str, &'static str) {
    let b = brand.to_ascii_uppercase();
    let (epyc, tr, hx) = (b.contains("EPYC"), b.contains("THREADRIPPER"), b.contains("HX"));
    // Ryzen x 8x4x (8840HS, 8640U): Hawk Point, the Phoenix refresh with the same model number
    let hawk = b.split(|c: char| !c.is_ascii_alphanumeric()).any(|w| {
        let w = w.as_bytes();
        w.len() >= 4 && w[..4].iter().all(u8::is_ascii_digit) && w[0] == b'8' && w[2] == b'4'
    });
    match (vendor, family) {
        ("GenuineIntel", 6) => match model {
            0x7D | 0x7E => ("Ice Lake", "Sunny Cove"),
            0x8C | 0x8D => ("Tiger Lake", "Willow Cove"),
            0xA5 | 0xA6 => ("Comet Lake", "Skylake"),
            0xA7 => ("Rocket Lake", "Cypress Cove"),
            0x8F => ("Sapphire Rapids", "Golden Cove"),
            0xCF => ("Emerald Rapids", "Raptor Cove"),
            0x97 | 0x9A => ("Alder Lake", "Golden Cove + Gracemont"),
            0xBE => ("Alder Lake-N", "Gracemont"),
            0xB7 | 0xBA | 0xBF => ("Raptor Lake", "Raptor Cove + Gracemont"),
            0xAA | 0xAC => ("Meteor Lake", "Redwood Cove + Crestmont"),
            0xB5 => ("Arrow Lake-U", "Redwood Cove + Crestmont"),
            0xC5 | 0xC6 => ("Arrow Lake", "Lion Cove + Skymont"),
            0xBD => ("Lunar Lake", "Lion Cove + Skymont"),
            0xCC => ("Panther Lake", "Cougar Cove + Darkmont"),
            _ => ("", ""),
        },
        ("AuthenticAMD", 0x17) => match model {
            0x01 if epyc => ("Naples", "Zen"),
            0x01 if tr => ("Whitehaven", "Zen"),
            0x01 => ("Summit Ridge", "Zen"),
            0x08 if tr => ("Colfax", "Zen+"),
            0x08 => ("Pinnacle Ridge", "Zen+"),
            0x11 => ("Raven Ridge", "Zen"),
            0x18 => ("Picasso", "Zen+"),
            0x20 => ("Dali", "Zen"),
            0x31 if epyc => ("Rome", "Zen 2"),
            0x31 => ("Castle Peak", "Zen 2"),
            0x60 => ("Renoir", "Zen 2"),
            0x68 => ("Lucienne", "Zen 2"),
            0x71 => ("Matisse", "Zen 2"),
            0x90 | 0x91 => ("Van Gogh", "Zen 2"),
            0xA0 => ("Mendocino", "Zen 2"),
            _ => ("", ""),
        },
        ("AuthenticAMD", 0x19) => match model {
            0x01 => ("Milan", "Zen 3"),
            0x08 => ("Chagall", "Zen 3"),
            0x21 => ("Vermeer", "Zen 3"),
            0x50 => ("Cezanne", "Zen 3"),
            0x40 | 0x44 => ("Rembrandt", "Zen 3+"),
            0x10 | 0x11 => ("Genoa", "Zen 4"),
            0x18 => ("Storm Peak", "Zen 4"),
            0x61 if hx => ("Dragon Range", "Zen 4"),
            0x61 => ("Raphael", "Zen 4"),
            0x74 | 0x75 if hawk => ("Hawk Point", "Zen 4"),
            0x74 | 0x75 => ("Phoenix", "Zen 4"),
            0x78 => ("Phoenix 2", "Zen 4 + Zen 4c"),
            _ => ("", ""),
        },
        ("AuthenticAMD", 0x1A) => match model {
            0x02 => ("Turin", "Zen 5"),
            0x11 => ("Turin Dense", "Zen 5c"),
            0x24 => ("Strix Point", "Zen 5 + Zen 5c"),
            0x44 if hx => ("Fire Range", "Zen 5"),
            0x44 => ("Granite Ridge", "Zen 5"),
            0x60 => ("Krackan Point", "Zen 5 + Zen 5c"),
            0x70 => ("Strix Halo", "Zen 5"),
            _ => ("", ""),
        },
        _ => ("", ""),
    }
}

/// Raw GetLogicalProcessorInformationEx records of one relationship, as bytes (parsed by offset, no alignment worries).
fn lpi(rel: windows::Win32::System::SystemInformation::LOGICAL_PROCESSOR_RELATIONSHIP) -> Vec<u8> {
    use windows::Win32::System::SystemInformation::GetLogicalProcessorInformationEx;
    let mut len = 0u32;
    let _ = unsafe { GetLogicalProcessorInformationEx(rel, None, &mut len) };
    if len == 0 {
        return vec![];
    }
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    if unsafe { GetLogicalProcessorInformationEx(rel, Some(buf.as_mut_ptr().cast()), &mut len) }.is_err() {
        return vec![];
    }
    buf.iter().flat_map(|w| w.to_le_bytes()).take(len as usize).collect()
}

/// Each record, from its 8-byte header (Relationship, Size).
fn records(b: &[u8]) -> Vec<&[u8]> {
    let (mut out, mut off) = (vec![], 0usize);
    while off + 8 <= b.len() {
        let size = u32::from_le_bytes(b[off + 4..off + 8].try_into().unwrap()) as usize;
        if size < 8 || off + size > b.len() {
            break;
        }
        out.push(&b[off..off + size]);
        off += size;
    }
    out
}

/// Cores per efficiency class (one entry = not hybrid). PROCESSOR_RELATIONSHIP: EfficiencyClass @9, GroupCount @30, masks @32.
fn core_info() -> Vec<CoreClass> {
    let raw = lpi(windows::Win32::System::SystemInformation::RelationProcessorCore);
    let mut out: Vec<CoreClass> = vec![];
    for r in records(&raw).into_iter().filter(|r| r.len() >= 48) {
        let class = r[9];
        let groups = u16::from_le_bytes([r[30], r[31]]) as usize;
        let threads: u32 = (0..groups)
            .filter_map(|g| r.get(32 + g * 16..40 + g * 16))
            .map(|m| u64::from_le_bytes(m.try_into().unwrap()).count_ones())
            .sum();
        match out.iter_mut().find(|c| c.class == class) {
            Some(c) => {
                c.cores += 1;
                c.threads += threads;
            }
            None => out.push(CoreClass { class, cores: 1, threads }),
        }
    }
    out.sort_by(|a, b| b.class.cmp(&a.class));
    out
}

/// Distinct caches with how many of each. CACHE_RELATIONSHIP: Level @8, Associativity @9, LineSize @10, CacheSize @12, Type @16.
fn cache_info() -> Vec<CpuCache> {
    let raw = lpi(windows::Win32::System::SystemInformation::RelationCache);
    let mut out: Vec<CpuCache> = vec![];
    for r in records(&raw).into_iter().filter(|r| r.len() >= 20) {
        let kind = match u32::from_le_bytes(r[16..20].try_into().unwrap()) {
            0 => "unified",
            1 => "instruction",
            2 => "data",
            _ => continue,
        };
        let c = CpuCache {
            level: r[8],
            kind,
            size_kb: u32::from_le_bytes(r[12..16].try_into().unwrap()) / 1024,
            ways: if r[9] == 0xff { 0 } else { r[9] as u32 },
            line: u16::from_le_bytes([r[10], r[11]]) as u32,
            count: Some(1),
        };
        let same = |x: &CpuCache| (x.level, x.kind, x.size_kb, x.ways, x.line) == (c.level, c.kind, c.size_kb, c.ways, c.line);
        match out.iter_mut().find(|x| same(x)) {
            Some(x) => x.count = x.count.map(|n| n + 1),
            None => out.push(c),
        }
    }
    let order = |k: &str| ["data", "instruction", "unified"].iter().position(|x| *x == k).unwrap_or(3);
    out.sort_by(|a, b| (a.level, order(a.kind), b.size_kb).cmp(&(b.level, order(b.kind), a.size_kb)));
    out
}

/// (fastest current MHz across logical processors, base MHz) from the power manager; `n` = logical processors (0 = ask Windows).
fn clocks(n: usize) -> Option<(u32, u32)> {
    use windows::Win32::System::Power::{CallNtPowerInformation, ProcessorInformation, PROCESSOR_POWER_INFORMATION};
    let n = if n == 0 { core_info().iter().map(|c| c.threads as usize).sum() } else { n };
    let n = n.clamp(1, 2048);
    let mut v = vec![PROCESSOR_POWER_INFORMATION::default(); n];
    let size = (n * std::mem::size_of::<PROCESSOR_POWER_INFORMATION>()) as u32;
    let st = unsafe { CallNtPowerInformation(ProcessorInformation, None, 0, Some(v.as_mut_ptr().cast()), size) };
    if st.0 != 0 {
        return None;
    }
    let cur = v.iter().map(|p| p.CurrentMhz).max().unwrap_or(0);
    let base = v.iter().map(|p| p.MaxMhz).max().unwrap_or(0);
    (cur > 0).then_some((cur, base))
}

// ---------- disk health alerts (background check) ----------

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Alert {
    pub disk_id: String,
    pub model: String,
    /// "health" | "wear" | "errors" | "temp" | "status"
    pub kind: &'static str,
    pub from: String,
    pub to: String,
    /// "warn" | "bad"
    pub severity: &'static str,
}

fn text(d: &Value, k: &str) -> String {
    match &d[k] {
        Value::String(s) => s.trim().to_string(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

fn num(d: &Value, k: &str) -> Option<f64> {
    d[k].as_f64()
}

fn status_ok(s: &str) -> bool {
    s.is_empty() || s.eq_ignore_ascii_case("OK")
}

/// What got worse between two inventories' `disks`. Edge-triggered (a threshold is reported when crossed, not on every
/// check), and a counter missing on either side is skipped, so a counter that flickers away doesn't fake a change.
pub fn health_alerts(prev: &Value, cur: &Value) -> Vec<Alert> {
    let empty = vec![];
    let old = prev.get("disks").and_then(Value::as_array).unwrap_or(&empty);
    let key = |d: &Value| (text(d, "id"), text(d, "model"));
    let mut out = vec![];
    for d in cur.get("disks").and_then(Value::as_array).unwrap_or(&empty) {
        let (disk_id, model) = key(d);
        let mut push = |kind, from: String, to: String, severity| {
            out.push(Alert { disk_id: disk_id.clone(), model: model.clone(), kind, from, to, severity })
        };
        let status = text(d, "status");
        let Some(p) = old.iter().find(|p| key(p) == key(d)) else {
            if !status_ok(&status) {
                push("status", String::new(), status, "warn");
            }
            continue;
        };
        let (h0, h1) = (text(p, "health"), text(d, "health"));
        if h0 == "Healthy" && !h1.is_empty() && h1 != "Healthy" {
            push("health", h0, h1, "bad");
        }
        let s0 = text(p, "status");
        if status_ok(&s0) && !status_ok(&status) {
            push("status", s0, status, "warn");
        }
        if let (Some(w0), Some(w1)) = (num(p, "wear"), num(d, "wear")) {
            if let Some(&(_, sev)) = [(90.0, "bad"), (80.0, "warn")].iter().find(|(t, _)| w0 < *t && w1 >= *t) {
                push("wear", format!("{w0}"), format!("{w1}"), sev);
            }
        }
        let errs = |x: &Value| match (num(x, "read_err"), num(x, "write_err")) {
            (None, None) => None,
            (r, w) => Some(r.unwrap_or(0.0) + w.unwrap_or(0.0)),
        };
        if let (Some(e0), Some(e1)) = (errs(p), errs(d)) {
            if e1 > e0 {
                push("errors", format!("{e0}"), format!("{e1}"), "bad");
            }
        }
        // NVMe controllers run hotter by design; SATA SSDs and HDDs start to suffer around 60 °C
        let limit = if text(d, "bus").eq_ignore_ascii_case("NVMe") { 70.0 } else { 60.0 };
        if let (Some(t0), Some(t1)) = (num(p, "temp"), num(d, "temp")) {
            if t0 < limit && t1 >= limit {
                push("temp", format!("{t0}"), format!("{t1}"), if t1 >= limit + 10.0 { "bad" } else { "warn" });
            }
        }
    }
    out
}

static LAST_DISKS: Mutex<Option<Value>> = Mutex::new(None);

/// Re-reads only the disks (one short PowerShell call) and compares with the previous check. The first call only records
/// a baseline. Returns `{ "disks": [...] }` (same shape as the inventory's) and the alerts; also refreshes the inventory cache.
pub fn check_disks() -> Result<(Value, Vec<Alert>), String> {
    let script = format!("$o = [ordered]@{{}}\n{DISKS}\n$o | ConvertTo-Json -Depth 5 -Compress");
    let out = crate::util::powershell(&script).map_err(|e| e.to_string())?;
    let cur: Value = serde_json::from_str(out.trim()).map_err(|e| format!("disk check: {e}"))?;
    let Some(disks) = cur.get("disks").cloned() else {
        // the section failed this time; keep the old baseline rather than treating every disk as new next time
        return Ok((cur, vec![]));
    };
    let alerts = {
        let mut last = LAST_DISKS.lock().map_err(|_| "disk check: lock poisoned".to_string())?;
        let a = last.as_ref().map(|p| health_alerts(p, &cur)).unwrap_or_default();
        *last = Some(cur.clone());
        a
    };
    if let Ok(mut c) = CACHE.lock() {
        if let Some(inv) = c.as_mut().filter(|v| v.is_object()) {
            inv["disks"] = disks;
        }
    }
    Ok((cur, alerts))
}

// ---------- disk speed test ----------

#[derive(Serialize, Clone, Debug)]
pub struct Bench {
    pub drive: String,
    /// MB/s (10^6 bytes), large blocks, one request at a time
    pub seq_read: f64,
    pub seq_write: f64,
    /// MB/s, 4 KB blocks at random offsets, one request at a time
    pub rnd_read: f64,
    pub rnd_iops: f64,
    pub test_bytes: u64,
}

const TEST_BYTES: u64 = 512 * 1024 * 1024;
const BLOCK: usize = 4 * 1024 * 1024;
const SMALL: usize = 4096;
const RANDOM_FOR: Duration = Duration::from_secs(3);

// bypass the Windows file cache so the disk itself is measured, and remove the file however the test ends
const FILE_FLAG_NO_BUFFERING: u32 = 0x2000_0000;
const FILE_FLAG_WRITE_THROUGH: u32 = 0x8000_0000;
const FILE_FLAG_DELETE_ON_CLOSE: u32 = 0x0400_0000;

/// Page-aligned buffer, as unbuffered I/O requires.
struct Aligned {
    ptr: *mut u8,
    layout: std::alloc::Layout,
}
impl Aligned {
    fn new(len: usize) -> Aligned {
        let layout = std::alloc::Layout::from_size_align(len, 4096).expect("layout");
        let ptr = unsafe { std::alloc::alloc_zeroed(layout) };
        assert!(!ptr.is_null(), "out of memory");
        Aligned { ptr, layout }
    }
    fn buf(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.layout.size()) }
    }
}
impl Drop for Aligned {
    fn drop(&mut self) {
        unsafe { std::alloc::dealloc(self.ptr, self.layout) }
    }
}

fn xorshift(s: &mut u64) -> u64 {
    *s ^= *s << 13;
    *s ^= *s >> 7;
    *s ^= *s << 17;
    *s
}

fn mbps(bytes: u64, d: Duration) -> f64 {
    (bytes as f64 / 1e6 / d.as_secs_f64().max(1e-6) * 10.0).round() / 10.0
}

/// `drive` is a letter like "C:". Writes a 512 MB temporary file on it, reads it back, then
/// reads random 4 KB blocks for a few seconds. The file is deleted when the test ends.
pub fn bench(drive: &str) -> Result<Bench, String> {
    let d = drive.trim().trim_end_matches(['\\', '/']).to_uppercase();
    if d.len() != 2 || !d.as_bytes()[0].is_ascii_alphabetic() || !d.ends_with(':') {
        return Err(format!("not a drive letter: {drive}"));
    }
    let disks = sysinfo::Disks::new_with_refreshed_list().map_err(|e| e.to_string())?;
    let free = disks
        .iter()
        .find(|x| x.mount_point().to_string_lossy().to_uppercase().starts_with(&d))
        .map(|x| x.available_space())
        .ok_or_else(|| format!("{d} not found"))?;
    if free < TEST_BYTES * 4 {
        return Err(format!("{d} needs at least 2 GB free for the test"));
    }
    // the system drive's root is protected; use the temp folder there
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let dir = if d == sys { std::env::temp_dir() } else { std::path::PathBuf::from(format!("{d}\\")) };
    let path = dir.join(format!(".rigsweep-speedtest-{}.tmp", std::process::id()));
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH | FILE_FLAG_DELETE_ON_CLOSE)
        .open(&path)
        .map_err(|e| format!("can't create the test file on {d}: {e}"))?;
    let io = |e: std::io::Error| format!("speed test on {d}: {e}");

    // random data, so drives that compress don't look faster than they are
    let mut buf = Aligned::new(BLOCK);
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    for c in buf.buf().chunks_exact_mut(8) {
        c.copy_from_slice(&xorshift(&mut seed).to_le_bytes());
    }

    let t = Instant::now();
    for _ in 0..TEST_BYTES / BLOCK as u64 {
        f.write_all(buf.buf()).map_err(io)?;
    }
    let seq_write = mbps(TEST_BYTES, t.elapsed());

    f.seek(SeekFrom::Start(0)).map_err(io)?;
    let t = Instant::now();
    for _ in 0..TEST_BYTES / BLOCK as u64 {
        f.read_exact(buf.buf()).map_err(io)?;
    }
    let seq_read = mbps(TEST_BYTES, t.elapsed());

    let mut small = Aligned::new(SMALL);
    let blocks = TEST_BYTES / SMALL as u64;
    let (t, mut n) = (Instant::now(), 0u64);
    while t.elapsed() < RANDOM_FOR {
        f.seek(SeekFrom::Start((xorshift(&mut seed) % blocks) * SMALL as u64)).map_err(io)?;
        f.read_exact(small.buf()).map_err(io)?;
        n += 1;
    }
    let el = t.elapsed();
    drop(f);
    Ok(Bench {
        drive: d,
        seq_read,
        seq_write,
        rnd_read: mbps(n * SMALL as u64, el),
        rnd_iops: (n as f64 / el.as_secs_f64()).round(),
        test_bytes: TEST_BYTES,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_drive() {
        assert!(bench("C:\\Windows").is_err());
        assert!(bench("1:").is_err());
    }

    /// cargo test hardware_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn hardware_report() {
        println!("{}", serde_json::to_string_pretty(&inventory(true).unwrap()).unwrap());
    }

    /// cargo test speed_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn speed_report() {
        println!("{:?}", bench("C:"));
    }

    #[test]
    fn hardware_codenames() {
        assert_eq!(codename("GenuineIntel", 6, 0x97, "12th Gen Intel(R) Core(TM) i9-12900K").0, "Alder Lake");
        assert_eq!(codename("GenuineIntel", 6, 0xB7, "").0, "Raptor Lake");
        assert_eq!(codename("GenuineIntel", 6, 0xBD, "").0, "Lunar Lake");
        assert_eq!(codename("GenuineIntel", 6, 0xC6, "").0, "Arrow Lake");
        assert_eq!(codename("GenuineIntel", 6, 0xAA, "").0, "Meteor Lake");
        assert_eq!(codename("AuthenticAMD", 0x19, 0x61, "AMD Ryzen 9 7950X 16-Core Processor").0, "Raphael");
        assert_eq!(codename("AuthenticAMD", 0x19, 0x74, "AMD Ryzen 7 7840HS w/ Radeon 780M Graphics").0, "Phoenix");
        assert_eq!(codename("AuthenticAMD", 0x19, 0x75, "AMD Ryzen 7 8845HS w/ Radeon 780M Graphics").0, "Hawk Point");
        assert_eq!(codename("AuthenticAMD", 0x1A, 0x44, "AMD Ryzen 7 9700X 8-Core Processor"), ("Granite Ridge", "Zen 5"));
        assert_eq!(codename("AuthenticAMD", 0x1A, 0x24, "AMD Ryzen AI 9 HX 370").0, "Strix Point");
        assert_eq!(codename("AuthenticAMD", 0x17, 0x71, "").1, "Zen 2");
        assert_eq!(codename("AuthenticAMD", 0x19, 0x21, "").1, "Zen 3");
        assert_eq!(codename("GenuineIntel", 6, 0x01, ""), ("", ""));
        assert_eq!(codename("AuthenticAMD", 0x15, 0x30, ""), ("", ""));
        assert_eq!(codename("SomeoneElse", 6, 0x97, ""), ("", ""));
    }

    #[test]
    fn hardware_cpuid_reads() {
        let d = cpu_detail();
        assert!(!d.vendor.is_empty(), "{d:?}");
        assert!(d.family > 0);
        assert!(d.features.contains(&"SSE2"));
        assert!(d.threads >= d.cores);
        println!("{}", serde_json::to_string_pretty(&d).unwrap());
    }

    #[test]
    fn hardware_mem_types() {
        assert_eq!(mem_type_name(26), "DDR4");
        assert_eq!(mem_type_name(34), "DDR5");
        assert_eq!(mem_type_name(30), "LPDDR4");
        assert_eq!(mem_type_name(35), "LPDDR5");
        assert_eq!(mem_type_name(24), "DDR3");
        assert_eq!(mem_type_name(0), "");
        assert_eq!(form_factor_name(8), "DIMM");
        assert_eq!(form_factor_name(12), "SODIMM");
        assert_eq!(form_factor_name(0), "");
        let mut v = serde_json::json!({ "ram": [{ "type": 34, "form_factor": 8 }, { "type": 34, "form_factor": 8 }] });
        enrich(&mut v);
        assert_eq!(v["ram"][0]["type_name"], "DDR5");
        assert_eq!(v["ram"][1]["form"], "DIMM");
        assert_eq!(v["ram_channels"], 2);
        assert_eq!(v["ram_populated"], 2);
    }

    fn disk(health: &str, wear: f64, temp: f64, errs: u64) -> Value {
        serde_json::json!({ "disks": [{ "id": "0", "model": "Test SSD", "bus": "NVMe", "health": health, "status": "OK",
            "wear": wear, "temp": temp, "read_err": errs, "write_err": 0 }] })
    }

    #[test]
    fn hardware_health_alerts() {
        let a = health_alerts(&disk("Healthy", 79.0, 40.0, 0), &disk("Healthy", 81.0, 40.0, 0));
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].kind, a[0].severity, a[0].from.as_str(), a[0].to.as_str()), ("wear", "warn", "79", "81"));
        let a = health_alerts(&disk("Healthy", 85.0, 40.0, 0), &disk("Healthy", 91.0, 40.0, 0));
        assert_eq!((a.len(), a[0].severity), (1, "bad"));
        let a = health_alerts(&disk("Healthy", 10.0, 40.0, 0), &disk("Warning", 10.0, 40.0, 0));
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].kind, a[0].severity, a[0].to.as_str()), ("health", "bad", "Warning"));
        let a = health_alerts(&disk("Healthy", 10.0, 50.0, 0), &disk("Healthy", 10.0, 72.0, 0));
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].kind, a[0].severity), ("temp", "warn"));
        let a = health_alerts(&disk("Healthy", 10.0, 40.0, 0), &disk("Healthy", 10.0, 40.0, 3));
        assert_eq!((a.len(), a[0].kind), (1, "errors"));
        assert!(health_alerts(&disk("Healthy", 10.0, 40.0, 0), &disk("Healthy", 10.0, 40.0, 0)).is_empty());
        // still hot: reported once when crossed, not on every check
        assert!(health_alerts(&disk("Healthy", 10.0, 75.0, 0), &disk("Healthy", 10.0, 76.0, 0)).is_empty());
        // a disk that appears already failing
        let new = serde_json::json!({ "disks": [{ "id": "1", "model": "USB", "status": "Degraded" }] });
        let a = health_alerts(&serde_json::json!({ "disks": [] }), &new);
        assert_eq!((a.len(), a[0].kind), (1, "status"));
    }

    /// cargo test disk_check_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn disk_check_report() {
        println!("{:?}", check_disks().map(|r| r.1));
        println!("{:?}", check_disks().map(|r| r.1));
    }
}

/// One mounted drive's size and free space, for the "used now / after cleaning" meters.
#[derive(Serialize)]
pub struct Volume {
    /// drive letter like "C:"
    pub drive: String,
    pub total: u64,
    pub free: u64,
    pub system: bool,
}

pub fn volumes() -> Result<Vec<Volume>, String> {
    let disks = sysinfo::Disks::new_with_refreshed_list().map_err(|e| e.to_string())?;
    let sys = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into()).to_uppercase();
    let mut out: Vec<Volume> = disks
        .iter()
        .filter_map(|d| {
            let m = d.mount_point().to_string_lossy().to_uppercase();
            let drive = m.get(..2).filter(|s| s.ends_with(':'))?.to_string();
            (d.total_space() > 0).then(|| Volume { system: drive == sys, drive, total: d.total_space(), free: d.available_space() })
        })
        .collect();
    out.sort_by(|a, b| a.drive.cmp(&b.drive));
    out.dedup_by(|a, b| a.drive == b.drive);
    Ok(out)
}
