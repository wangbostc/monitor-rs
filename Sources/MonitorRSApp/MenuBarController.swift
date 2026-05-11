import AppKit
import SwiftUI
import MonitorRSC

@MainActor
final class MenuBarController: NSObject, NSPopoverDelegate {
    private let statusItem: NSStatusItem
    private let popover: NSPopover
    private let bridge: RustBridge?
    private let viewModel = MonitorViewModel()
    private var refreshTimer: Timer?

    /// Cached last-rendered status item title. Writing to
    /// `statusItem.button?.title` invalidates the cell and triggers a
    /// CoreAnimation transaction; skipping the write when the string is
    /// unchanged eliminates the dominant CPU cost of the refresh loop.
    private var lastStatusTitle: String = ""

    /// Hot/idle refresh intervals. Hot fires while the popover is open
    /// (sparklines need fresh data); idle fires only fast enough to keep
    /// the menu bar rotation feeling alive.
    private static let hotRefreshIntervalSeconds: TimeInterval = 0.25
    private static let idleRefreshIntervalSeconds: TimeInterval = 1.0

    override init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        bridge = RustBridge()

        popover = NSPopover()
        popover.contentSize = NSSize(width: 300, height: 360)
        popover.behavior = .transient
        popover.animates = true

        super.init()

        // Content controller is built lazily on first open and torn down
        // on close via `popoverDidClose` so SwiftUI doesn't hold the view
        // tree, AttributeGraph, and backing layers while the popover isn't
        // visible. The delegate catches both our toggle-driven closes and
        // user dismissals (click outside).
        popover.delegate = self

        if let button = statusItem.button {
            // SF Symbol icon stays visible even when menu bar is crowded
            // (notched Mac with many items). Title text shows live numbers
            // alongside it once samples arrive.
            let icon = NSImage(systemSymbolName: "gauge.with.dots.needle.50percent",
                               accessibilityDescription: "monitor-rs")
            icon?.isTemplate = true  // tints with menu bar foreground (Light/Dark aware)
            button.image = icon
            button.imagePosition = .imageLeft
            // Monospaced digits so the title doesn't shuffle as values tick.
            button.font = Self.statusItemFont
            button.title = "—"  // narrow placeholder until first sample arrives
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        // Lock the status item to the widest possible rotation entry so the
        // popover anchor doesn't shift horizontally as the title rotates.
        statusItem.length = Self.statusItemFixedLength

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
            // Kick the sampler back into full-fidelity mode *before* showing
            // the popover so the first tick after open is fresh.
            bridge?.setActive(true)
            popover.contentViewController = NSHostingController(
                rootView: PopoverView(model: viewModel, onQuit: {
                    NSApp.terminate(nil)
                })
            )
            popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
            restartRefreshLoop(interval: Self.hotRefreshIntervalSeconds)
            refreshTick()
        }
    }

    // MARK: - NSPopoverDelegate

    /// Fired once the close animation finishes — for both user dismissal
    /// (click outside / .transient behavior) and our programmatic close.
    /// Tearing down the hosting controller here is the single place that
    /// releases SwiftUI's view tree, AttributeGraph state, and CoreAnimation
    /// backing layers; without it, opening the popover once permanently
    /// inflates the process's phys_footprint by ~100 MB.
    func popoverDidClose(_ notification: Notification) {
        // Idle cadence — sparklines aren't visible.
        restartRefreshLoop(interval: Self.idleRefreshIntervalSeconds)
        // Tell the Rust sampler it can skip the expensive process refresh
        // until we open again.
        bridge?.setActive(false)
        // Drop the SwiftUI view tree and the in-memory sample history.
        popover.contentViewController = nil
        viewModel.recent = []
    }

    private func startRefreshLoop() {
        restartRefreshLoop(interval: Self.idleRefreshIntervalSeconds)
    }

    private func restartRefreshLoop(interval: TimeInterval) {
        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: interval, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshTick() }
        }
    }

    private func refreshTick() {
        guard let bridge = bridge else { return }
        let latest = bridge.latest()

        // Sparkline history is only consumed by the popover view tree, so
        // skip the 120-sample copy (~300 KB / call) when the popover is
        // closed. The latest sample is still required for the status item.
        if popover.isShown {
            let recent = bridge.recent(120)
            viewModel.refresh(latest: latest, recent: recent)
        } else {
            viewModel.latest = latest
        }

        if let s = latest {
            let index = Int(Date().timeIntervalSinceReferenceDate / Self.rotationPeriodSeconds) % 7
            let title = MenuBarController.formatStatus(s, index: index)
            // Avoid dirtying the cell (and re-running the CA transaction)
            // when nothing visible changed.
            if title != lastStatusTitle {
                statusItem.button?.title = title
                lastStatusTitle = title
            }
        }
    }

    /// Seconds each metric is shown before rotating to the next.
    private static let rotationPeriodSeconds: TimeInterval = 2.0

    /// Monospaced-digit menu-bar font so digits don't shuffle as values tick.
    private static let statusItemFont: NSFont = NSFont.monospacedDigitSystemFont(
        ofSize: NSFont.menuBarFont(ofSize: 0).pointSize,
        weight: .regular
    )

    /// Fixed pixel width for the status item, computed from the widest title
    /// the rotation can produce ("NET ↓99.9 ↑99.9"). Locking the length keeps
    /// the button frame stable so the popover anchor doesn't drift.
    private static let statusItemFixedLength: CGFloat = {
        let longest = "NET ↓99.9 ↑99.9"
        let textWidth = (longest as NSString)
            .size(withAttributes: [.font: statusItemFont]).width
        let iconSlot: CGFloat = 22  // gauge symbol + a little breathing room
        let edgePadding: CGFloat = 8
        return ceil(iconSlot + textWidth + edgePadding)
    }()

    /// Compact status text shown alongside the gauge icon. Rotates through
    /// CPU / GPU / MEM / NET / DSK / BAT / TMP so the item stays narrow enough
    /// to fit right of the camera notch on notched MacBook Pros.
    static func formatStatus(_ s: MrsSample, index: Int) -> String {
        switch index % 7 {
        case 0:
            return "CPU \(Int(s.cpu_total_pct.rounded()))%"
        case 1:
            return s.gpu_present == 1
                ? "GPU \(Int(s.gpu_pct.rounded()))%"
                : "GPU —"
        case 2:
            let memPct = s.mem_total_bytes > 0
                ? Int((Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0).rounded())
                : 0
            return "MEM \(memPct)%"
        case 3:
            return "NET ↓\(formatRateMB(s.net_rx_bps)) ↑\(formatRateMB(s.net_tx_bps))"
        case 4:
            return "DSK ↓\(formatRateMB(s.disk_read_bps)) ↑\(formatRateMB(s.disk_write_bps))"
        case 5:
            guard s.battery_present == 1 else { return "BAT —" }
            let pct = Int(s.battery_pct.rounded())
            return s.battery_charging == 1 ? "BAT \(pct)%⚡" : "BAT \(pct)%"
        default: // 6
            guard s.cpu_temp_present == 1 else { return "TMP —" }
            return "TMP \(Int(s.cpu_temp_c.rounded()))°C"
        }
    }

    /// Format a bytes-per-second value as MB/s with one decimal, clamping to
    /// "0.0" below 0.05 MB/s to avoid jitter.
    private static func formatRateMB(_ bps: UInt64) -> String {
        let mb = Double(bps) / (1024.0 * 1024.0)
        if mb < 0.05 { return "0.0" }
        return String(format: "%.1f", mb)
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
