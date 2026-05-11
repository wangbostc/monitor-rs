use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize logging.
///
/// Returns a `WorkerGuard` the caller must keep alive for the duration of
/// the program; dropping it flushes the file appender.
pub fn init() -> WorkerGuard {
    let log_dir = log_dir();
    if let Some(dir) = &log_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    let file_appender = match log_dir {
        Some(dir) => tracing_appender::rolling::daily(dir, "monitor-rs.log"),
        None => tracing_appender::rolling::never(std::env::temp_dir(), "monitor-rs.log"),
    };
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,monitor_rs=debug"));

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_writer).with_ansi(false));

    #[cfg(debug_assertions)]
    let registry = registry.with(fmt::layer().with_writer(std::io::stderr));

    registry.init();
    guard
}

fn log_dir() -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    Some(home.join("Library/Logs/monitor-rs"))
}
