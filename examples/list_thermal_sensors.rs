//! Run with: cargo run --example list_thermal_sensors
//! Prints every thermal sensor service IOHIDEventSystemClient enumerates,
//! with its current reading. Use this to validate or extend the sensor
//! name table in src/metrics/thermal.rs.

#[cfg(target_os = "macos")]
fn main() {
    monitor_rs::__example_dump_thermal_sensors();
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("not macOS");
}
