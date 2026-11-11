//! Temperatures for the live meter, read without any kernel driver:
//! - GPU: the same counter Task Manager shows (WDDM 2.4+ `D3DKMT_ADAPTER_PERFDATA`, Windows 10
//!   1809 and later). Works for NVIDIA, AMD and Intel GPUs whose driver reports it.
//! - CPU: the ACPI thermal zones (performance counter `\Thermal Zone Information(*)`, no admin
//!   needed). Laptops usually wire one to the CPU package; many desktops report nothing or a
//!   fixed value, so the UI labels it as approximate.
//!
//! Either one is `None` when the PC doesn't expose it. RigSweep never installs a sensor driver
//! (the usual WinRing0-style drivers are flagged as vulnerable by Microsoft).

use std::time::{Duration, Instant};
use windows::Wdk::Graphics::Direct3D::{
    D3DKMTCloseAdapter, D3DKMTEnumAdapters2, D3DKMTQueryAdapterInfo, D3DKMT_ADAPTERINFO,
    D3DKMT_ADAPTER_PERFDATA, D3DKMT_CLOSEADAPTER, D3DKMT_ENUMADAPTERS2, D3DKMT_QUERYADAPTERINFO,
    KMTQAITYPE_ADAPTERPERFDATA,
};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY,
};
use windows::core::{PCWSTR, w};

/// Readings outside this range are sensor garbage (unplugged zone, 0 K, 0xFFFF…).
fn plausible(c: f32) -> Option<f32> {
    (c.is_finite() && (5.0..=125.0).contains(&c)).then_some(c)
}

/// Hottest GPU in °C, or `None` if no adapter reports a temperature.
pub fn gpu_temp() -> Option<f32> {
    unsafe {
        let mut e = D3DKMT_ENUMADAPTERS2::default();
        if D3DKMTEnumAdapters2(&mut e).is_err() || e.NumAdapters == 0 {
            return None;
        }
        let mut adapters = vec![D3DKMT_ADAPTERINFO::default(); e.NumAdapters as usize];
        e.pAdapters = adapters.as_mut_ptr();
        if D3DKMTEnumAdapters2(&mut e).is_err() {
            return None;
        }
        adapters.truncate(e.NumAdapters as usize);
        let mut best: Option<f32> = None;
        for a in &adapters {
            let mut perf = D3DKMT_ADAPTER_PERFDATA::default();
            let mut q = D3DKMT_QUERYADAPTERINFO {
                hAdapter: a.hAdapter,
                Type: KMTQAITYPE_ADAPTERPERFDATA,
                pPrivateDriverData: &mut perf as *mut _ as *mut _,
                PrivateDriverDataSize: std::mem::size_of::<D3DKMT_ADAPTER_PERFDATA>() as u32,
            };
            // Temperature is in tenths of a degree Celsius; 0 means "not reported"
            if D3DKMTQueryAdapterInfo(&mut q).is_ok() && perf.Temperature > 0 {
                if let Some(c) = plausible(perf.Temperature as f32 / 10.0) {
                    best = Some(best.map_or(c, |b| b.max(c)));
                }
            }
            let _ = D3DKMTCloseAdapter(&D3DKMT_CLOSEADAPTER { hAdapter: a.hAdapter });
        }
        best
    }
}

/// Thermal zones update every few seconds at best, so they are read at most every `CPU_EVERY`;
/// in between the last value is reused.
const CPU_EVERY: Duration = Duration::from_secs(6);

/// Open PDH query on the thermal-zone counter, in Kelvin (or tenths of Kelvin).
struct Zones {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
    /// the counter is in tenths of Kelvin ("High Precision Temperature", Windows 10 1809+)
    tenths: bool,
}

impl Zones {
    fn open() -> Option<Zones> {
        unsafe {
            for (path, tenths) in [
                (w!(r"\Thermal Zone Information(*)\High Precision Temperature"), true),
                (w!(r"\Thermal Zone Information(*)\Temperature"), false),
            ] {
                let mut query = PDH_HQUERY::default();
                if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                    return None;
                }
                let mut counter = PDH_HCOUNTER::default();
                if PdhAddEnglishCounterW(query, path, 0, &mut counter) != 0 {
                    PdhCloseQuery(query);
                    continue;
                }
                // dropping a `Zones` that reads nothing closes its query
                let z = Zones { query, counter, tenths };
                if z.read().is_some() {
                    return Some(z);
                }
            }
            None
        }
    }

    /// Hottest zone in °C.
    fn read(&self) -> Option<f32> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let (mut size, mut count) = (0u32, 0u32);
            PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if size == 0 {
                return None;
            }
            // items + their name strings share one buffer; u64 keeps it aligned for the f64s
            let mut buf = vec![0u64; (size as usize).div_ceil(8)];
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return None;
            }
            std::slice::from_raw_parts(items, count as usize)
                .iter()
                .filter(|it| it.FmtValue.CStatus == 0)
                .map(|it| it.FmtValue.Anonymous.doubleValue)
                .map(|k| (if self.tenths { k / 10.0 } else { k }) - 273.15)
                .filter_map(|c| plausible(c as f32))
                .reduce(f32::max)
        }
    }
}

impl Drop for Zones {
    fn drop(&mut self) {
        unsafe {
            PdhCloseQuery(self.query);
        }
    }
}

pub struct Sensors {
    zones: Option<Zones>,
    cpu: Option<f32>,
    last_cpu: Option<Instant>,
}

impl Sensors {
    pub fn new() -> Self {
        Sensors { zones: Zones::open(), cpu: None, last_cpu: None }
    }

    /// (cpu °C, gpu °C)
    pub fn read(&mut self) -> (Option<f32>, Option<f32>) {
        if let Some(z) = &self.zones {
            if self.last_cpu.is_none_or(|t| t.elapsed() >= CPU_EVERY) {
                self.last_cpu = Some(Instant::now());
                self.cpu = z.read();
            }
        }
        (self.cpu.map(round1), gpu_temp().map(round1))
    }
}

fn round1(c: f32) -> f32 {
    (c * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_garbage() {
        assert_eq!(plausible(-273.2), None);
        assert_eq!(plausible(0.0), None);
        assert_eq!(plausible(6553.5), None);
        assert_eq!(plausible(f32::NAN), None);
        assert_eq!(plausible(47.5), Some(47.5));
    }

    /// Prints what this PC reports: cargo test heat_report -- --ignored --nocapture
    #[test]
    #[ignore]
    fn heat_report() {
        let mut s = Sensors::new();
        println!("cpu, gpu = {:?}", s.read());
    }
}
