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
        // Promote RSS to phys_footprint for the rows we'll actually show.
        // Cheap (one syscall per row, <= top_n rows) and gives a number
        // that matches Activity Monitor's Memory column.
        for p in &mut by_cpu {
            if let Some(fp) = phys_footprint_bytes(p.pid) {
                p.rss_bytes = fp;
            }
        }

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
        for p in &mut by_mem {
            if let Some(fp) = phys_footprint_bytes(p.pid) {
                p.rss_bytes = fp;
            }
        }
        // After the upgrade the values may have shuffled; re-sort the
        // shown rows so the column is monotonic.
        by_mem.sort_by(|a, b| b.rss_bytes.cmp(&a.rss_bytes));

        Ok(TopProcs { by_cpu, by_mem })
    }
}

// ----- Phys-footprint lookup via proc_pid_rusage -----
//
// `proc_pid_taskinfo` (what sysinfo uses) gives RSS — pages physically
// mapped to the process, including the read-only text pages of
// SwiftUI / AppKit / Foundation / etc. that every GUI process shares.
// That makes our own row in "Top Processes" look ~3× larger than what
// Activity Monitor shows for the same PID.
//
// `proc_pid_rusage` with `RUSAGE_INFO_V4` exposes `ri_phys_footprint`,
// Apple's own per-process memory metric (anonymous + compressed +
// wired-private, excluding shared library text). It's exactly what AM
// displays in its Memory column. The call requires same euid as the
// target process; for processes we can't read we fall back to RSS.

// `RUSAGE_INFO_V3` (= 3) is the lowest flavor that exposes
// `ri_phys_footprint`. We use V3 specifically because:
//   * V3's `struct rusage_info_v3` is 232 bytes (stable since 10.10).
//   * V4 and later add tail fields whose exact layout has changed
//     across macOS versions; mis-sizing the buffer corrupts the stack.
// `ri_phys_footprint` is at byte offset 72 in V0/V1/V2/V3/V4, so V3 is
// enough and gives us a struct size that's been stable for a decade.
const RUSAGE_INFO_V3: libc::c_int = 3;
const RUSAGE_INFO_V3_SIZE_U64: usize = 29;  // 232 bytes
const RI_PHYS_FOOTPRINT_INDEX: usize = 9;   // bytes 72..80 (after uuid + 7 u64s)

unsafe extern "C" {
    fn proc_pid_rusage(
        pid: libc::pid_t,
        flavor: libc::c_int,
        buffer: *mut libc::c_void,
    ) -> libc::c_int;
}

/// Returns `ri_phys_footprint` for the given PID if the caller has
/// permission (same euid as the target). `None` for processes we can't
/// read (kernel_task, WindowServer, anything running as another user).
fn phys_footprint_bytes(pid: u32) -> Option<u64> {
    // Buffer is a u64 array so accessing `ri_phys_footprint` is a clean
    // indexed read. Sized exactly to V3's struct (232 bytes), matching
    // the flavor we pass.
    let mut buf: [u64; RUSAGE_INFO_V3_SIZE_U64] = [0; RUSAGE_INFO_V3_SIZE_U64];
    // SAFETY: pid is by value, buffer is exclusively borrowed and exactly
    // the size that RUSAGE_INFO_V3 requires the kernel to fill.
    let rc = unsafe {
        proc_pid_rusage(
            pid as libc::pid_t,
            RUSAGE_INFO_V3,
            buf.as_mut_ptr() as *mut libc::c_void,
        )
    };
    if rc == 0 {
        Some(buf[RI_PHYS_FOOTPRINT_INDEX])
    } else {
        None
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

    #[test]
    fn phys_footprint_of_self_is_nonzero() {
        let pid = std::process::id();
        let fp = phys_footprint_bytes(pid).expect("can read own phys_footprint");
        assert!(fp > 0, "expected nonzero phys_footprint, got {fp}");
    }
}
