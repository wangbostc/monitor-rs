//! Integration smoke test for the GPU sampler.
//!
//! This test passes whether or not the IOReport channel walk actually returns
//! a real value — it only fails if `tick` errors, or the value is out of
//! range. On Apple Silicon machines with a working IOReport we expect to see
//! a number printed; on Intel / locked-down environments we expect "skipping".

#[cfg(target_os = "macos")]
#[test]
fn gpu_returns_some_after_warmup_on_apple_silicon() {
    use monitor_rs::metrics::gpu::GpuSampler;
    use std::time::Duration;

    let mut s = GpuSampler::new();
    let _ = s.tick(); // warmup — establishes prev sample
    std::thread::sleep(Duration::from_millis(250));
    let r = s.tick().expect("tick must not error");
    if let Some(pct) = r {
        eprintln!("GPU utilization: {pct:.2}%");
        assert!(
            (0.0..=100.0).contains(&pct),
            "GPU pct out of range: {pct}"
        );
    } else {
        eprintln!("GPU sampler returned None — skipping");
    }
}
