import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var menuBarController: MenuBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        menuBarController = MenuBarController()
        LoginItem.ensureRegistered()
    }

    func applicationWillTerminate(_ notification: Notification) {
        menuBarController = nil  // will eventually release the RustBridge → calls monitor_rs_stop
    }

    // LSUIElement app: don't quit when the popover closes; only quit on explicit termination.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false
    }
}
