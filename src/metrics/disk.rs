//! Disk throughput sampler. Reads cumulative read/written byte counters
//! from sysinfo each tick and converts them to bytes-per-second using
//! the previous tick's snapshot.

use std::time::{Duration, Instant};

use sysinfo::Disks;

use crate::sample::DiskIo;

fn rate_bps(prev: u64, current: u64, dt: Duration) -> u64 {
    let secs = dt.as_secs_f64();
    if secs < 1e-6 || current < prev { return 0; }
    (((current - prev) as f64) / secs).round() as u64
}

pub struct DiskSampler {
    disks: Disks,
    last: Option<(u64, u64, Instant)>,  // (read_total, write_total, ts)
}

impl Default for DiskSampler {
    fn default() -> Self { Self::new() }
}

impl DiskSampler {
    pub fn new() -> Self {
        Self {
            disks: Disks::new_with_refreshed_list(),
            last: None,
        }
    }

    pub fn tick(&mut self) -> DiskIo {
        self.disks.refresh(true);
        let mut read_total: u64 = 0;
        let mut write_total: u64 = 0;
        for d in self.disks.iter() {
            let u = d.usage();
            read_total = read_total.saturating_add(u.total_read_bytes);
            write_total = write_total.saturating_add(u.total_written_bytes);
        }
        let now = Instant::now();
        let out = if let Some((pr, pw, pts)) = self.last {
            DiskIo {
                read_bps: rate_bps(pr, read_total, now.duration_since(pts)),
                write_bps: rate_bps(pw, write_total, now.duration_since(pts)),
            }
        } else {
            DiskIo::default()
        };
        self.last = Some((read_total, write_total, now));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_basic() {
        assert_eq!(rate_bps(0, 2_000_000, Duration::from_secs(1)), 2_000_000);
    }

    #[test]
    fn rate_zero_dt_yields_zero() {
        assert_eq!(rate_bps(0, 1, Duration::from_nanos(0)), 0);
    }

    #[test]
    fn rate_regress_yields_zero() {
        assert_eq!(rate_bps(100, 50, Duration::from_secs(1)), 0);
    }

    #[test]
    fn first_tick_returns_zero() {
        let mut s = DiskSampler::new();
        let r = s.tick();
        assert_eq!(r.read_bps, 0);
        assert_eq!(r.write_bps, 0);
    }
}
