//! Probe the GPU sampler. Run with: cargo run --example check_gpu

#[cfg(target_os = "macos")]
fn main() {
    use std::io::Write;
    // Send tracing logs to stderr so we see the warn! from GpuSampler::new.
    tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).init();

    use monitor_rs::metrics::gpu::GpuSampler;
    let mut s = GpuSampler::new();

    for i in 0..5 {
        std::thread::sleep(std::time::Duration::from_millis(300));
        let r = s.tick();
        eprintln!("tick {i}: {r:?}");
        std::io::stderr().flush().ok();
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("not macOS");
}
