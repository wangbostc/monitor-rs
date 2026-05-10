use sysinfo::MINIMUM_CPU_UPDATE_INTERVAL;

fn main() {
    println!("MINIMUM_CPU_UPDATE_INTERVAL: {} ms", MINIMUM_CPU_UPDATE_INTERVAL.as_millis());
    println!("Actual sleep in code: {} ms", MINIMUM_CPU_UPDATE_INTERVAL.as_millis() as u64 + 10);
}
