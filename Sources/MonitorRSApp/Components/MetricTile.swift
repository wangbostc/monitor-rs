import SwiftUI

/// One of the three top-row tiles (CPU / GPU / MEM).
/// `value` is the current display (e.g. "9%" or "n/a").
/// `history` are recent samples normalized to 0…1 for the sparkline.
struct MetricTile: View {
    let label: String
    let value: String
    let color: Color
    let history: [Float]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(.caption, design: .rounded).weight(.medium))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.5)

            Text(value)
                .font(.system(.title2, design: .rounded).weight(.semibold))
                .monospacedDigit()

            Sparkline(values: history, color: color)
                .frame(height: 28)
        }
    }
}
