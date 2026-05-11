import SwiftUI

struct FooterStrip: View {
    let swapUsedBytes: UInt64
    let swapTotalBytes: UInt64

    // Optional inputs — pass nil/false to hide.
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
}
