import Foundation

/// Decides which metric is the popover's "hero" — i.e., the one shown big.
/// Auto-promotes the most-loaded metric with hysteresis to avoid flicker;
/// a manual pin overrides the auto behavior.
///
/// Not thread-safe. Call from the main actor (the view model).
public final class HeroSelector {
    /// The metric currently selected as hero.
    public private(set) var current: MetricKind = .cpu

    /// Score-difference required between a non-current candidate and the
    /// current hero before the candidate starts accumulating ticks.
    public static let threshold: Double = 0.05

    /// Number of consecutive ticks a candidate must lead before it becomes
    /// the new hero. Extended when VoiceOver is active so the screen
    /// reader doesn't get interrupted mid-utterance.
    public static let standardHysteresisTicks: Int = 5
    public static let voiceoverHysteresisTicks: Int = 15

    private var pinned: MetricKind? = nil
    private var leadingCandidate: MetricKind? = nil
    private var leadingTicks: Int = 0

    public init() {}

    /// Returns the (possibly updated) hero after observing this tick.
    @discardableResult
    public func observe(scores: LoadScores, voiceoverEnabled: Bool = false) -> MetricKind {
        if let pinned = pinned {
            current = pinned
            leadingCandidate = nil
            leadingTicks = 0
            return current
        }

        let requiredTicks = voiceoverEnabled
            ? Self.voiceoverHysteresisTicks
            : Self.standardHysteresisTicks
        let currentScore = scores.score(for: current)

        // Find the strongest non-current candidate that beats the current
        // hero by at least the threshold.
        var topCandidate: MetricKind? = nil
        var topMargin: Double = Self.threshold - 1e-12  // exclusive lower bound
        for kind in MetricKind.allCases where kind != current {
            let margin = scores.score(for: kind) - currentScore
            if margin >= Self.threshold && margin > topMargin {
                topCandidate = kind
                topMargin = margin
            }
        }

        guard let candidate = topCandidate else {
            leadingCandidate = nil
            leadingTicks = 0
            return current
        }

        if candidate == leadingCandidate {
            leadingTicks += 1
        } else {
            leadingCandidate = candidate
            leadingTicks = 1
        }

        if leadingTicks >= requiredTicks {
            current = candidate
            leadingCandidate = nil
            leadingTicks = 0
        }
        return current
    }

    /// Pin a specific metric as the hero. Subsequent `observe(...)` calls
    /// will keep returning this kind until `unpin()` is called.
    public func pin(_ kind: MetricKind) {
        pinned = kind
        current = kind
        leadingCandidate = nil
        leadingTicks = 0
    }

    /// Remove a manual pin. Auto-promotion resumes from the next `observe(...)`.
    public func unpin() {
        pinned = nil
    }

    public var isPinned: Bool { pinned != nil }
}
