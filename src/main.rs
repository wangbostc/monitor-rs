fn main() {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");
}
