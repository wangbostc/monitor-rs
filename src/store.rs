use std::collections::VecDeque;

use crate::sample::Sample;

pub struct SampleStore {
    buf: VecDeque<Sample>,
    capacity: usize,
}

impl SampleStore {
    pub fn new(capacity: usize) -> Self {
        Self { buf: VecDeque::with_capacity(capacity.max(1)), capacity: capacity.max(1) }
    }

    pub fn push(&mut self, s: Sample) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(s);
    }

    pub fn latest(&self) -> Option<&Sample> {
        self.buf.back()
    }

    pub fn recent(&self, n: usize) -> impl Iterator<Item = &Sample> + '_ {
        let take = n.min(self.buf.len());
        self.buf.iter().skip(self.buf.len() - take)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{MemInfo, MemPressure, SwapInfo, NetIo, DiskIo, BatteryInfo, ThermalInfo};
    use std::time::Instant;

    fn dummy_sample(cpu: f32) -> Sample {
        Sample {
            ts: Instant::now(),
            cpu_total: cpu,
            cpu_per_core: vec![cpu],
            gpu_pct: None,
            mem: MemInfo { used_bytes: 0, total_bytes: 1, pressure: MemPressure::Normal },
            swap: SwapInfo { used_bytes: 0, total_bytes: 0 },
            top_procs: vec![],
            net: NetIo::default(),
            disk: DiskIo::default(),
            battery: BatteryInfo::default(),
            thermal: ThermalInfo::default(),
        }
    }

    #[test]
    fn pushes_and_evicts_at_capacity() {
        let mut s = SampleStore::new(3);
        s.push(dummy_sample(1.0));
        s.push(dummy_sample(2.0));
        s.push(dummy_sample(3.0));
        s.push(dummy_sample(4.0));
        assert_eq!(s.len(), 3);
        assert_eq!(s.latest().unwrap().cpu_total, 4.0);
        let recents: Vec<f32> = s.recent(10).map(|x| x.cpu_total).collect();
        assert_eq!(recents, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn recent_clamped_to_len() {
        let mut s = SampleStore::new(10);
        s.push(dummy_sample(1.0));
        s.push(dummy_sample(2.0));
        let r: Vec<f32> = s.recent(5).map(|x| x.cpu_total).collect();
        assert_eq!(r, vec![1.0, 2.0]);
    }

    #[test]
    fn capacity_zero_clamped_to_one() {
        let s = SampleStore::new(0);
        assert_eq!(s.capacity(), 1);
    }
}
