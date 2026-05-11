//! Network throughput sampler. Reads cumulative rx/tx byte counters from
//! sysinfo each tick and converts them to bytes-per-second using the
//! previous tick's snapshot.

use std::time::{Duration, Instant};

use sysinfo::Networks;

use crate::sample::NetIo;

/// Compute byte-per-second rate from two cumulative readings.
///
/// - Returns 0 if `dt` is zero or near-zero (avoids div-by-zero on fast ticks).
/// - Returns 0 if `current < prev` (counter regress: interface reset / wrap).
pub(crate) fn rate_bps(prev: u64, current: u64, dt: Duration) -> u64 {
    let secs = dt.as_secs_f64();
    if secs < 1e-6 || current < prev { return 0; }
    let delta = current - prev;
    ((delta as f64) / secs).round() as u64
}

/// Returns true if `name` looks like a macOS pseudo-interface that does
/// not carry real off-host traffic (loopback, VPN tunnels, AirDrop, etc.).
fn is_pseudo_interface(name: &str) -> bool {
    name == "lo0"
        || name == "stf0"
        || name.starts_with("utun")
        || name.starts_with("llw")
        || name.starts_with("awdl")
        || name.starts_with("anpi")
        || name.starts_with("bridge")
        || name.starts_with("gif")
        || name.starts_with("ipsec")
}

pub struct NetSampler {
    nets: Networks,
    last: Option<(u64, u64, Instant)>,  // (rx_total, tx_total, ts)
}

impl Default for NetSampler {
    fn default() -> Self { Self::new() }
}

impl NetSampler {
    pub fn new() -> Self {
        Self {
            nets: Networks::new_with_refreshed_list(),
            last: None,
        }
    }

    /// Returns the current rx/tx rates in bytes per second. The first call
    /// after construction always returns zero (no prior snapshot to delta against).
    pub fn tick(&mut self) -> NetIo {
        self.nets.refresh(true);
        let mut rx_total: u64 = 0;
        let mut tx_total: u64 = 0;
        for (name, data) in self.nets.iter() {
            if is_pseudo_interface(name) { continue; }
            rx_total = rx_total.saturating_add(data.total_received());
            tx_total = tx_total.saturating_add(data.total_transmitted());
        }
        let now = Instant::now();
        let out = if let Some((prx, ptx, pts)) = self.last {
            NetIo {
                rx_bps: rate_bps(prx, rx_total, now.duration_since(pts)),
                tx_bps: rate_bps(ptx, tx_total, now.duration_since(pts)),
            }
        } else {
            NetIo::default()
        };
        self.last = Some((rx_total, tx_total, now));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_basic() {
        let r = rate_bps(0, 1_000_000, Duration::from_secs(1));
        assert_eq!(r, 1_000_000);
    }

    #[test]
    fn rate_handles_zero_dt() {
        assert_eq!(rate_bps(0, 1000, Duration::from_nanos(0)), 0);
    }

    #[test]
    fn rate_clamps_counter_regress() {
        // Counter went backwards (interface reset). Drop the delta.
        assert_eq!(rate_bps(5_000, 100, Duration::from_secs(1)), 0);
    }

    #[test]
    fn rate_subsecond_tick() {
        // 1 MB in 200 ms ⇒ 5 MB/s
        let r = rate_bps(0, 1_000_000, Duration::from_millis(200));
        assert_eq!(r, 5_000_000);
    }

    #[test]
    fn first_tick_returns_zero() {
        let mut s = NetSampler::new();
        let r = s.tick();
        assert_eq!(r.rx_bps, 0);
        assert_eq!(r.tx_bps, 0);
    }
}
