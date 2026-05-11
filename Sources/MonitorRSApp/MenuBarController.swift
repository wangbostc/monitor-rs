import AppKit
import SwiftUI
import MonitorRSC

@MainActor
final class MenuBarController {
    private let statusItem: NSStatusItem
    private let popover: NSPopover
    private let bridge: RustBridge?
    private let viewModel = MonitorViewModel()
    private var refreshTimer: Timer?

    init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        bridge = RustBridge()

        popover = NSPopover()
        popover.contentSize = NSSize(width: 300, height: 360)
        popover.behavior = .transient
        popover.animates = true
        popover.contentViewController = NSHostingController(
            rootView: PopoverView(model: viewModel, onQuit: {
                NSApp.terminate(nil)
            })
        )

        if let button = statusItem.button {
            button.title = "monitor-rs"
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        startRefreshLoop()
    }

    deinit {
        refreshTimer?.invalidate()
    }

    @objc private func togglePopover(_ sender: NSStatusBarButton) {
        if popover.isShown {
            popover.performClose(sender)
        } else {
            popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }

    private func startRefreshLoop() {
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshTick() }
        }
    }

    private func refreshTick() {
        guard let bridge = bridge else { return }
        let latest = bridge.latest()
        let recent = bridge.recent(120)
        viewModel.latest = latest
        viewModel.recent = recent

        if let s = latest {
            statusItem.button?.title = MenuBarController.formatStatus(s)
        }
    }

    /// Equivalent to the old Rust render_menu_bar with the default template
    /// "C {cpu} G {gpu} M {mem}". Em-dash for GPU None.
    static func formatStatus(_ s: MrsSample) -> String {
        let cpu = Int(s.cpu_total_pct.rounded())
        let gpu: String = s.gpu_present == 1 ? "\(Int(s.gpu_pct.rounded()))" : "—"
        let memPct: Int = {
            guard s.mem_total_bytes > 0 else { return 0 }
            return Int((Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0).rounded())
        }()
        return "C \(cpu) G \(gpu) M \(memPct)"
    }
}
