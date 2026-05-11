//! Battery information via the IOKit power-sources API.
//! Public API only — no private framework calls.

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::CFString;

use crate::sample::BatteryInfo;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFArrayRef;
    fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFDictionaryRef;
}

pub struct BatterySampler;

impl BatterySampler {
    pub fn new() -> Self { Self }

    pub fn tick(&self) -> BatteryInfo {
        unsafe {
            let blob = IOPSCopyPowerSourcesInfo();
            if blob.is_null() { return BatteryInfo::default(); }
            // Wrap so the blob is released on drop.
            let blob_owned = CFType::wrap_under_create_rule(blob);

            let list_ref = IOPSCopyPowerSourcesList(blob_owned.as_concrete_TypeRef());
            if list_ref.is_null() { return BatteryInfo::default(); }
            let list: CFArray<CFType> = CFArray::wrap_under_create_rule(list_ref);

            for ps in list.iter() {
                let dict_ref = IOPSGetPowerSourceDescription(
                    blob_owned.as_concrete_TypeRef(),
                    ps.as_CFTypeRef(),
                );
                if dict_ref.is_null() { continue; }
                let dict: CFDictionary<CFString, CFType> =
                    CFDictionary::wrap_under_get_rule(dict_ref);

                if let Some(info) = parse_battery_dict(&dict) {
                    return info;
                }
            }
            BatteryInfo::default()
        }
    }
}

impl Default for BatterySampler {
    fn default() -> Self { Self::new() }
}

/// Extract `BatteryInfo` from a power-source description dictionary.
/// Returns `None` if the dict doesn't look like an internal battery.
fn parse_battery_dict(dict: &CFDictionary<CFString, CFType>) -> Option<BatteryInfo> {
    // Keys: see Apple's IOPSKeys.h
    let kind = get_string(dict, "Type")?;
    if kind != "InternalBattery" { return None; }

    let current = get_i64(dict, "Current Capacity").unwrap_or(0);
    let max = get_i64(dict, "Max Capacity").unwrap_or(0);
    let percent = if max > 0 { (current as f32 / max as f32) * 100.0 } else { 0.0 };
    let is_charging = get_bool(dict, "Is Charging").unwrap_or(false);

    Some(BatteryInfo { present: true, is_charging, percent })
}

fn get_string(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<String> {
    let v = dict.find(&CFString::new(key))?;
    let s: CFString = v.downcast::<CFString>()?;
    Some(s.to_string())
}

fn get_i64(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<i64> {
    let v = dict.find(&CFString::new(key))?;
    let n: CFNumber = v.downcast::<CFNumber>()?;
    n.to_i64()
}

fn get_bool(dict: &CFDictionary<CFString, CFType>, key: &str) -> Option<bool> {
    use core_foundation::boolean::CFBoolean;
    let v = dict.find(&CFString::new(key))?;
    let b: CFBoolean = v.downcast::<CFBoolean>()?;
    Some(b.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;

    /// Build a fixture dictionary matching the IOPS description shape and
    /// run the parser against it.
    #[test]
    fn parses_internal_battery() {
        let pairs: Vec<(CFString, CFType)> = vec![
            (CFString::new("Type"), CFString::new("InternalBattery").as_CFType()),
            (CFString::new("Current Capacity"), CFNumber::from(85i64).as_CFType()),
            (CFString::new("Max Capacity"), CFNumber::from(100i64).as_CFType()),
            (CFString::new("Is Charging"), CFBoolean::true_value().as_CFType()),
        ];
        let dict = CFDictionary::from_CFType_pairs(&pairs);
        let info = parse_battery_dict(&dict).expect("battery parsed");
        assert!(info.present);
        assert!(info.is_charging);
        assert!((info.percent - 85.0).abs() < 0.01);
    }

    #[test]
    fn rejects_non_internal() {
        let pairs: Vec<(CFString, CFType)> = vec![
            (CFString::new("Type"), CFString::new("UPS").as_CFType()),
        ];
        let dict = CFDictionary::from_CFType_pairs(&pairs);
        assert!(parse_battery_dict(&dict).is_none());
    }
}
