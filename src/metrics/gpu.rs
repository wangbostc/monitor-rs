//! GPU utilization on Apple Silicon via the private IOReport framework.
//!
//! This binds the minimal subset of IOReport.framework needed to read the
//! GPU performance-state channels. The framework lives at
//! `/System/Library/PrivateFrameworks/IOReport.framework` and isn't in the
//! standard linker search path, so we dlopen it via `libloading`. All FFI
//! errors degrade to `Ok(None)`.
//!
//! The technique mirrors what `macmon` / `asitop` / `mactop` do: subscribe to
//! the `("GPU Stats", "GPU Performance States")` channels, then for each
//! sample tick walk the per-state residency tuples. States named `IDLE`,
//! `DOWN`, or `OFF` are non-active; the rest are active. Utilization is
//! `active / (active + idle)`.

use std::ffi::c_void;

use core_foundation::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation::base::TCFType;
use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation::string::{CFString, CFStringGetCString, CFStringRef, kCFStringEncodingUTF8};
use libloading::{Library, Symbol};

use super::MetricError;

/// Candidate paths for the IOReport dylib. macOS layouts have shifted across
/// versions: modern macOS (≥ 13) ships it as `/usr/lib/libIOReport.dylib`,
/// older systems (Big Sur era) had it as a framework binary inside
/// `/System/Library/PrivateFrameworks/IOReport.framework/...`. We try them
/// in order. Anything that lives in the dyld shared cache will resolve via
/// `dlopen` even if there's no on-disk file.
const IOREPORT_PATHS: &[&str] = &[
    "/usr/lib/libIOReport.dylib",
    "/System/Library/PrivateFrameworks/IOReport.framework/Versions/A/IOReport",
    "/System/Library/PrivateFrameworks/IOReport.framework/IOReport",
];

type IOReportCopyChannelsInGroupFn = unsafe extern "C" fn(
    group: CFStringRef,
    subgroup: CFStringRef,
    a: u64,
    b: u64,
    c: u64,
) -> CFDictionaryRef;

type IOReportCreateSubscriptionFn = unsafe extern "C" fn(
    a: *const c_void,
    desired_channels: CFDictionaryRef,
    out_subbed: *mut CFDictionaryRef,
    b: u64,
    options: CFTypeRef,
) -> CFTypeRef;

type IOReportCreateSamplesFn = unsafe extern "C" fn(
    subscription: CFTypeRef,
    subscribed_channels: CFDictionaryRef,
    options: CFTypeRef,
) -> CFDictionaryRef;

type IOReportCreateSamplesDeltaFn = unsafe extern "C" fn(
    prev: CFDictionaryRef,
    curr: CFDictionaryRef,
    options: CFTypeRef,
) -> CFDictionaryRef;

type IOReportChannelGetGroupFn = unsafe extern "C" fn(channel: CFDictionaryRef) -> CFStringRef;
type IOReportChannelGetSubGroupFn = unsafe extern "C" fn(channel: CFDictionaryRef) -> CFStringRef;
type IOReportChannelGetChannelNameFn =
    unsafe extern "C" fn(channel: CFDictionaryRef) -> CFStringRef;
type IOReportStateGetCountFn = unsafe extern "C" fn(channel: CFDictionaryRef) -> i32;
type IOReportStateGetNameForIndexFn =
    unsafe extern "C" fn(channel: CFDictionaryRef, idx: i32) -> CFStringRef;
type IOReportStateGetResidencyFn =
    unsafe extern "C" fn(channel: CFDictionaryRef, idx: i32) -> i64;

pub struct GpuSampler {
    inner: Option<Inner>,
}

/// Resolved IOReport entry-points. We keep these alongside the `Library` so
/// that drop order tears the function pointers down before unloading the lib.
struct Funcs {
    create_samples: IOReportCreateSamplesFn,
    create_delta: IOReportCreateSamplesDeltaFn,
    channel_get_group: IOReportChannelGetGroupFn,
    channel_get_subgroup: IOReportChannelGetSubGroupFn,
    channel_get_channel_name: IOReportChannelGetChannelNameFn,
    state_get_count: IOReportStateGetCountFn,
    state_get_name_for_index: IOReportStateGetNameForIndexFn,
    state_get_residency: IOReportStateGetResidencyFn,
}

struct Inner {
    // SAFETY: must outlive every function pointer derived from it. Drop order
    // is field-declaration order, so put _lib *last* so functions tear down
    // first.
    funcs: Funcs,
    subscription: CFTypeRef,
    subscribed_channels: CFDictionaryRef,
    last_sample: Option<CFDictionaryRef>,
    _lib: Library,
}

// SAFETY: `Inner` is owned exclusively by a single `GpuSampler`. The CF refs and
// function pointers it holds are never aliased across threads, and CoreFoundation
// objects are safe to use from a single thread at a time. The sampler thread
// will own the GpuSampler exclusively while ticking.
unsafe impl Send for GpuSampler {}

impl GpuSampler {
    pub fn new() -> Self {
        match unsafe { Self::try_init() } {
            Ok(inner) => Self { inner: Some(inner) },
            Err(e) => {
                tracing::warn!("GPU sampler unavailable: {e}");
                Self { inner: None }
            }
        }
    }

    /// Returns `Ok(None)` if GPU sampling isn't supported on this machine
    /// (Intel Mac, future macOS, private-framework break, etc.) or if this is
    /// the first tick (we need a previous sample to compute deltas).
    pub fn tick(&mut self) -> Result<Option<f32>, MetricError> {
        let Some(inner) = self.inner.as_mut() else {
            return Ok(None);
        };
        unsafe { inner.tick() }
    }

    unsafe fn try_init() -> Result<Inner, MetricError> {
        let mut last_err: Option<String> = None;
        let lib = IOREPORT_PATHS
            .iter()
            .find_map(|p| match unsafe { Library::new(*p) } {
                Ok(l) => Some(l),
                Err(e) => {
                    last_err = Some(format!("{p}: {e}"));
                    None
                }
            })
            .ok_or_else(|| {
                MetricError::Ffi(format!(
                    "dlopen IOReport: {}",
                    last_err.unwrap_or_else(|| "no candidate paths".into())
                ))
            })?;

        // Resolve all symbols up front. We immediately copy the function
        // pointers out of the `Symbol` wrappers; the lib itself stays owned
        // by `Inner` so the pointers remain valid.
        unsafe {
            let copy_channels: Symbol<IOReportCopyChannelsInGroupFn> = lib
                .get(b"IOReportCopyChannelsInGroup\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CopyChannels: {e}")))?;
            let create_subscription: Symbol<IOReportCreateSubscriptionFn> = lib
                .get(b"IOReportCreateSubscription\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSubscription: {e}")))?;
            let create_samples: Symbol<IOReportCreateSamplesFn> = lib
                .get(b"IOReportCreateSamples\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSamples: {e}")))?;
            let create_delta: Symbol<IOReportCreateSamplesDeltaFn> = lib
                .get(b"IOReportCreateSamplesDelta\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSamplesDelta: {e}")))?;
            let channel_get_group: Symbol<IOReportChannelGetGroupFn> = lib
                .get(b"IOReportChannelGetGroup\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym ChannelGetGroup: {e}")))?;
            let channel_get_subgroup: Symbol<IOReportChannelGetSubGroupFn> = lib
                .get(b"IOReportChannelGetSubGroup\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym ChannelGetSubGroup: {e}")))?;
            let channel_get_channel_name: Symbol<IOReportChannelGetChannelNameFn> = lib
                .get(b"IOReportChannelGetChannelName\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym ChannelGetChannelName: {e}")))?;
            let state_get_count: Symbol<IOReportStateGetCountFn> = lib
                .get(b"IOReportStateGetCount\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym StateGetCount: {e}")))?;
            let state_get_name_for_index: Symbol<IOReportStateGetNameForIndexFn> = lib
                .get(b"IOReportStateGetNameForIndex\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym StateGetNameForIndex: {e}")))?;
            let state_get_residency: Symbol<IOReportStateGetResidencyFn> = lib
                .get(b"IOReportStateGetResidency\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym StateGetResidency: {e}")))?;

            // macmon's chip-specific subgroup. "GPU PMU" exists too but the
            // residency states we need live in "GPU Performance States".
            let group = CFString::new("GPU Stats");
            let subgroup = CFString::new("GPU Performance States");
            let desired = copy_channels(
                group.as_concrete_TypeRef(),
                subgroup.as_concrete_TypeRef(),
                0,
                0,
                0,
            );
            if desired.is_null() {
                return Err(MetricError::Unavailable("no GPU channels".into()));
            }

            let mut subbed: CFDictionaryRef = std::ptr::null();
            let subscription = create_subscription(
                std::ptr::null(),
                desired,
                &mut subbed as *mut _,
                0,
                std::ptr::null(),
            );
            if subscription.is_null() || subbed.is_null() {
                if !subscription.is_null() { CFRelease(subscription); }
                if !subbed.is_null() { CFRelease(subbed as CFTypeRef); }
                CFRelease(desired as CFTypeRef);
                return Err(MetricError::Unavailable("subscription failed".into()));
            }

            // The desired-channels dict was a template; once we've received
            // the subscribed-channels dict via the out-param we can release
            // the template.
            CFRelease(desired as CFTypeRef);

            // Pull raw fn pointers out of the Symbol wrappers. They remain
            // valid as long as `lib` is alive, which is for the lifetime of
            // `Inner`.
            let funcs = Funcs {
                create_samples: *create_samples,
                create_delta: *create_delta,
                channel_get_group: *channel_get_group,
                channel_get_subgroup: *channel_get_subgroup,
                channel_get_channel_name: *channel_get_channel_name,
                state_get_count: *state_get_count,
                state_get_name_for_index: *state_get_name_for_index,
                state_get_residency: *state_get_residency,
            };

            Ok(Inner {
                funcs,
                subscription,
                subscribed_channels: subbed,
                last_sample: None,
                _lib: lib,
            })
        }
    }
}

impl Default for GpuSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            if let Some(s) = self.last_sample.take() {
                CFRelease(s as CFTypeRef);
            }
            if !self.subscribed_channels.is_null() {
                CFRelease(self.subscribed_channels as CFTypeRef);
            }
            if !self.subscription.is_null() {
                CFRelease(self.subscription);
            }
        }
    }
}

impl Inner {
    unsafe fn tick(&mut self) -> Result<Option<f32>, MetricError> {
        let curr =
            unsafe { (self.funcs.create_samples)(self.subscription, self.subscribed_channels, std::ptr::null()) };
        if curr.is_null() {
            return Ok(None);
        }

        let pct = if let Some(prev) = self.last_sample {
            let delta = unsafe { (self.funcs.create_delta)(prev, curr, std::ptr::null()) };
            unsafe { CFRelease(prev as CFTypeRef) };
            self.last_sample = Some(curr);
            if delta.is_null() {
                None
            } else {
                let pct = unsafe { compute_idle_complement(delta, &self.funcs) };
                unsafe { CFRelease(delta as CFTypeRef) };
                pct
            }
        } else {
            // First call — stash the sample and return None. Caller is
            // expected to tick again to get a real reading.
            self.last_sample = Some(curr);
            None
        };

        Ok(pct)
    }
}

/// Walk the IOReport delta-sample dict and compute `1 - idle_residency` over
/// the GPU performance states. Returns a percentage in `[0.0, 100.0]`, or
/// `None` if the data shape doesn't match what we expect (e.g. future macOS
/// changed the keys).
unsafe fn compute_idle_complement(
    delta_dict: CFDictionaryRef,
    funcs: &Funcs,
) -> Option<f32> {
    if delta_dict.is_null() {
        return None;
    }

    // Pull the IOReportChannels CFArray out of the delta dict.
    let key = CFString::new("IOReportChannels");
    let channels_ref = unsafe { CFDictionaryGetValue(delta_dict, key.as_concrete_TypeRef() as _) };
    if channels_ref.is_null() {
        return None;
    }
    let channels = channels_ref as CFArrayRef;
    let count = unsafe { CFArrayGetCount(channels) };

    let mut active_total: i64 = 0;
    let mut grand_total: i64 = 0;

    for i in 0..count {
        let item = unsafe { CFArrayGetValueAtIndex(channels, i) } as CFDictionaryRef;
        if item.is_null() {
            continue;
        }

        // Filter to the GPU Performance States subgroup. macmon uses
        // channel name == "GPUPH" to select the residency channel; we accept
        // anything in this subgroup since IOReportCopyChannelsInGroup
        // already narrowed it down.
        let group = unsafe { (funcs.channel_get_group)(item) };
        let subgroup = unsafe { (funcs.channel_get_subgroup)(item) };
        if !cfstr_eq(group, "GPU Stats") {
            continue;
        }
        if !cfstr_eq(subgroup, "GPU Performance States") {
            continue;
        }

        // GPUPH is the channel name macmon keys on. If it's not present we
        // still walk what we have; in practice there's typically one channel
        // here on Apple Silicon.
        let _channel_name = unsafe { (funcs.channel_get_channel_name)(item) };

        let n_states = unsafe { (funcs.state_get_count)(item) };
        if n_states <= 0 {
            continue;
        }

        for s in 0..n_states {
            let name = unsafe { (funcs.state_get_name_for_index)(item, s) };
            let residency = unsafe { (funcs.state_get_residency)(item, s) };
            if residency < 0 {
                continue;
            }
            grand_total = grand_total.saturating_add(residency);
            // Idle states macmon recognises: "IDLE", "DOWN", "OFF".
            let is_idle = cfstr_eq(name, "IDLE") || cfstr_eq(name, "DOWN") || cfstr_eq(name, "OFF");
            if !is_idle {
                active_total = active_total.saturating_add(residency);
            }
        }
    }

    if grand_total <= 0 {
        return None;
    }
    let frac = active_total as f64 / grand_total as f64;
    let pct = (frac * 100.0).clamp(0.0, 100.0) as f32;
    Some(pct)
}

/// Compare a CFString to a Rust `&str` without allocating a `CFString` on the
/// other side. We pull the UTF-8 bytes out into a stack buffer.
fn cfstr_eq(s: CFStringRef, want: &str) -> bool {
    if s.is_null() {
        return false;
    }
    let mut buf = [0i8; 128];
    let ok = unsafe {
        CFStringGetCString(s, buf.as_mut_ptr(), buf.len() as isize, kCFStringEncodingUTF8)
    };
    if ok == 0 {
        return false;
    }
    // Find NUL.
    let bytes_u8: &[u8] = unsafe {
        std::slice::from_raw_parts(buf.as_ptr() as *const u8, buf.len())
    };
    let nul = bytes_u8.iter().position(|&b| b == 0).unwrap_or(buf.len());
    bytes_u8[..nul] == *want.as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_sampler_does_not_panic() {
        let mut s = GpuSampler::new();
        let _ = s.tick();
        let _ = s.tick();
    }
}
