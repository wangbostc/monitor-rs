#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::metrics::cpu::CpuSampler;
use crate::metrics::gpu::GpuSampler;
use crate::metrics::mem::MemSampler;
use crate::metrics::procs::ProcSampler;
use crate::sample::Sample;
use crate::settings::Settings;
use crate::store::SampleStore;

pub struct SamplerHandle {
    pub store: Arc<RwLock<SampleStore>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl SamplerHandle {
    pub fn spawn(settings: Settings) -> Self {
        let cap = settings.history_capacity();
        let store = Arc::new(RwLock::new(SampleStore::new(cap)));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let store_w = store.clone();
        let stop_w = stop.clone();
        let join = thread::Builder::new()
            .name("monitor-rs-sampler".into())
            .spawn(move || run_loop(settings, store_w, stop_w))
            .expect("spawn sampler thread");

        Self { store, stop, join: Some(join) }
    }

    pub fn stop(mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run_loop(
    settings: Settings,
    store: Arc<RwLock<SampleStore>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut cpu = CpuSampler::new();
    let mut mem = MemSampler::new();
    let mut procs = ProcSampler::new(settings.top_n_procs);
    let mut gpu = GpuSampler::new();

    let interval = Duration::from_secs_f32(1.0 / settings.sample_rate_hz.max(0.1));
    let mut next = Instant::now();

    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        next += interval;
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        } else if now > next + Duration::from_secs(2) {
            // Process slept; resync.
            next = Instant::now();
        }

        let cpu_r = match cpu.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("cpu tick: {e}"); continue; }
        };
        let mem_r = match mem.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("mem tick: {e}"); continue; }
        };
        let top = procs.tick().unwrap_or_default();
        let gpu_pct = gpu.tick().ok().flatten();

        let s = Sample {
            ts: Instant::now(),
            cpu_total: cpu_r.total_pct,
            cpu_per_core: cpu_r.per_core_pct,
            gpu_pct,
            mem: mem_r.mem,
            swap: mem_r.swap,
            top_procs: top,
        };
        store.write().push(s);
    }
}
