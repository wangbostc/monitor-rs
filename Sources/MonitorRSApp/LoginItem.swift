import ServiceManagement
import os

enum LoginItem {
    private static let log = Logger(subsystem: "dev.monitor-rs", category: "login-item")

    static var isRegistered: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// Register the app to auto-launch at login. Safe to call repeatedly.
    static func ensureRegistered() {
        let svc = SMAppService.mainApp
        guard svc.status != .enabled else { return }
        do {
            try svc.register()
            log.info("registered for launch-at-login")
        } catch {
            log.error("register failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    static func unregister() {
        try? SMAppService.mainApp.unregister()
    }
}
