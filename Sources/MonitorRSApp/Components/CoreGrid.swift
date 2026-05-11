import SwiftUI

/// A horizontal row of small colored blocks, one per CPU core, colored by usage.
struct CoreGrid: View {
    let perCoreUsage: [Float]  // 0–100 per core

    var body: some View {
        GeometryReader { geo in
            let n = max(perCoreUsage.count, 1)
            let gap: CGFloat = 2
            let blockW = max(4, (geo.size.width - gap * CGFloat(n - 1)) / CGFloat(n))
            HStack(spacing: gap) {
                ForEach(Array(perCoreUsage.enumerated()), id: \.offset) { _, usage in
                    RoundedRectangle(cornerRadius: 2)
                        .fill(color(for: usage))
                        .frame(width: blockW)
                }
            }
        }
        .frame(height: 8)
    }

    private func color(for pct: Float) -> Color {
        // Green at 0% → red at 100% via HSV hue interpolation.
        let p = Double(min(max(pct, 0), 100) / 100.0)
        let hue = 0.33 * (1.0 - p)  // 0.33 (green) at 0%, 0 (red) at 100%
        return Color(hue: hue, saturation: 0.7, brightness: 0.85)
    }
}
