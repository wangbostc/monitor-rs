import SwiftUI

public enum MetricKind: String, CaseIterable, Sendable, Hashable {
    case cpu, gpu, mem, net, disk

    public var displayLabel: String {
        switch self {
        case .cpu: return "CPU"
        case .gpu: return "GPU"
        case .mem: return "MEM"
        case .net: return "NET"
        case .disk: return "DSK"
        }
    }

    /// System-palette color for this metric. Adapts across light/dark mode.
    /// Memory pressure is applied separately at the view layer.
    public var color: Color {
        switch self {
        case .cpu: return .green
        case .gpu: return .blue
        case .mem: return .orange
        case .net: return .teal
        case .disk: return .purple
        }
    }
}
