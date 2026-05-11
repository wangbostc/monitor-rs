import Foundation
import Observation
import MonitorRSC
import MonitorRSLogic

/// Snapshot the SwiftUI view tree binds to. Updated from the main-thread
/// timer in MenuBarController. Using @Observable (macOS 14+) so SwiftUI
/// tracks reads automatically.
@Observable
@MainActor
final class MonitorViewModel {
    var latest: MrsSample? = nil
    var recent: [MrsSample] = []

    /// Currently-promoted metric. Set by `refresh(latest:recent:)` after the
    /// selector observes the new sample.
    var hero: MetricKind = .cpu

    /// Set by `PopoverView` from `@Environment(\.accessibilityVoiceOverEnabled)`
    /// so the selector can extend its hysteresis window.
    var voiceoverEnabled: Bool = false

    private let selector = HeroSelector()

    /// Pin a metric as the hero. Called from `MetricPill` taps.
    func pin(_ kind: MetricKind) {
        selector.pin(kind)
        hero = selector.current
    }

    /// Release a manual pin and resume auto-promotion.
    func unpinHero() {
        selector.unpin()
        if let latest {
            hero = selector.observe(
                scores: loadScores(from: latest),
                voiceoverEnabled: voiceoverEnabled
            )
        }
    }

    var isHeroPinned: Bool { selector.isPinned }

    /// Apply a new sample: update `latest`, append to `recent`, then ask
    /// the selector for the new hero.
    func refresh(latest: MrsSample?, recent: [MrsSample]) {
        self.latest = latest
        self.recent = recent
        guard let latest else { return }
        let scores = loadScores(from: latest)
        let next = selector.observe(scores: scores, voiceoverEnabled: voiceoverEnabled)
        if next != hero {
            hero = next
        }
    }

    /// Returns just the CPU totals from recent samples, oldest first.
    var cpuHistory: [Float] { recent.map { $0.cpu_total_pct } }
    var gpuHistory: [Float] { recent.map { $0.gpu_present == 1 ? $0.gpu_pct : 0.0 } }
    var memHistory: [Float] {
        recent.map { s -> Float in
            guard s.mem_total_bytes > 0 else { return 0 }
            return Float(Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0)
        }
    }

    /// Combined network throughput (rx + tx) in MB/s, oldest first.
    var netHistory: [Float] {
        recent.map { s in
            Float(s.net_rx_bps + s.net_tx_bps) / Float(1024 * 1024)
        }
    }

    /// Combined disk throughput (read + write) in MB/s, oldest first.
    var diskHistory: [Float] {
        recent.map { s in
            Float(s.disk_read_bps + s.disk_write_bps) / Float(1024 * 1024)
        }
    }

    /// Floor of 1 MB/s on the rolling peak so an idle window doesn't
    /// produce wild load scores from sub-MB blips.
    private static let ioPeakFloorMBs: Float = 1.0

    private func loadScores(from sample: MrsSample) -> LoadScores {
        let cpu = Double(sample.cpu_total_pct) / 100.0
        let gpu = sample.gpu_present == 1 ? Double(sample.gpu_pct) / 100.0 : 0.0
        let mem: Double
        if sample.mem_total_bytes > 0 {
            mem = Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes)
        } else {
            mem = 0
        }

        let netPeak = max(Self.ioPeakFloorMBs, netHistory.max() ?? 0)
        let curNetMBs = Float(sample.net_rx_bps + sample.net_tx_bps) / Float(1024 * 1024)
        let net = Double(curNetMBs / netPeak)

        let diskPeak = max(Self.ioPeakFloorMBs, diskHistory.max() ?? 0)
        let curDiskMBs = Float(sample.disk_read_bps + sample.disk_write_bps) / Float(1024 * 1024)
        let disk = Double(curDiskMBs / diskPeak)

        return LoadScores(
            cpu: max(0, min(1, cpu)),
            gpu: max(0, min(1, gpu)),
            mem: max(0, min(1, mem)),
            net: max(0, min(1, net)),
            disk: max(0, min(1, disk))
        )
    }
}
