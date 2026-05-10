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
        let used = self.sys.used_memory();
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
/// Apple's exact formula isn't public; this approximation uses the used/total
/// ratio at the published thresholds (Normal < 70%, Warning < 90%, else Critical).
pub fn classify_pressure(used: u64, total: u64) -> MemPressure {
    if total == 0 { return MemPressure::Normal; }
    let r = used as f64 / total as f64;
    if r < 0.70 { MemPressure::Normal }
    else if r < 0.90 { MemPressure::Warning }
    else { MemPressure::Critical }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_thresholds() {
        assert_eq!(classify_pressure(0, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(50, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(80, 100), MemPressure::Warning);
        assert_eq!(classify_pressure(95, 100), MemPressure::Critical);
        assert_eq!(classify_pressure(0, 0), MemPressure::Normal);
    }

    #[test]
    fn mem_sampler_returns_sane_values() {
        let mut s = MemSampler::new();
        let r = s.tick().expect("tick succeeds");
        assert!(r.mem.total_bytes > 0);
        assert!(r.mem.used_bytes <= r.mem.total_bytes);
        assert!(r.swap.used_bytes <= r.swap.total_bytes.max(r.swap.used_bytes));
    }
}
