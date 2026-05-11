import SwiftUI

struct FooterStrip: View {
    let swapUsedBytes: UInt64
    let swapTotalBytes: UInt64
    let sampleRateHz: Double

    // New optional inputs — pass nil/false to hide.
    let batteryPresent: Bool
    let batteryPct: Float
    let batteryCharging: Bool
    let cpuTempC: Float?
    let gpuTempC: Float?

    var body: some View {
        HStack(spacing: 8) {
            Text("swap \(swapText)")
                .font(.caption)
                .foregroundStyle(.secondary)

            if batteryPresent {
                Text("·")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Text(batteryText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            if cpuTempC != nil || gpuTempC != nil {
                Text("·")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                Text(tempText)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

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

    private var batteryText: String {
        let pct = Int(batteryPct.rounded())
        return batteryCharging ? "🔋 \(pct)% ⚡" : "🔋 \(pct)%"
    }

    private var tempText: String {
        var parts: [String] = []
        if let c = cpuTempC { parts.append("CPU \(Int(c.rounded()))°") }
        if let g = gpuTempC { parts.append("GPU \(Int(g.rounded()))°") }
        return "🌡 " + parts.joined(separator: " ")
    }

    private var rateText: String {
        let rounded = (sampleRateHz * 10).rounded() / 10
        if rounded == 1.0 { return "1 Hz" }
        return String(format: "%.1f Hz", rounded)
    }
}
