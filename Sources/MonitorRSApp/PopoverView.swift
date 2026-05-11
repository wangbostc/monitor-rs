import SwiftUI
import MonitorRSC

struct PopoverView: View {
    @Bindable var model: MonitorViewModel
    let onQuit: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HeaderStrip(onQuit: onQuit)

            if let latest = model.latest {
                summaryGrid(latest: latest)
                CoreGrid(perCoreUsage: latest.perCoreUsage)
                Divider()
                Text("TOP PROCESSES")
                    .font(.system(.caption, design: .rounded).weight(.medium))
                    .tracking(0.5)
                    .foregroundStyle(.secondary)
                ProcessList(procs: latest.topProcesses)
                Divider()
                FooterStrip(
                    swapUsedBytes: latest.swap_used_bytes,
                    swapTotalBytes: latest.swap_total_bytes,
                    sampleRateHz: 1.0  // Settings UI is v1.5 scope; reading from the bridge later.
                )
            } else {
                VStack {
                    Spacer()
                    Text("Sampling…")
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, minHeight: 200)
            }
        }
        .padding(14)
        .frame(width: 300)
    }

    @ViewBuilder
    private func summaryGrid(latest: MrsSample) -> some View {
        HStack(alignment: .top, spacing: 12) {
            MetricTile(
                label: "CPU",
                value: "\(Int(latest.cpu_total_pct.rounded()))%",
                color: .green,
                history: normalize(model.cpuHistory)
            )
            MetricTile(
                label: "GPU",
                value: latest.gpuUsage.map { "\(Int($0.rounded()))%" } ?? "n/a",
                color: .blue,
                history: normalize(model.gpuHistory)
            )
            MetricTile(
                label: "MEM",
                value: "\(Int(memPct(latest).rounded()))%",
                color: memColor(latest.mem_pressure),
                history: normalize(model.memHistory)
            )
        }
    }

    private func normalize(_ raw: [Float]) -> [Float] {
        raw.map { max(0, min(1, $0 / 100)) }
    }

    private func memPct(_ s: MrsSample) -> Double {
        guard s.mem_total_bytes > 0 else { return 0 }
        return Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0
    }

    private func memColor(_ pressure: UInt8) -> Color {
        switch pressure {
        case 0: return .orange
        case 1: return Color(red: 0.95, green: 0.55, blue: 0.20)
        default: return .red
        }
    }
}
