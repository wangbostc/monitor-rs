//! C-compatible FFI surface for the Swift side. Every function catches
//! panics and returns a safe sentinel on failure. All pointers in the
//! signatures are owned by the Rust side except where noted.

#![cfg(target_os = "macos")]

use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::{Arc, OnceLock};

use parking_lot::RwLock;

use crate::sample::{MemPressure, Sample};
use crate::sampler::SamplerHandle;
use crate::settings::Settings;
use crate::store::SampleStore;

pub const MRS_MAX_CORES: usize = 64;
pub const MRS_MAX_PROCS: usize = 16;
pub const MRS_PROC_NAME: usize = 64;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MrsProcInfo {
    pub pid: u32,
    pub name: [c_char; MRS_PROC_NAME],
    pub cpu_pct: f32,
    pub rss_bytes: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MrsSample {
    pub ts_seconds: f64,
    pub cpu_total_pct: f32,
    pub core_count: u8,
    pub cpu_per_core_pct: [f32; MRS_MAX_CORES],
    pub gpu_present: i8,
    pub gpu_pct: f32,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub mem_pressure: u8,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub proc_count: u8,
    pub procs: [MrsProcInfo; MRS_MAX_PROCS],

    // ----- new fields (additive only) -----
    pub net_rx_bps: u64,
    pub net_tx_bps: u64,
    pub disk_read_bps: u64,
    pub disk_write_bps: u64,
    pub battery_present: u8,
    pub battery_charging: u8,
    pub battery_pct: f32,
    pub cpu_temp_present: u8,
    pub cpu_temp_c: f32,
    pub gpu_temp_present: u8,
    pub gpu_temp_c: f32,
}

pub struct MrsHandle {
    #[allow(dead_code)]  // held for Drop: stops the sampler thread
    sampler: SamplerHandle,
    store: Arc<RwLock<SampleStore>>,
    settings: parking_lot::RwLock<Settings>,
    #[allow(dead_code)]  // held for Drop: flushes the log appender on shutdown
    log_guard: Option<tracing_appender::non_blocking::WorkerGuard>,
    start: std::time::Instant,
}

static LOGGING_INIT: OnceLock<()> = OnceLock::new();

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_start() -> *mut MrsHandle {
    let r = catch_unwind(|| {
        // Initialize logging on the first call only; subsequent calls reuse the
        // global subscriber. Only the first handle holds a WorkerGuard, which
        // is kept alive until that handle is dropped.
        let log_guard = if LOGGING_INIT.set(()).is_ok() {
            Some(crate::logging::init())
        } else {
            None
        };

        let settings = Settings::load();
        let sampler = SamplerHandle::spawn(settings.clone());
        let store = sampler.store.clone();
        Box::into_raw(Box::new(MrsHandle {
            sampler,
            store,
            settings: parking_lot::RwLock::new(settings),
            log_guard,
            start: std::time::Instant::now(),
        }))
    });
    r.unwrap_or(ptr::null_mut())
}

/// # Safety
/// `h` must be a non-null pointer previously returned by `monitor_rs_start` and
/// not yet freed. After this call the pointer is invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_stop(h: *mut MrsHandle) {
    if h.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let boxed = unsafe { Box::from_raw(h) };
        // SamplerHandle's Drop runs here: stops the thread and joins.
        drop(boxed);
    }));
}

/// # Safety
/// `h` must be a valid handle returned by `monitor_rs_start` and not yet freed.
/// `out` must point to a writable `MrsSample`-sized allocation.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_latest(h: *mut MrsHandle, out: *mut MrsSample) -> c_int {
    if h.is_null() || out.is_null() {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let handle = &*h;
        let store = handle.store.read();
        let Some(s) = store.latest() else { return 0 };
        *out = sample_to_c(s, handle.start);
        1
    }));
    r.unwrap_or(0)
}

/// # Safety
/// `h` must be a valid handle returned by `monitor_rs_start` and not yet freed.
/// `out` must point to a writable array of at least `n` `MrsSample` elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_recent(h: *mut MrsHandle, n: usize, out: *mut MrsSample) -> usize {
    if h.is_null() || out.is_null() || n == 0 {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let handle = &*h;
        let store = handle.store.read();
        let slice = std::slice::from_raw_parts_mut(out, n);
        let mut written = 0usize;
        for s in store.recent(n) {
            if written >= n { break; }
            slice[written] = sample_to_c(s, handle.start);
            written += 1;
        }
        written
    }));
    r.unwrap_or(0)
}

/// # Safety
/// `h` must be a valid handle returned by `monitor_rs_start` and not yet freed.
/// The returned pointer must be freed with `monitor_rs_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_settings_get(h: *mut MrsHandle) -> *const c_char {
    if h.is_null() {
        return ptr::null();
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let handle = &*h;
        let settings = handle.settings.read();
        let json = serde_json::to_string(&*settings).unwrap_or_else(|_| "{}".to_string());
        let cstring = CString::new(json).unwrap_or_else(|_| CString::new("{}").unwrap());
        cstring.into_raw() as *const c_char
    }));
    r.unwrap_or(ptr::null())
}

/// # Safety
/// `h` must be a valid handle returned by `monitor_rs_start` and not yet freed
/// (may be null, in which case settings are saved but not reflected in the handle).
/// `json` must be a valid NUL-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_settings_set(h: *mut MrsHandle, json: *const c_char) -> c_int {
    if json.is_null() {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let cstr = CStr::from_ptr(json);
        let Ok(s) = cstr.to_str() else { return 0 };
        let Ok(settings) = serde_json::from_str::<Settings>(s) else { return 0 };
        if settings.save().is_err() { return 0 }
        // Update in-memory copy too so a subsequent settings_get reflects the change.
        // (Note: this doesn't restart the sampler; sample_rate_hz changes take effect
        // on the next process launch. That's acceptable v1 behavior.)
        if !h.is_null() {
            let handle = &*h;
            *handle.settings.write() = settings;
        }
        1
    }));
    r.unwrap_or(0)
}

/// # Safety
/// `s` must be a pointer previously returned by `monitor_rs_settings_get`, or null.
/// After this call the pointer is invalid.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn monitor_rs_string_free(s: *const c_char) {
    if s.is_null() { return; }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        // Reconstruct the CString to drop it.
        let _ = CString::from_raw(s as *mut c_char);
    }));
}

fn sample_to_c(s: &Sample, start: std::time::Instant) -> MrsSample {
    // SAFETY: MrsSample is repr(C) with only Copy primitive fields and fixed-size
    // arrays — all-zeros is a valid bit pattern. Using mem::zeroed() (rather than
    // a struct literal) ensures the implicit C padding bytes are also zeroed so
    // we don't leak indeterminate stack memory across the FFI boundary.
    let mut out: MrsSample = unsafe { std::mem::zeroed() };

    out.ts_seconds = s
        .ts
        .checked_duration_since(start)
        .unwrap_or_default()
        .as_secs_f64();
    out.cpu_total_pct = s.cpu_total;
    out.core_count = s.cpu_per_core.len().min(MRS_MAX_CORES) as u8;
    out.gpu_present = if s.gpu_pct.is_some() { 1 } else { 0 };
    out.gpu_pct = s.gpu_pct.unwrap_or(0.0);
    out.mem_used_bytes = s.mem.used_bytes;
    out.mem_total_bytes = s.mem.total_bytes;
    out.mem_pressure = match s.mem.pressure {
        MemPressure::Normal => 0,
        MemPressure::Warning => 1,
        MemPressure::Critical => 2,
    };
    out.swap_used_bytes = s.swap.used_bytes;
    out.swap_total_bytes = s.swap.total_bytes;
    out.proc_count = s.top_procs.len().min(MRS_MAX_PROCS) as u8;

    for (dst, src) in out.cpu_per_core_pct.iter_mut().zip(s.cpu_per_core.iter()) {
        *dst = *src;
    }
    for (dst, src) in out.procs.iter_mut().zip(s.top_procs.iter()) {
        dst.pid = src.pid;
        dst.cpu_pct = src.cpu_pct;
        dst.rss_bytes = src.rss_bytes;
        // Truncate name to NAME-1 bytes, NUL-terminate (the zero-init guarantees
        // the trailing byte is already 0).
        let max = MRS_PROC_NAME - 1;
        let bytes = src.name.as_bytes();
        let n = bytes.len().min(max);
        for (i, b) in bytes.iter().enumerate().take(n) {
            dst.name[i] = *b as c_char;
        }
    }

    out.net_rx_bps = s.net.rx_bps;
    out.net_tx_bps = s.net.tx_bps;
    out.disk_read_bps = s.disk.read_bps;
    out.disk_write_bps = s.disk.write_bps;
    out.battery_present = if s.battery.present { 1 } else { 0 };
    out.battery_charging = if s.battery.is_charging { 1 } else { 0 };
    out.battery_pct = s.battery.percent;
    out.cpu_temp_present = if s.thermal.cpu_c.is_some() { 1 } else { 0 };
    out.cpu_temp_c = s.thermal.cpu_c.unwrap_or(0.0);
    out.gpu_temp_present = if s.thermal.gpu_c.is_some() { 1 } else { 0 };
    out.gpu_temp_c = s.thermal.gpu_c.unwrap_or(0.0);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_latest_stop_round_trip() {
        let h = monitor_rs_start();
        assert!(!h.is_null());

        // Sampler needs ~one tick before latest() returns a sample.
        // CpuSampler::new() sleeps ~210ms to prime; run_loop then waits one
        // full 1-second interval before the first tick, so we need >1.2 s.
        std::thread::sleep(std::time::Duration::from_millis(2000));

        let mut out: MrsSample = unsafe { std::mem::zeroed() };
        let got = unsafe { monitor_rs_latest(h, &mut out) };
        assert_eq!(got, 1);
        assert!(out.cpu_total_pct >= 0.0 && out.cpu_total_pct <= 100.0);
        assert!(out.core_count >= 1);
        assert!(out.mem_total_bytes > 0);

        unsafe { monitor_rs_stop(h) };
    }

    #[test]
    fn start_latest_yields_plausible_new_metrics() {
        let h = monitor_rs_start();
        assert!(!h.is_null());
        std::thread::sleep(std::time::Duration::from_millis(2500));

        let mut sample: MrsSample = unsafe { std::mem::zeroed() };
        let ok = unsafe { monitor_rs_latest(h, &mut sample) };
        assert_eq!(ok, 1);

        // Net/disk are non-negative by construction (u64).
        // Battery percent (when present) is bounded.
        if sample.battery_present == 1 {
            assert!(
                sample.battery_pct >= 0.0 && sample.battery_pct <= 100.0,
                "battery_pct out of range: {}", sample.battery_pct,
            );
        }
        if sample.cpu_temp_present == 1 {
            assert!(
                sample.cpu_temp_c > 0.0 && sample.cpu_temp_c < 130.0,
                "cpu_temp_c implausible: {}", sample.cpu_temp_c,
            );
        }
        if sample.gpu_temp_present == 1 {
            assert!(
                sample.gpu_temp_c > 0.0 && sample.gpu_temp_c < 130.0,
                "gpu_temp_c implausible: {}", sample.gpu_temp_c,
            );
        }
        unsafe { monitor_rs_stop(h) };
    }

    #[test]
    fn null_handle_returns_zero() {
        let mut out: MrsSample = unsafe { std::mem::zeroed() };
        assert_eq!(unsafe { monitor_rs_latest(std::ptr::null_mut(), &mut out) }, 0);
        assert_eq!(unsafe { monitor_rs_recent(std::ptr::null_mut(), 5, &mut out) }, 0);
        unsafe { monitor_rs_stop(std::ptr::null_mut()) }; // must not crash
    }

    #[test]
    fn settings_round_trip() {
        let h = monitor_rs_start();
        let json_ptr = unsafe { monitor_rs_settings_get(h) };
        assert!(!json_ptr.is_null());
        let json = unsafe { CStr::from_ptr(json_ptr).to_str().unwrap().to_string() };
        unsafe { monitor_rs_string_free(json_ptr) };
        assert!(json.contains("sample_rate_hz"));

        // Set the same JSON back — should succeed.
        let cstring = CString::new(json).unwrap();
        let rc = unsafe { monitor_rs_settings_set(h, cstring.as_ptr()) };
        assert_eq!(rc, 1);

        unsafe { monitor_rs_stop(h) };
    }
}
