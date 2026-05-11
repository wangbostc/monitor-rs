import Foundation

/// Per-metric load score in `[0, 1]`. Used by `HeroSelector` to decide
/// which metric to promote.
public struct LoadScores: Sendable, Equatable {
    public let cpu: Double
    public let gpu: Double
    public let mem: Double
    public let net: Double
    public let disk: Double

    public init(cpu: Double, gpu: Double, mem: Double, net: Double, disk: Double) {
        self.cpu = cpu
        self.gpu = gpu
        self.mem = mem
        self.net = net
        self.disk = disk
    }

    public static let zero = LoadScores(cpu: 0, gpu: 0, mem: 0, net: 0, disk: 0)

    public func score(for kind: MetricKind) -> Double {
        switch kind {
        case .cpu:  return cpu
        case .gpu:  return gpu
        case .mem:  return mem
        case .net:  return net
        case .disk: return disk
        }
    }
}
