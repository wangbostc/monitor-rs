//! CPU/GPU die temperature on Apple Silicon via the private
//! `IOKit/hid/IOHIDEventSystemClient` API.
//!
//! Private framework: sensor names differ per chip generation. The
//! tables below are authoritative for Apple M4; M1-M3 are best-effort
//! and may break on future macOS releases. If enumeration fails we log
//! once at WARN and return `ThermalInfo::default()`.

use std::sync::OnceLock;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};

use libloading::Library;

use crate::sample::ThermalInfo;

// IOHIDEventSystemClient* symbols live inside the IOKit framework binary.
const HID_LIB_CANDIDATES: &[&str] = &[
    "/System/Library/Frameworks/IOKit.framework/IOKit",
    "/System/Library/Frameworks/IOKit.framework/Versions/A/IOKit",
];

type IOHIDEventSystemClientCreateFn = unsafe extern "C" fn(allocator: CFTypeRef) -> CFTypeRef;
type IOHIDEventSystemClientSetMatchingFn =
    unsafe extern "C" fn(client: CFTypeRef, matching: CFDictionaryRef);
type IOHIDEventSystemClientCopyServicesFn =
    unsafe extern "C" fn(client: CFTypeRef) -> CFArrayRef;
type IOHIDServiceClientCopyEventFn = unsafe extern "C" fn(
    service: CFTypeRef,
    event_type: i64,
    options: i32,
    timestamp: u64,
) -> CFTypeRef;
type IOHIDServiceClientCopyPropertyFn =
    unsafe extern "C" fn(service: CFTypeRef, key: CFStringRef) -> CFTypeRef;
type IOHIDEventGetFloatValueFn = unsafe extern "C" fn(event: CFTypeRef, field: i64) -> f64;

const K_IOHID_EVENT_TYPE_TEMPERATURE: i64 = 15;
// IOHIDEventFieldBase(kIOHIDEventTypeTemperature) | 0 — the "level" field of a
// temperature event. The base is (event_type << 16); field index 0 is the value.
const K_IOHID_EVENT_FIELD_TEMP_LEVEL: i64 = K_IOHID_EVENT_TYPE_TEMPERATURE << 16;
// "PrimaryUsage" page=0xff00, usage=0x0005 selects thermal sensors.
const K_HID_PAGE_APPLE_VENDOR: i64 = 0xff00;
const K_HID_USAGE_TEMPERATURE: i64 = 0x0005;

struct Hid {
    #[allow(dead_code)] // keep the library mapped
    lib: Library,
    client_create: IOHIDEventSystemClientCreateFn,
    client_set_matching: IOHIDEventSystemClientSetMatchingFn,
    client_copy_services: IOHIDEventSystemClientCopyServicesFn,
    service_copy_event: IOHIDServiceClientCopyEventFn,
    service_copy_property: IOHIDServiceClientCopyPropertyFn,
    event_get_float: IOHIDEventGetFloatValueFn,
}

static HID: OnceLock<Option<Hid>> = OnceLock::new();
static WARN_LOGGED: OnceLock<()> = OnceLock::new();

fn hid() -> Option<&'static Hid> {
    HID.get_or_init(|| unsafe {
        for path in HID_LIB_CANDIDATES {
            let Ok(lib) = Library::new(*path) else { continue };
            let Ok(c1) = lib.get::<IOHIDEventSystemClientCreateFn>(b"IOHIDEventSystemClientCreate")
            else {
                continue;
            };
            let Ok(c2) = lib
                .get::<IOHIDEventSystemClientSetMatchingFn>(b"IOHIDEventSystemClientSetMatching")
            else {
                continue;
            };
            let Ok(c3) = lib
                .get::<IOHIDEventSystemClientCopyServicesFn>(b"IOHIDEventSystemClientCopyServices")
            else {
                continue;
            };
            let Ok(c4) =
                lib.get::<IOHIDServiceClientCopyEventFn>(b"IOHIDServiceClientCopyEvent")
            else {
                continue;
            };
            let Ok(c5) =
                lib.get::<IOHIDServiceClientCopyPropertyFn>(b"IOHIDServiceClientCopyProperty")
            else {
                continue;
            };
            let Ok(c6) = lib.get::<IOHIDEventGetFloatValueFn>(b"IOHIDEventGetFloatValue") else {
                continue;
            };
            return Some(Hid {
                client_create: *c1,
                client_set_matching: *c2,
                client_copy_services: *c3,
                service_copy_event: *c4,
                service_copy_property: *c5,
                event_get_float: *c6,
                lib,
            });
        }
        None
    })
    .as_ref()
}

/// Best-effort name fragments. A sensor whose `Product` property
/// contains any of these case-insensitive substrings is treated as
/// belonging to that domain. The first match wins per domain per tick.
struct Table {
    cpu: &'static [&'static str],
    gpu: &'static [&'static str],
}

// On Apple M4 (and other M3/M4 generation chips), Apple stopped publishing
// semantic per-block sensor names like `pACC MTR Temp Sensor`. Instead the
// IOHIDEventSystem only exposes generic `PMU tdie<N>` numbered sensors plus
// `PMU tdev<N>` (which return invalid values like -9199 C on this machine
// and are filtered by the value-range guard) plus a few others.
//
// On the M4 base (10-core GPU, 4P+4E CPU) sensor layout that this code was
// tuned against, the lower-numbered `tdie` sensors (1-8) sit physically
// over the CPU clusters and run noticeably hotter under CPU load, while
// the higher-numbered ones (9-14) sit over the GPU clusters. This is a
// heuristic mapping derived empirically from observed temperatures and
// confirmed under controlled load; it is NOT a documented Apple mapping
// and may need adjustment on M4 Pro / M4 Max (more cores) or other SKUs.
//
// The older M1-M3 semantic fragments are kept as a wider fallback in case
// this same code path is exercised on an older Mac. They are harmless when
// they don't match anything.
const TABLE: Table = Table {
    cpu: &[
        // Apple M4 numbered die sensors that sit over the CPU clusters.
        "PMU tdie1",
        "PMU tdie2",
        "PMU tdie3",
        "PMU tdie4",
        "PMU tdie5",
        "PMU tdie6",
        "PMU tdie7",
        "PMU tdie8",
        // M1-M3 semantic fragments kept as a wider fallback.
        "pACC",
        "eACC",
        "pCPU",
        "eCPU",
        "CPU die",
    ],
    gpu: &[
        // Apple M4 numbered die sensors that sit over the GPU clusters.
        "PMU tdie9",
        "PMU tdie10",
        "PMU tdie11",
        "PMU tdie12",
        "PMU tdie13",
        "PMU tdie14",
        // M1-M3 semantic fragments kept as a wider fallback.
        "GPU",
        "AGX",
    ],
};

#[derive(Copy, Clone)]
enum Domain {
    Cpu,
    Gpu,
}

fn classify(name: &str) -> Option<Domain> {
    // Use longest-matching-fragment-wins so that e.g. `PMU tdie11` matches the
    // `PMU tdie11` GPU fragment instead of the shorter `PMU tdie1` CPU prefix.
    let lower = name.to_ascii_lowercase();
    let mut best: Option<(usize, Domain)> = None;
    for needle in TABLE.cpu {
        let n = needle.to_ascii_lowercase();
        if lower.contains(&n) && best.map(|(len, _)| n.len() > len).unwrap_or(true) {
            best = Some((n.len(), Domain::Cpu));
        }
    }
    for needle in TABLE.gpu {
        let n = needle.to_ascii_lowercase();
        if lower.contains(&n) && best.map(|(len, _)| n.len() > len).unwrap_or(true) {
            best = Some((n.len(), Domain::Gpu));
        }
    }
    best.map(|(_, d)| d)
}

pub struct ThermalSampler;

impl ThermalSampler {
    pub fn new() -> Self {
        Self
    }

    pub fn tick(&self) -> ThermalInfo {
        let Some(api) = hid() else {
            if WARN_LOGGED.set(()).is_ok() {
                tracing::warn!(
                    "thermal: IOHIDEventSystemClient unavailable; temperatures disabled"
                );
            }
            return ThermalInfo::default();
        };
        unsafe { read_temps(api) }.unwrap_or_default()
    }
}

impl Default for ThermalSampler {
    fn default() -> Self {
        Self::new()
    }
}

unsafe fn service_product_name(api: &Hid, service: CFTypeRef) -> Option<String> {
    let key = CFString::new("Product");
    let v = unsafe { (api.service_copy_property)(service, key.as_concrete_TypeRef()) };
    if v.is_null() {
        return None;
    }
    let v_owned = unsafe { CFType::wrap_under_create_rule(v) };
    v_owned.downcast::<CFString>().map(|s| s.to_string())
}

fn build_matching_dict() -> CFDictionary<CFString, CFType> {
    let pairs: Vec<(CFString, CFType)> = vec![
        (
            CFString::new("PrimaryUsagePage"),
            CFNumber::from(K_HID_PAGE_APPLE_VENDOR as i32).as_CFType(),
        ),
        (
            CFString::new("PrimaryUsage"),
            CFNumber::from(K_HID_USAGE_TEMPERATURE as i32).as_CFType(),
        ),
    ];
    CFDictionary::from_CFType_pairs(&pairs)
}

unsafe fn read_temps(api: &Hid) -> Option<ThermalInfo> {
    let client = unsafe { (api.client_create)(std::ptr::null()) };
    if client.is_null() {
        return None;
    }
    let client_owned = unsafe { CFType::wrap_under_create_rule(client) };

    let matching = build_matching_dict();
    unsafe {
        (api.client_set_matching)(
            client_owned.as_concrete_TypeRef(),
            matching.as_concrete_TypeRef(),
        )
    };

    let services_ref = unsafe { (api.client_copy_services)(client_owned.as_concrete_TypeRef()) };
    if services_ref.is_null() {
        return None;
    }
    let services: CFArray<CFType> = unsafe { CFArray::wrap_under_create_rule(services_ref) };

    let mut cpu_sum = 0.0f64;
    let mut cpu_count = 0u32;
    let mut gpu_sum = 0.0f64;
    let mut gpu_count = 0u32;

    for service in services.iter() {
        let Some(name) = (unsafe { service_product_name(api, service.as_CFTypeRef()) }) else {
            continue;
        };
        let Some(domain) = classify(&name) else { continue };

        let event = unsafe {
            (api.service_copy_event)(
                service.as_CFTypeRef(),
                K_IOHID_EVENT_TYPE_TEMPERATURE,
                0,
                0,
            )
        };
        if event.is_null() {
            continue;
        }
        let event_owned = unsafe { CFType::wrap_under_create_rule(event) };
        let value = unsafe {
            (api.event_get_float)(
                event_owned.as_concrete_TypeRef(),
                K_IOHID_EVENT_FIELD_TEMP_LEVEL,
            )
        };
        if !value.is_finite() || !(-20.0..=130.0).contains(&value) {
            continue;
        }

        match domain {
            Domain::Cpu => {
                cpu_sum += value;
                cpu_count += 1;
            }
            Domain::Gpu => {
                gpu_sum += value;
                gpu_count += 1;
            }
        }
    }

    Some(ThermalInfo {
        cpu_c: if cpu_count > 0 {
            Some((cpu_sum / cpu_count as f64) as f32)
        } else {
            None
        },
        gpu_c: if gpu_count > 0 {
            Some((gpu_sum / gpu_count as f64) as f32)
        } else {
            None
        },
    })
}

/// Enumerate every thermal sensor `IOHIDEventSystemClient` exposes and
/// print its Product name and current temperature. Used by
/// `examples/list_thermal_sensors` for one-off discovery -- call
/// `crate::__example_dump_thermal_sensors()` from the example.
pub fn dump_all() {
    let Some(api) = hid() else {
        eprintln!("IOHIDEventSystemClient unavailable");
        return;
    };
    unsafe {
        let client = (api.client_create)(std::ptr::null());
        if client.is_null() {
            eprintln!("client create failed");
            return;
        }
        let client_owned = CFType::wrap_under_create_rule(client);
        let matching = build_matching_dict();
        (api.client_set_matching)(
            client_owned.as_concrete_TypeRef(),
            matching.as_concrete_TypeRef(),
        );
        let services_ref = (api.client_copy_services)(client_owned.as_concrete_TypeRef());
        if services_ref.is_null() {
            eprintln!("no services");
            return;
        }
        let services: CFArray<CFType> = CFArray::wrap_under_create_rule(services_ref);
        println!("Found {} thermal sensor services", services.len());
        for service in services.iter() {
            let Some(name) = service_product_name(api, service.as_CFTypeRef()) else {
                continue;
            };
            let event = (api.service_copy_event)(
                service.as_CFTypeRef(),
                K_IOHID_EVENT_TYPE_TEMPERATURE,
                0,
                0,
            );
            let value = if event.is_null() {
                f64::NAN
            } else {
                let e = CFType::wrap_under_create_rule(event);
                (api.event_get_float)(e.as_concrete_TypeRef(), K_IOHID_EVENT_FIELD_TEMP_LEVEL)
            };
            let domain = classify(&name)
                .map(|d| match d {
                    Domain::Cpu => "cpu",
                    Domain::Gpu => "gpu",
                })
                .unwrap_or("---");
            println!("{name:<40} {value:6.2} C  [{domain}]");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cpu_names() {
        // M1-M3 legacy fragments.
        assert!(matches!(classify("pACC MTR Temp Sensor"), Some(Domain::Cpu)));
        assert!(matches!(classify("eCPU Temperature"), Some(Domain::Cpu)));
        // M4 numbered die sensors mapped to CPU clusters.
        assert!(matches!(classify("PMU tdie1"), Some(Domain::Cpu)));
        assert!(matches!(classify("PMU tdie4"), Some(Domain::Cpu)));
        assert!(matches!(classify("PMU tdie8"), Some(Domain::Cpu)));
    }

    #[test]
    fn classify_gpu_names() {
        // M1-M3 legacy fragments.
        assert!(matches!(classify("GPU0 Temp"), Some(Domain::Gpu)));
        assert!(matches!(classify("AGX Sensor"), Some(Domain::Gpu)));
        // M4 numbered die sensors mapped to GPU clusters.
        assert!(matches!(classify("PMU tdie9"), Some(Domain::Gpu)));
        assert!(matches!(classify("PMU tdie14"), Some(Domain::Gpu)));
    }

    #[test]
    fn classify_disambiguates_overlapping_numbers() {
        // `PMU tdie11` contains the substring `PMU tdie1`. The longest-match
        // tie-breaker must pick the more specific GPU fragment, not the
        // shorter CPU prefix.
        assert!(matches!(classify("PMU tdie11"), Some(Domain::Gpu)));
        assert!(matches!(classify("PMU tdie12"), Some(Domain::Gpu)));
        assert!(matches!(classify("PMU tdie13"), Some(Domain::Gpu)));
    }

    #[test]
    fn classify_unknown_returns_none() {
        assert!(classify("PMU tcal").is_none());
        assert!(classify("NAND CH0 temp").is_none());
        assert!(classify("gas gauge battery").is_none());
        // PMU tdev sensors return -9199 C on M4 and aren't physically
        // mappable; we don't classify them.
        assert!(classify("PMU tdev1").is_none());
    }
}
