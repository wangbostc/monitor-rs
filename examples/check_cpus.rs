use sysinfo::{CpuRefreshKind, RefreshKind, System};

fn main() {
    let sys = System::new_with_specifics(
        RefreshKind::nothing().with_cpu(CpuRefreshKind::everything()),
    );
    println!("CPU count: {}", sys.cpus().len());
}
