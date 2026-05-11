import SwiftUI

/// A small filled-area sparkline. `values` should be 0…1 normalized.
struct Sparkline: View {
    let values: [Float]
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard values.count >= 2 else { return }

            let n = values.count
            let dx = size.width / CGFloat(n - 1)

            func point(_ i: Int) -> CGPoint {
                let x = CGFloat(i) * dx
                let v = max(0, min(1, CGFloat(values[i])))
                let y = size.height * (1 - v)
                return CGPoint(x: x, y: y)
            }

            // Filled area under the line.
            var fillPath = Path()
            fillPath.move(to: CGPoint(x: 0, y: size.height))
            for i in 0..<n { fillPath.addLine(to: point(i)) }
            fillPath.addLine(to: CGPoint(x: size.width, y: size.height))
            fillPath.closeSubpath()
            context.fill(fillPath, with: .color(color.opacity(0.20)))

            // Line on top.
            var linePath = Path()
            linePath.move(to: point(0))
            for i in 1..<n { linePath.addLine(to: point(i)) }
            context.stroke(linePath, with: .color(color), lineWidth: 1.5)
        }
    }
}
