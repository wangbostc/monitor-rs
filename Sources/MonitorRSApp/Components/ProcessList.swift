import SwiftUI
import MonitorRSC

struct ProcessList: View {
    let procs: [MrsProcInfo]

    var body: some View {
        if procs.isEmpty {
            Text("No process data")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                ForEach(Array(procs.enumerated()), id: \.offset) { _, p in
                    GridRow {
                        Text(truncate(p.nameString, max: 22))
                            .font(.system(.caption, design: .default))
                            .lineLimit(1)
                        Text("\(Int(p.cpu_pct.rounded()))%")
                            .font(.system(.caption, design: .default).monospacedDigit())
                            .frame(minWidth: 36, alignment: .trailing)
                            .foregroundStyle(.secondary)
                        Text(formatBytes(p.rss_bytes))
                            .font(.system(.caption, design: .default).monospacedDigit())
                            .frame(minWidth: 52, alignment: .trailing)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func truncate(_ s: String, max: Int) -> String {
        s.count <= max ? s : String(s.prefix(max - 1)) + "…"
    }

    private func formatBytes(_ b: UInt64) -> String {
        let b = Double(b)
        let GB = 1024.0 * 1024.0 * 1024.0
        let MB = 1024.0 * 1024.0
        let KB = 1024.0
        if b >= GB { return String(format: "%.1fG", b / GB) }
        if b >= MB { return String(format: "%.0fM", b / MB) }
        return String(format: "%.0fK", max(b / KB, 0))
    }
}
