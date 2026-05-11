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
                ioGrid(latest: latest)
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
                    sampleRateHz: 1.0,
                    batteryPresent: latest.battery_present == 1,
                    batteryPct: latest.battery_pct,
                    batteryCharging: latest.battery_charging == 1,
                    cpuTempC: latest.cpu_temp_present == 1 ? latest.cpu_temp_c : nil,
                    gpuTempC: latest.gpu_temp_present == 1 ? latest.gpu_temp_c : nil
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

    @ViewBuilder
    private func ioGrid(latest: MrsSample) -> some View {
        HStack(alignment: .top, spacing: 12) {
            MetricTile(
                label: "NET",
                value: formatIO(rx: latest.net_rx_bps, tx: latest.net_tx_bps),
                color: .teal,
                history: normalizeIO(model.netHistory)
            )
            MetricTile(
                label: "DSK",
                value: formatIO(rx: latest.disk_read_bps, tx: latest.disk_write_bps),
                color: .purple,
                history: normalizeIO(model.diskHistory)
            )
        }
    }

    private func formatIO(rx: UInt64, tx: UInt64) -> String {
        let rxMB = Double(rx) / (1024.0 * 1024.0)
        let txMB = Double(tx) / (1024.0 * 1024.0)
        return String(format: "↓%.1f ↑%.1f", rxMB, txMB)
    }

    /// Auto-scale IO history into 0…1 against the max in the window
    /// (with a floor of 1 MB/s so an idle window doesn't render full-scale noise).
    private func normalizeIO(_ raw: [Float]) -> [Float] {
        let peak = max(1.0, raw.max() ?? 0.0)
        return raw.map { max(0, min(1, $0 / peak)) }
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
