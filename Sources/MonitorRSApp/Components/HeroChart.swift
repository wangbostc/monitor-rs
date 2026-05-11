import SwiftUI

/// Area + line chart used inside `HeroCard`. Tinted via `color`.
///
/// `values` are the recent samples in display units (any scale — auto-scales
/// against the max in the window with a small floor so an idle window
/// doesn't render as a single flat line at full-scale). Oldest on the left.
///
/// Pure-Canvas implementation — deliberately doesn't import Swift Charts
/// to keep our framework load light. Sparkline.swift uses the same
/// pattern at a smaller size.
struct HeroChart: View {
    let values: [Float]
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard values.count >= 2 else { return }

            let n = values.count
            let dx = size.width / CGFloat(n - 1)
            let peak = max(0.001, CGFloat(values.max() ?? 0))

            func point(_ i: Int) -> CGPoint {
                let x = CGFloat(i) * dx
                let v = max(0, min(1, CGFloat(values[i]) / peak))
                let y = size.height * (1 - v)
                return CGPoint(x: x, y: y)
            }

            // Filled area under the line, with a vertical gradient.
            var fillPath = Path()
            fillPath.move(to: CGPoint(x: 0, y: size.height))
            for i in 0..<n { fillPath.addLine(to: point(i)) }
            fillPath.addLine(to: CGPoint(x: size.width, y: size.height))
            fillPath.closeSubpath()
            context.fill(
                fillPath,
                with: .linearGradient(
                    Gradient(colors: [color.opacity(0.55), color.opacity(0)]),
                    startPoint: CGPoint(x: 0, y: 0),
                    endPoint: CGPoint(x: 0, y: size.height)
                )
            )

            // Line on top.
            var linePath = Path()
            linePath.move(to: point(0))
            for i in 1..<n { linePath.addLine(to: point(i)) }
            context.stroke(linePath, with: .color(color), lineWidth: 1.4)
        }
        .accessibilityHidden(true)
    }
}
