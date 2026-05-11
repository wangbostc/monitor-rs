import MonitorRSC
import Foundation

guard let handle = monitor_rs_start() else {
    print("ERROR: monitor_rs_start returned NULL")
    exit(1)
}

print("Sampler started — waiting 1.5s for first samples...")
Thread.sleep(forTimeInterval: 1.5)

var sample = MrsSample()
let ok = monitor_rs_latest(handle, &sample)
if ok == 1 {
    let gpuStr = sample.gpu_present == 1 ? String(format: "%.1f%%", sample.gpu_pct) : "n/a"
    let memPct = Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes) * 100.0
    print(String(format: "CPU: %.1f%%   GPU: %@   MEM: %.1f%% (used %llu / total %llu)",
                 sample.cpu_total_pct, gpuStr, memPct,
                 sample.mem_used_bytes, sample.mem_total_bytes))
    print("Cores: \(sample.core_count), Top processes: \(sample.proc_count)")
} else {
    print("ERROR: monitor_rs_latest returned 0")
}

monitor_rs_stop(handle)
print("Done.")
