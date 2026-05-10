use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use super::MetricError;
use crate::sample::ProcInfo;

pub struct ProcSampler {
    sys: System,
    top_n: usize,
}

impl ProcSampler {
    pub fn new(top_n: usize) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::nothing().with_processes(ProcessRefreshKind::everything()),
        );
        Self { sys, top_n: top_n.max(1) }
    }

    pub fn tick(&mut self) -> Result<Vec<ProcInfo>, MetricError> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let mut all: Vec<ProcInfo> = self.sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_pct: p.cpu_usage(),
                rss_bytes: p.memory(),
            })
            .collect();
        // Rank by CPU then by RSS as tiebreaker.
        all.sort_by(|a, b| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.rss_bytes.cmp(&a.rss_bytes))
        });
        all.truncate(self.top_n);
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_sampler_respects_top_n() {
        let mut s = ProcSampler::new(3);
        let r = s.tick().expect("tick succeeds");
        assert!(r.len() <= 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn proc_sampler_sorted_desc_by_cpu() {
        let mut s = ProcSampler::new(20);
        let r = s.tick().expect("tick succeeds");
        for w in r.windows(2) {
            assert!(w[0].cpu_pct >= w[1].cpu_pct);
        }
    }
}
