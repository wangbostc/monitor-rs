use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use super::MetricError;
use crate::sample::{MemInfo, MemPressure, SwapInfo};

pub struct MemReading {
    pub mem: MemInfo,
    pub swap: SwapInfo,
}

pub struct MemSampler {
    sys: System,
}

impl MemSampler {
    pub fn new() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        Self { sys }
    }

    pub fn tick(&mut self) -> Result<MemReading, MetricError> {
        self.sys.refresh_memory();

        let total = self.sys.total_memory();
        // Activity Monitor's "Memory Used" excludes the reclaimable file
        // cache (inactive + purgeable + speculative pages). Falling back
        // to sysinfo's total-minus-free if the Mach call ever fails keeps
        // the metric live across macOS revisions.
        let used = activity_monitor_used_bytes().unwrap_or_else(|| self.sys.used_memory());
        let swap_total = self.sys.total_swap();
        let swap_used = self.sys.used_swap();

        let pressure = classify_pressure(used, total);

        Ok(MemReading {
            mem: MemInfo { used_bytes: used, total_bytes: total, pressure },
            swap: SwapInfo { used_bytes: swap_used, total_bytes: swap_total },
        })
    }
}

impl Default for MemSampler {
    fn default() -> Self { Self::new() }
}

/// Approximate Activity Monitor's memory-pressure classification.
/// Apple's exact formula isn't public; tuned for the AM-style "used"
/// reported above. Under that accounting, 50% routinely means a busy
/// system and 75% means real pressure with compression engaging.
pub fn classify_pressure(used: u64, total: u64) -> MemPressure {
    if total == 0 { return MemPressure::Normal; }
    let r = used as f64 / total as f64;
    if r < 0.50 { MemPressure::Normal }
    else if r < 0.75 { MemPressure::Warning }
    else { MemPressure::Critical }
}

// ---------- Activity Monitor-style "Memory Used" via Mach VM stats ----------
//
// AM-style Used = App Memory + Wired + Compressed
//               = (internal_page_count - purgeable_count
//                  + wire_count + compressor_page_count) × page_size
//
// `internal_page_count` is anonymous (app-allocated) pages; subtracting
// `purgeable_count` excludes pages an app explicitly marked as reclaimable
// (e.g. caches that NSPurgeableData controls).
//
// Reference: <mach/vm_statistics.h>, Activity Monitor disassembly notes,
// htop-osx's `PlatformHelpers.c`.

#[allow(non_camel_case_types)]
type mach_port_t = u32;
#[allow(non_camel_case_types)]
type host_flavor_t = libc::c_int;
#[allow(non_camel_case_types)]
type natural_t = u32;
#[allow(non_camel_case_types)]
type kern_return_t = libc::c_int;

const KERN_SUCCESS: kern_return_t = 0;
const HOST_VM_INFO64: host_flavor_t = 4;
/// vm_statistics64_data_t is 38 natural_t (u32)-sized words.
const HOST_VM_INFO64_COUNT: u32 = 38;

#[repr(C)]
#[derive(Default, Copy, Clone)]
struct VmStatistics64 {
    free_count: natural_t,
    active_count: natural_t,
    inactive_count: natural_t,
    wire_count: natural_t,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: natural_t,
    speculative_count: natural_t,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: natural_t,
    throttled_count: natural_t,
    external_page_count: natural_t,
    internal_page_count: natural_t,
    total_uncompressed_pages_in_compressor: u64,
}

unsafe extern "C" {
    fn mach_host_self() -> mach_port_t;
    fn host_statistics64(
        host: mach_port_t,
        flavor: host_flavor_t,
        info: *mut natural_t,
        count: *mut u32,
    ) -> kern_return_t;
}

fn activity_monitor_used_bytes() -> Option<u64> {
    // SAFETY: sysconf(_SC_PAGESIZE) is a fundamental Unix call returning a
    // positive value on any sane system. We only fail (returning None) if
    // it returns ≤ 0, which on macOS shouldn't happen.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return None;
    }
    let page_size = page_size as u64;

    let mut stat: VmStatistics64 = VmStatistics64::default();
    let mut count: u32 = HOST_VM_INFO64_COUNT;
    // SAFETY: VmStatistics64 is repr(C) and exactly HOST_VM_INFO64_COUNT
    // natural_t-words in size; we pass a writable pointer to it together
    // with the matching count, as required by host_statistics64.
    let kr = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            &mut stat as *mut VmStatistics64 as *mut natural_t,
            &mut count,
        )
    };
    if kr != KERN_SUCCESS {
        return None;
    }

    let app_pages = (stat.internal_page_count as u64)
        .saturating_sub(stat.purgeable_count as u64);
    let used_pages = app_pages
        + (stat.wire_count as u64)
        + (stat.compressor_page_count as u64);
    Some(used_pages.saturating_mul(page_size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_thresholds() {
        assert_eq!(classify_pressure(0, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(40, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(60, 100), MemPressure::Warning);
        assert_eq!(classify_pressure(80, 100), MemPressure::Critical);
        assert_eq!(classify_pressure(0, 0), MemPressure::Normal);
    }

    #[test]
    fn mem_sampler_returns_sane_values() {
        let mut s = MemSampler::new();
        let r = s.tick().expect("tick succeeds");
        assert!(r.mem.total_bytes > 0);
        assert!(r.mem.used_bytes <= r.mem.total_bytes);
        if r.swap.total_bytes > 0 {
            assert!(r.swap.used_bytes <= r.swap.total_bytes);
        }
    }

    #[test]
    fn am_used_is_smaller_than_total_minus_free() {
        // AM-style "used" excludes the inactive file cache, so it should
        // always be ≤ (total - free) on any live system.
        let mut s = System::new_with_specifics(
            RefreshKind::nothing().with_memory(MemoryRefreshKind::everything()),
        );
        s.refresh_memory();
        let total = s.total_memory();
        let sysinfo_used = s.used_memory();
        if let Some(am_used) = activity_monitor_used_bytes() {
            assert!(am_used <= total, "am_used={am_used} > total={total}");
            // Most macOS systems will have *some* file cache; assert with a
            // safety margin so the test isn't fragile on a freshly-booted box.
            assert!(am_used <= sysinfo_used + total / 100,
                "am_used={am_used} should be ≤ sysinfo_used={sysinfo_used}");
        }
    }
}
