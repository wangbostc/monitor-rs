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
            // SF Symbol icon stays visible even when menu bar is crowded
            // (notched Mac with many items). Title text shows live numbers
            // alongside it once samples arrive.
            let icon = NSImage(systemSymbolName: "gauge.with.dots.needle.50percent",
                               accessibilityDescription: "monitor-rs")
            icon?.isTemplate = true  // tints with menu bar foreground (Light/Dark aware)
            button.image = icon
            button.imagePosition = .imageLeft
            button.title = "—"  // narrow placeholder until first sample arrives
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        tracing_log_startup()
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

    /// Compact status text shown alongside the gauge icon.
    /// Format: "CPU% GPU% MEM%" — em-dash for GPU None.
    static func formatStatus(_ s: MrsSample) -> String {
        let cpu = Int(s.cpu_total_pct.rounded())
        let gpu: String = s.gpu_present == 1 ? "\(Int(s.gpu_pct.rounded()))" : "—"
        let memPct: Int = {
            guard s.mem_total_bytes > 0 else { return 0 }
            return Int((Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0).rounded())
        }()
        return "\(cpu) \(gpu) \(memPct)"
    }

    /// Write a startup line to the rolling log so we can confirm the menu-bar
    /// app actually launched (and from which bundle path).
    private func tracing_log_startup() {
        let logPath = ("~/Library/Logs/monitor-rs/" as NSString).expandingTildeInPath
        let line = "\(Date()) menu bar app launched (status item created)\n"
        let fm = FileManager.default
        try? fm.createDirectory(atPath: logPath, withIntermediateDirectories: true)
        let file = (logPath as NSString).appendingPathComponent("monitor-rs-swift.log")
        if let data = line.data(using: .utf8) {
            if fm.fileExists(atPath: file) {
                if let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: file)) {
                    handle.seekToEndOfFile()
                    handle.write(data)
                    try? handle.close()
                }
            } else {
                try? data.write(to: URL(fileURLWithPath: file))
            }
        }
    }
}
