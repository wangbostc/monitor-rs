import SwiftUI
import Charts

/// Area + line chart used inside `HeroCard`. Tinted via `color`.
///
/// `values` are the recent samples in display units (any scale — the chart
/// auto-scales). They are rendered oldest-on-the-left.
struct HeroChart: View {
    let values: [Float]
    let color: Color

    var body: some View {
        Chart {
            ForEach(Array(values.enumerated()), id: \.offset) { idx, value in
                AreaMark(
                    x: .value("t", idx),
                    y: .value("v", value)
                )
                .foregroundStyle(
                    LinearGradient(
                        colors: [color.opacity(0.55), color.opacity(0)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                .interpolationMethod(.monotone)

                LineMark(
                    x: .value("t", idx),
                    y: .value("v", value)
                )
                .foregroundStyle(color)
                .interpolationMethod(.monotone)
                .lineStyle(StrokeStyle(lineWidth: 1.4))
            }
        }
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartLegend(.hidden)
        .chartPlotStyle { plot in plot.background(Color.clear) }
        .accessibilityHidden(true)
    }
}
