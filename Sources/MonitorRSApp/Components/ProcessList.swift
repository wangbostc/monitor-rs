import SwiftUI
import MonitorRSC

struct ProcessList: View {
    let procs: [MrsProcInfo]

    /// Fixed pixel widths for the numeric columns. Choosing them once here
    /// (rather than relying on `Grid`'s content-driven sizing) keeps the
    /// CPU% / RSS columns at the same X position across re-sorts — the name
    /// column absorbs whatever's left.
    private static let cpuColumnWidth: CGFloat = 36
    private static let rssColumnWidth: CGFloat = 52
    private static let columnSpacing: CGFloat = 12

    var body: some View {
        if procs.isEmpty {
            Text("No process data")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            VStack(alignment: .leading, spacing: 4) {
                ForEach(Array(procs.enumerated()), id: \.offset) { _, p in
                    HStack(spacing: Self.columnSpacing) {
                        Text(truncate(p.nameString, max: 22))
                            .font(.caption)
                            .lineLimit(1)
                            .truncationMode(.tail)
                            .frame(maxWidth: .infinity, alignment: .leading)

                        Text("\(Int(p.cpu_pct.rounded()))%")
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .frame(width: Self.cpuColumnWidth, alignment: .trailing)

                        Text(formatBytes(p.rss_bytes))
                            .font(.caption.monospacedDigit())
                            .foregroundStyle(.secondary)
                            .frame(width: Self.rssColumnWidth, alignment: .trailing)
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
