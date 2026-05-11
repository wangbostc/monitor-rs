import SwiftUI

struct HeaderStrip: View {
    /// Called when the user clicks the power icon — should quit the app.
    let onQuit: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Text("MONITOR-RS")
                .font(.system(.caption, design: .rounded).weight(.medium))
                .tracking(1.2)
                .foregroundStyle(.secondary)

            Spacer()

            Button(action: {}) {
                Image(systemName: "gearshape")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Settings (coming soon)")
            .disabled(true)

            Button(action: onQuit) {
                Image(systemName: "power")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Quit monitor-rs")
        }
    }
}
