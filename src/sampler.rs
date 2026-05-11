#![cfg(target_os = "macos")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    /// When set to `true` the sampler refreshes the (expensive) process
    /// table each tick. When `false` it reuses the last result, since the
    /// popover (the only consumer of that data) is not visible. Toggled
    /// from the Swift side via `monitor_rs_set_active`.
    pub procs_active: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl SamplerHandle {
    pub fn spawn(settings: Settings) -> Self {
        let cap = settings.history_capacity();
        let store = Arc::new(RwLock::new(SampleStore::new(cap)));
        let stop = Arc::new(AtomicBool::new(false));
        let procs_active = Arc::new(AtomicBool::new(false));

        let store_w = store.clone();
        let stop_w = stop.clone();
        let procs_active_w = procs_active.clone();
        let join = thread::Builder::new()
            .name("monitor-rs-sampler".into())
            .spawn(move || run_loop(settings, store_w, stop_w, procs_active_w))
            .expect("spawn sampler thread");

        Self { store, procs_active, stop, join: Some(join) }
    }

    pub fn stop(self) {
        drop(self);
    }
}

impl Drop for SamplerHandle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run_loop(
    settings: Settings,
    store: Arc<RwLock<SampleStore>>,
    stop: Arc<AtomicBool>,
    procs_active: Arc<AtomicBool>,
) {
    let mut cpu = CpuSampler::new();
    let mut mem = MemSampler::new();
    let mut procs = ProcSampler::new(settings.top_n_procs);
    let mut gpu = GpuSampler::new();
    let mut net = crate::metrics::net::NetSampler::new();
    let mut disk = crate::metrics::disk::DiskSampler::new();
    let battery = crate::metrics::battery::BatterySampler::new();
    let mut thermal = crate::metrics::thermal::ThermalSampler::new();

    let interval = Duration::from_secs_f32(1.0 / settings.sample_rate_hz.max(0.1));
    let mut next = Instant::now();

    // Latest computed top-process lists. When `procs_active` is false we
    // reuse these instead of paying for a full sysinfo refresh; the
    // popover (the only consumer) isn't visible, so the freshness doesn't
    // matter. When the popover next opens, Swift flips the flag and the
    // following tick recomputes.
    let mut last_top = crate::metrics::procs::TopProcs {
        by_cpu: Vec::new(),
        by_mem: Vec::new(),
    };

    while !stop.load(Ordering::Relaxed) {
        next += interval;
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        } else if now > next + Duration::from_secs(2) {
            // Process slept; resync.
            next = Instant::now();
        }

        // CPU and memory are required: if they fail, skip this tick entirely.
        // Processes and GPU are best-effort: partial samples (empty proc list,
        // gpu_pct=None) are still useful, so we don't abort the tick on their failure.
        let cpu_r = match cpu.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("cpu tick: {e}"); continue; }
        };
        let mem_r = match mem.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("mem tick: {e}"); continue; }
        };
        if procs_active.load(Ordering::Relaxed) {
            if let Ok(top) = procs.tick() {
                last_top = top;
            }
        }
        let gpu_pct = gpu.tick().ok().flatten();
        let net_io = net.tick();
        let disk_io = disk.tick();
        let battery_info = battery.tick();
        let thermal_info = thermal.tick();

        let s = Sample {
            ts: Instant::now(),
            cpu_total: cpu_r.total_pct,
            cpu_per_core: cpu_r.per_core_pct,
            gpu_pct,
            mem: mem_r.mem,
            swap: mem_r.swap,
            top_procs: last_top.by_cpu.clone(),
            top_procs_by_mem: last_top.by_mem.clone(),
            net: net_io,
            disk: disk_io,
            battery: battery_info,
            thermal: thermal_info,
        };
        store.write().push(s);
    }
}
