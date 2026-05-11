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

#[derive(Debug, Clone)]
pub struct Sample {
    pub ts: Instant,
    pub cpu_total: f32,
    pub cpu_per_core: Vec<f32>,
    pub gpu_pct: Option<f32>,
    pub mem: MemInfo,
    pub swap: SwapInfo,
    pub top_procs: Vec<ProcInfo>,
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
