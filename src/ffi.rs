//! C-compatible FFI surface for the Swift side. Every function catches
//! panics and returns a safe sentinel on failure. All pointers in the
//! signatures are owned by the Rust side except where noted.

#![cfg(target_os = "macos")]

use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Arc;

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
}

pub struct MrsHandle {
    sampler: SamplerHandle,
    store: Arc<RwLock<SampleStore>>,
    start: std::time::Instant,
}

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_start() -> *mut MrsHandle {
    let r = catch_unwind(|| {
        let start = std::time::Instant::now();
        let settings = Settings::load();
        let sampler = SamplerHandle::spawn(settings);
        let store = sampler.store.clone();
        Box::into_raw(Box::new(MrsHandle {
            sampler,
            store,
            start,
        }))
    });
    r.unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_stop(h: *mut MrsHandle) {
    if h.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let boxed = unsafe { Box::from_raw(h) };
        // SamplerHandle's Drop runs here: stops the thread and joins.
        drop(boxed);
    }));
}

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_latest(h: *mut MrsHandle, out: *mut MrsSample) -> c_int {
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

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_recent(h: *mut MrsHandle, n: usize, out: *mut MrsSample) -> usize {
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

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_settings_get(h: *mut MrsHandle) -> *const c_char {
    if h.is_null() {
        return ptr::null();
    }
    let r = catch_unwind(AssertUnwindSafe(|| {
        let settings = Settings::load();
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());
        let cstring = CString::new(json).unwrap_or_else(|_| CString::new("{}").unwrap());
        cstring.into_raw() as *const c_char
    }));
    r.unwrap_or(ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_settings_set(_h: *mut MrsHandle, json: *const c_char) -> c_int {
    if json.is_null() {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let cstr = CStr::from_ptr(json);
        let Ok(s) = cstr.to_str() else { return 0 };
        let Ok(settings) = serde_json::from_str::<Settings>(s) else { return 0 };
        if settings.save().is_err() { return 0 }
        1
    }));
    r.unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn monitor_rs_string_free(s: *const c_char) {
    if s.is_null() { return; }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        // Reconstruct the CString to drop it.
        let _ = CString::from_raw(s as *mut c_char);
    }));
}

fn sample_to_c(s: &Sample, start: std::time::Instant) -> MrsSample {
    let mut out = MrsSample {
        ts_seconds: s.ts.checked_duration_since(start).unwrap_or_default().as_secs_f64(),
        cpu_total_pct: s.cpu_total,
        core_count: s.cpu_per_core.len().min(MRS_MAX_CORES) as u8,
        cpu_per_core_pct: [0.0; MRS_MAX_CORES],
        gpu_present: if s.gpu_pct.is_some() { 1 } else { 0 },
        gpu_pct: s.gpu_pct.unwrap_or(0.0),
        mem_used_bytes: s.mem.used_bytes,
        mem_total_bytes: s.mem.total_bytes,
        mem_pressure: match s.mem.pressure {
            MemPressure::Normal => 0,
            MemPressure::Warning => 1,
            MemPressure::Critical => 2,
        },
        swap_used_bytes: s.swap.used_bytes,
        swap_total_bytes: s.swap.total_bytes,
        proc_count: s.top_procs.len().min(MRS_MAX_PROCS) as u8,
        procs: [MrsProcInfo {
            pid: 0,
            name: [0; MRS_PROC_NAME],
            cpu_pct: 0.0,
            rss_bytes: 0,
        }; MRS_MAX_PROCS],
    };

    for (dst, src) in out.cpu_per_core_pct.iter_mut().zip(s.cpu_per_core.iter()) {
        *dst = *src;
    }
    for (dst, src) in out.procs.iter_mut().zip(s.top_procs.iter()) {
        dst.pid = src.pid;
        dst.cpu_pct = src.cpu_pct;
        dst.rss_bytes = src.rss_bytes;
        // Truncate name to NAME-1 bytes, NUL-terminate.
        let max = MRS_PROC_NAME - 1;
        let bytes = src.name.as_bytes();
        let n = bytes.len().min(max);
        for i in 0..n {
            dst.name[i] = bytes[i] as c_char;
        }
        // remaining bytes are already 0 (default), so NUL termination is implicit
    }

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
        let got = monitor_rs_latest(h, &mut out);
        assert_eq!(got, 1);
        assert!(out.cpu_total_pct >= 0.0 && out.cpu_total_pct <= 100.0);
        assert!(out.core_count >= 1);
        assert!(out.mem_total_bytes > 0);

        monitor_rs_stop(h);
    }

    #[test]
    fn null_handle_returns_zero() {
        let mut out: MrsSample = unsafe { std::mem::zeroed() };
        assert_eq!(monitor_rs_latest(std::ptr::null_mut(), &mut out), 0);
        assert_eq!(monitor_rs_recent(std::ptr::null_mut(), 5, &mut out), 0);
        monitor_rs_stop(std::ptr::null_mut()); // must not crash
    }

    #[test]
    fn settings_round_trip() {
        let h = monitor_rs_start();
        let json_ptr = monitor_rs_settings_get(h);
        assert!(!json_ptr.is_null());
        let json = unsafe { CStr::from_ptr(json_ptr).to_str().unwrap().to_string() };
        monitor_rs_string_free(json_ptr);
        assert!(json.contains("sample_rate_hz"));

        // Set the same JSON back — should succeed.
        let cstring = CString::new(json).unwrap();
        let rc = monitor_rs_settings_set(h, cstring.as_ptr());
        assert_eq!(rc, 1);

        monitor_rs_stop(h);
    }
}
