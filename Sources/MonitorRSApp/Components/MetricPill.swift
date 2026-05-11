import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// One pill in the non-hero row. Tapping pins the metric as hero.
struct MetricPill: View {
    let kind: MetricKind
    let sample: MrsSample
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 1) {
                Text(kind.displayLabel)
                    .font(.system(size: 9, design: .rounded).weight(.semibold))
                    .tracking(0.5)
                    .foregroundStyle(kind.color)
                Text(displayValue)
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(.white.opacity(0.04))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .strokeBorder(.white.opacity(0.05), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(kind.displayLabel), \(displayValue)")
        .accessibilityHint("Tap to pin as hero")
    }

    private var displayValue: String {
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
            return String(format: "%.1fM", mb)
        case .disk:
            let mb = Double(sample.disk_read_bps + sample.disk_write_bps) / (1024.0 * 1024.0)
            return String(format: "%.1fM", mb)
        }
    }
}
