import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// Horizontal row of pills for the four non-hero metrics. The order is
/// fixed (CPU, GPU, MEM, NET, DSK with the current hero filtered out)
/// so the layout doesn't churn as the hero swaps.
struct PillsRow: View {
    let hero: MetricKind
    let sample: MrsSample
    let onPin: (MetricKind) -> Void

    var body: some View {
        HStack(spacing: 6) {
            ForEach(MetricKind.allCases.filter { $0 != hero }, id: \.self) { kind in
                MetricPill(kind: kind, sample: sample, onTap: { onPin(kind) })
            }
        }
    }
}
