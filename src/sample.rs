use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemPressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct MemInfo {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub pressure: MemPressure,
}

#[derive(Debug, Clone)]
pub struct SwapInfo {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub rss_bytes: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NetIo {
    pub rx_bps: u64,
    pub tx_bps: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DiskIo {
    pub read_bps: u64,
    pub write_bps: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BatteryInfo {
    pub present: bool,
    pub is_charging: bool,
    pub percent: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ThermalInfo {
    pub cpu_c: Option<f32>,
    pub gpu_c: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub ts: Instant,
    pub cpu_total: f32,
    pub cpu_per_core: Vec<f32>,
    pub gpu_pct: Option<f32>,
    pub mem: MemInfo,
    pub swap: SwapInfo,
    pub top_procs: Vec<ProcInfo>,
    pub net: NetIo,
    pub disk: DiskIo,
    pub battery: BatteryInfo,
    pub thermal: ThermalInfo,
}

impl MemInfo {
    pub fn used_pct(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64 * 100.0) as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_used_pct_handles_zero_total() {
        let m = MemInfo { used_bytes: 0, total_bytes: 0, pressure: MemPressure::Normal };
        assert_eq!(m.used_pct(), 0.0);
    }

    #[test]
    fn mem_used_pct_basic() {
        let m = MemInfo { used_bytes: 50, total_bytes: 100, pressure: MemPressure::Normal };
        assert!((m.used_pct() - 50.0).abs() < 0.01);
    }
}
