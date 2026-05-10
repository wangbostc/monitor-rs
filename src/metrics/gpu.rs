//! GPU utilization on Apple Silicon via the private IOReport framework.
//!
//! This binds the minimal subset of IOReport.framework needed to read the
//! GPU performance-state channels. The framework lives at
//! `/System/Library/PrivateFrameworks/IOReport.framework` and isn't in the
//! standard linker search path, so we dlopen it via `libloading`. All FFI
//! errors degrade to `Ok(None)`.
//!
//! This file is the skeleton: subscription setup + sample bookkeeping. The
//! actual residency walk that turns the delta dict into a percentage lives
//! in `compute_idle_complement` and is currently a stub returning `None`.

use std::ffi::c_void;

use core_foundation::base::TCFType;
use core_foundation::base::{CFRelease, CFTypeRef};
use core_foundation::dictionary::CFDictionaryRef;
use core_foundation::string::{CFString, CFStringRef};
use libloading::{Library, Symbol};

use super::MetricError;

/// Candidate paths for the IOReport dylib. macOS layouts have shifted across
/// versions: modern macOS (>= 13) ships it as `/usr/lib/libIOReport.dylib`,
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

pub struct GpuSampler {
    inner: Option<Inner>,
}

struct Inner {
    // SAFETY: `_lib` must outlive the function pointers below. Drop runs in
    // field-declaration order, so put `_lib` last.
    create_samples: IOReportCreateSamplesFn,
    create_delta: IOReportCreateSamplesDeltaFn,
    subscription: CFTypeRef,
    subscribed_channels: CFDictionaryRef,
    last_sample: Option<CFDictionaryRef>,
    _lib: Library,
}

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
    /// (Intel Mac, future macOS, private-framework break, etc.) or if this
    /// is the first tick (we need a previous sample to compute deltas).
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

            // The "GPU Performance States" subgroup of "GPU Stats" is what
            // macmon / asitop subscribe to for residency-based GPU%.
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
                CFRelease(desired as CFTypeRef);
                return Err(MetricError::Unavailable("subscription failed".into()));
            }

            // Template no longer needed once the subscribed-channels dict is
            // populated.
            CFRelease(desired as CFTypeRef);

            // Pull raw fn pointers out of the Symbol wrappers. They remain
            // valid as long as `lib` is alive (= for the lifetime of `Inner`).
            let create_samples_fn: IOReportCreateSamplesFn = *create_samples;
            let create_delta_fn: IOReportCreateSamplesDeltaFn = *create_delta;

            Ok(Inner {
                create_samples: create_samples_fn,
                create_delta: create_delta_fn,
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
        let curr = unsafe {
            (self.create_samples)(self.subscription, self.subscribed_channels, std::ptr::null())
        };
        if curr.is_null() {
            return Ok(None);
        }

        let pct = if let Some(prev) = self.last_sample {
            let delta = unsafe { (self.create_delta)(prev, curr, std::ptr::null()) };
            unsafe { CFRelease(prev as CFTypeRef) };
            self.last_sample = Some(curr);
            if delta.is_null() {
                None
            } else {
                let pct = unsafe { compute_idle_complement(delta) };
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
/// the GPU performance states. Step-4 work — start at None and adapt the
/// macmon technique. Returning None just means the GPU sparkline shows "n/a".
unsafe fn compute_idle_complement(_delta_dict: CFDictionaryRef) -> Option<f32> {
    None
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
