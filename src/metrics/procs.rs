use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use super::MetricError;
use crate::sample::ProcInfo;

/// One snapshot, multiple ranking orders. Each `Vec` is already truncated
/// to `top_n`. The popover picks which list to display based on which
/// metric is currently hero.
pub struct TopProcs {
    pub by_cpu: Vec<ProcInfo>,
    pub by_mem: Vec<ProcInfo>,
}

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

    pub fn tick(&mut self) -> Result<TopProcs, MetricError> {
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        let all: Vec<ProcInfo> = self.sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_pct: p.cpu_usage(),
                rss_bytes: p.memory(),
            })
            .collect();

        // Two independently-sorted views of the same snapshot. Cloning here
        // is cheap (one Vec<ProcInfo>, ~hundreds of entries) and keeps the
        // function pure-by-construction.
        let mut by_cpu = all.clone();
        by_cpu.sort_by(|a, b| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.rss_bytes.cmp(&a.rss_bytes))
        });
        by_cpu.truncate(self.top_n);

        let mut by_mem = all;
        by_mem.sort_by(|a, b| {
            b.rss_bytes
                .cmp(&a.rss_bytes)
                .then_with(|| {
                    b.cpu_pct
                        .partial_cmp(&a.cpu_pct)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        by_mem.truncate(self.top_n);

        Ok(TopProcs { by_cpu, by_mem })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_sampler_respects_top_n() {
        let mut s = ProcSampler::new(3);
        let r = s.tick().expect("tick succeeds");
        assert!(r.by_cpu.len() <= 3);
        assert!(r.by_mem.len() <= 3);
        assert!(!r.by_cpu.is_empty());
        assert!(!r.by_mem.is_empty());
    }

    #[test]
    fn proc_sampler_by_cpu_sorted_desc() {
        let mut s = ProcSampler::new(20);
        let r = s.tick().expect("tick succeeds");
        for w in r.by_cpu.windows(2) {
            assert!(w[0].cpu_pct >= w[1].cpu_pct);
        }
    }

    #[test]
    fn proc_sampler_by_mem_sorted_desc() {
        let mut s = ProcSampler::new(20);
        let r = s.tick().expect("tick succeeds");
        for w in r.by_mem.windows(2) {
            assert!(w[0].rss_bytes >= w[1].rss_bytes);
        }
    }

}
