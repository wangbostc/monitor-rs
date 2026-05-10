#[cfg(target_os = "macos")]
#[test]
fn sampler_produces_samples() {
    use monitor_rs::sampler::SamplerHandle;
    use monitor_rs::settings::Settings;
    use std::time::Duration;

    let settings = Settings { sample_rate_hz: 4.0, history_seconds: 10, top_n_procs: 3, ..Settings::default() };
    let handle = SamplerHandle::spawn(settings);
    std::thread::sleep(Duration::from_millis(900)); // ~3-4 ticks at 4 Hz

    {
        let s = handle.store.read();
        assert!(s.len() >= 2, "expected ≥2 samples, got {}", s.len());
        let latest = s.latest().unwrap();
        assert!(latest.cpu_total >= 0.0 && latest.cpu_total <= 100.0);
        assert!(latest.mem.total_bytes > 0);
        assert!(latest.cpu_per_core.len() >= 1);
    }

    handle.stop();
}
