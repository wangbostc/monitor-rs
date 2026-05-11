import SwiftUI

@main
struct MonitorRSApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        // No main window — the app lives in the menu bar.
        // `Settings` is a no-op placeholder scene.
        Settings { EmptyView() }
    }
}
