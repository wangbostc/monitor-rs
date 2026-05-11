import Foundation
import Observation
import MonitorRSC

/// Snapshot the SwiftUI view tree binds to. Updated from the main-thread
/// timer in MenuBarController. Using @Observable (macOS 14+) so SwiftUI
/// tracks reads automatically.
@Observable
final class MonitorViewModel {
    var latest: MrsSample? = nil
    var recent: [MrsSample] = []

    /// Returns just the CPU totals from recent samples, oldest first.
    var cpuHistory: [Float] { recent.map { $0.cpu_total_pct } }
    var gpuHistory: [Float] { recent.map { $0.gpu_present == 1 ? $0.gpu_pct : 0.0 } }
    var memHistory: [Float] {
        recent.map { s -> Float in
            guard s.mem_total_bytes > 0 else { return 0 }
            return Float(Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0)
        }
    }
}
