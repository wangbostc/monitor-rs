import SwiftUI

struct FooterStrip: View {
    let swapUsedBytes: UInt64
    let swapTotalBytes: UInt64
    let sampleRateHz: Double

    var body: some View {
        HStack {
            Text("swap \(swapText)")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Text("· \(rateText) ·")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private var swapText: String {
        if swapTotalBytes == 0 { return "off" }
        let g = Double(swapUsedBytes) / (1024.0 * 1024.0 * 1024.0)
        return String(format: "%.2f GB", g)
    }

    private var rateText: String {
        let rounded = (sampleRateHz * 10).rounded() / 10
        if rounded == 1.0 { return "1 Hz" }
        return String(format: "%.1f Hz", rounded)
    }
}
