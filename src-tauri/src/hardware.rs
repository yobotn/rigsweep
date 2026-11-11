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
    rated = $_.Speed; type = $_.SMBIOSMemoryType; maker = "$($_.Manufacturer)".Trim(); part = "$($_.PartNumber)".Trim(); slot = "$($_.DeviceLocator)".Trim() } })
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
try {
  $o.battery = @(Get-CimInstance -Namespace root\wmi -ClassName BatteryFullChargedCapacity -ErrorAction Stop | ForEach-Object {
    $id = $_.InstanceName
    $s = Get-CimInstance -Namespace root\wmi -ClassName BatteryStaticData -ErrorAction SilentlyContinue | Where-Object InstanceName -eq $id
    [ordered]@{ full = $_.FullChargedCapacity; design = $s.DesignedCapacity } })
} catch {}
$o | ConvertTo-Json -Depth 5 -Compress
"#;

static CACHE: Mutex<Option<Value>> = Mutex::new(None);

/// The hardware inventory; `fresh` re-reads it instead of returning the cached one.
pub fn inventory(fresh: bool) -> Result<Value, String> {
    if !fresh {
        if let Some(v) = CACHE.lock().ok().and_then(|c| c.clone()) {
            return Ok(v);
        }
    }
    let out = crate::util::powershell(SCRIPT).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(out.trim()).map_err(|e| format!("hardware: {e}"))?;
    if let Ok(mut c) = CACHE.lock() {
        *c = Some(v.clone());
    }
    Ok(v)
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
}
