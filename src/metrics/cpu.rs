use sysinfo::{CpuRefreshKind, RefreshKind, System};

use super::MetricError;

pub struct CpuReading {
    pub total_pct: f32,
    pub per_core_pct: Vec<f32>,
}

pub struct CpuSampler {
    sys: System,
}

impl CpuSampler {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
        );
        // Prime the sampler — first read is meaningless.
        sys.refresh_cpu_usage();
        std::thread::sleep(std::time::Duration::from_millis(
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_millis() as u64 + 10,
        ));
        sys.refresh_cpu_usage();
        Self { sys }
    }

    pub fn tick(&mut self) -> Result<CpuReading, MetricError> {
        self.sys.refresh_cpu_usage();
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return Err(MetricError::Unavailable("no CPUs reported".into()));
        }
        let per_core: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
        let total = per_core.iter().sum::<f32>() / per_core.len() as f32;
        Ok(CpuReading { total_pct: total, per_core_pct: per_core })
    }
}

impl Default for CpuSampler {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_sampler_returns_sane_values() {
        let mut s = CpuSampler::new();
        let r = s.tick().expect("tick succeeds");
        assert!(!r.per_core_pct.is_empty());
        assert!(r.total_pct >= 0.0 && r.total_pct <= 100.0);
        for p in &r.per_core_pct {
            assert!(*p >= 0.0 && *p <= 100.0, "core pct out of range: {p}");
        }
    }
}
