import SwiftUI
import MonitorRSC

struct PopoverView: View {
    @Bindable var model: MonitorViewModel

    var body: some View {
        VStack(spacing: 8) {
            if let latest = model.latest {
                Text("CPU \(Int(latest.cpu_total_pct))%   GPU \(latest.gpu_present == 1 ? "\(Int(latest.gpu_pct))%" : "n/a")   MEM \(memPercent(latest))%")
                    .font(.system(.body, design: .monospaced))
                Text("Cores: \(latest.core_count) · Procs: \(latest.proc_count) · History: \(model.recent.count) samples")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text("Sampling…")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(20)
        .frame(width: 300, height: 120)
    }

    private func memPercent(_ s: MrsSample) -> Int {
        guard s.mem_total_bytes > 0 else { return 0 }
        return Int(Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0)
    }
}
