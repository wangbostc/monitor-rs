#[cfg(target_os = "macos")]
pub mod ffi;
pub mod logging;
pub mod metrics;
pub mod sample;
#[cfg(target_os = "macos")]
pub mod sampler;
pub mod settings;
pub mod store;

#[cfg(target_os = "macos")]
#[doc(hidden)]
pub fn __example_dump_thermal_sensors() {
    crate::metrics::thermal::dump_all();
}
