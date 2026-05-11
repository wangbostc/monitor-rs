import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// The large tinted card showing the currently-promoted metric.
struct HeroCard: View {
    let kind: MetricKind
    let sample: MrsSample
    let history: [Float]
    let isPinned: Bool
    let onTap: () -> Void

    var body: some View {
        let tint = effectiveTint
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(kind.displayLabel)
                        .font(.system(.caption, design: .rounded).weight(.medium))
                        .tracking(0.5)
                        .foregroundStyle(tint)
                    if isPinned {
                        Circle()
                            .fill(tint)
                            .frame(width: 4, height: 4)
                            .accessibilityHidden(true)
                    }
                }
                Text(bigValue)
                    .font(.system(size: 32, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
                Text(metaLine)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            HeroChart(values: history, color: tint)
                .frame(width: 110, height: 60)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(
            LinearGradient(
                colors: [tint.opacity(0.18), tint.opacity(0.05)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ),
            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(tint.opacity(0.16), lineWidth: 1)
        )
        .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onTapGesture(perform: onTap)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityDescription)
        .accessibilityHint(isPinned ? "Activate to unpin" : "Currently auto-selected")
        .accessibilityAddTraits(.isButton)
    }

    /// Memory-pressure-aware tint for MEM; otherwise the metric's own color.
    private var effectiveTint: Color {
        guard kind == .mem else { return kind.color }
        switch sample.mem_pressure {
        case 0:  return .orange
        case 1:  return Color(red: 0.95, green: 0.55, blue: 0.20)
        default: return .red
        }
    }

    private var bigValue: String {
        switch kind {
        case .cpu: return "\(Int(sample.cpu_total_pct.rounded()))%"
        case .gpu:
            return sample.gpu_present == 1
                ? "\(Int(sample.gpu_pct.rounded()))%"
                : "n/a"
        case .mem:
            let pct = sample.mem_total_bytes > 0
                ? Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes) * 100.0
                : 0
            return "\(Int(pct.rounded()))%"
        case .net:
            let mb = Double(sample.net_rx_bps + sample.net_tx_bps) / (1024.0 * 1024.0)
            return String(format: "%.1f MB/s", mb)
        case .disk:
            let mb = Double(sample.disk_read_bps + sample.disk_write_bps) / (1024.0 * 1024.0)
            return String(format: "%.1f MB/s", mb)
        }
    }

    private var metaLine: String {
        switch kind {
        case .cpu:
            let cores = sample.perCoreUsage
            let hottest = Int((cores.max() ?? 0).rounded())
            return "\(cores.count)-core · hot core \(hottest)%"
        case .gpu:
            return sample.gpu_present == 1 ? "Metal active" : "Metal idle"
        case .mem:
            let usedGB = Double(sample.mem_used_bytes) / (1024.0 * 1024.0 * 1024.0)
            let totalGB = Double(sample.mem_total_bytes) / (1024.0 * 1024.0 * 1024.0)
            let pressure: String
            switch sample.mem_pressure {
            case 0:  pressure = "normal"
            case 1:  pressure = "warn"
            default: pressure = "crit"
            }
            return String(format: "%.1f / %.1f GB · pressure %@", usedGB, totalGB, pressure)
        case .net:
            let rx = Double(sample.net_rx_bps) / (1024.0 * 1024.0)
            let tx = Double(sample.net_tx_bps) / (1024.0 * 1024.0)
            let peak = max(1.0, history.map { Double($0) }.max() ?? 0)
            return String(format: "↓ %.1f ↑ %.1f · peak %.1f", rx, tx, peak)
        case .disk:
            let rd = Double(sample.disk_read_bps) / (1024.0 * 1024.0)
            let wr = Double(sample.disk_write_bps) / (1024.0 * 1024.0)
            let peak = max(1.0, history.map { Double($0) }.max() ?? 0)
            return String(format: "↓ %.1f ↑ %.1f · peak %.1f", rd, wr, peak)
        }
    }

    private var accessibilityDescription: String {
        "\(kind.displayLabel), \(bigValue). \(metaLine)."
    }
}
