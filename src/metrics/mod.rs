use thiserror::Error;

pub mod cpu;
#[cfg(target_os = "macos")]
pub mod gpu;
pub mod mem;
#[cfg(target_os = "macos")]
pub mod disk;
#[cfg(target_os = "macos")]
pub mod net;
pub mod procs;

#[derive(Debug, Error)]
pub enum MetricError {
    #[error("metric unavailable: {0}")]
    Unavailable(String),
    #[error("FFI error: {0}")]
    Ffi(String),
}
