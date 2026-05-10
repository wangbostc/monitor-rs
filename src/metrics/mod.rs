use thiserror::Error;

pub mod cpu;
pub mod mem;

#[derive(Debug, Error)]
pub enum MetricError {
    #[error("metric unavailable: {0}")]
    Unavailable(String),
    #[error("FFI error: {0}")]
    Ffi(String),
}
