import SwiftUI

struct PopoverView: View {
    var body: some View {
        VStack {
            Text("monitor-rs")
                .font(.headline)
            Text("Popover scaffolding — components in later tasks.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(20)
        .frame(width: 300, height: 200)
    }
}
