# Popover Hero Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the popover's three-tile-plus-two-tile grid with a hero card for the metric under most pressure, four pills for the rest, and CPU per-core grid surfaced only when CPU is hero.

**Architecture:** Pure-Swift `HeroSelector` (auto-promote with hysteresis + manual pin) lives in a new `MonitorRSLogic` library target so it can be unit-tested without linking the Rust FFI. SwiftUI views in `MonitorRSApp` consume `MetricKind` and `HeroSelector` via the model. Swift Charts renders the hero sparkline; existing components (`CoreGrid`, `ProcessList`, `HeaderStrip`, `FooterStrip`) are reused unchanged.

**Tech Stack:** Swift 5.10+, SwiftUI on macOS 14, Swift Charts (system framework, no third-party), `@Observable` (Observation framework). No Rust / FFI changes.

**Spec:** `docs/superpowers/specs/2026-05-11-popover-hero-redesign-design.md`

---

## File Structure

**New files:**

```
Sources/MonitorRSLogic/MetricKind.swift                  # enum + label + Color
Sources/MonitorRSLogic/LoadScores.swift                  # 5-metric score container
Sources/MonitorRSLogic/HeroSelector.swift                # auto-promote + pin
Sources/MonitorRSApp/Components/HeroChart.swift          # Swift Charts area+line
Sources/MonitorRSApp/Components/HeroCard.swift           # tinted card around HeroChart
Sources/MonitorRSApp/Components/MetricPill.swift         # one pill
Sources/MonitorRSApp/Components/PillsRow.swift           # 4 non-hero pills
Tests/MonitorRSLogicTests/HeroSelectorTests.swift        # HeroSelector unit tests
```

**Modified files:**

```
Package.swift                                            # add library + test target
Sources/MonitorRSApp/ViewModel.swift                     # +hero, +pin, +loadScores
Sources/MonitorRSApp/PopoverView.swift                   # rewritten composer
README.md                                                # smoke checklist updates
```

**Deleted files:**

```
Sources/MonitorRSApp/Components/MetricTile.swift
```

**Unchanged:** `CoreGrid.swift`, `ProcessList.swift`, `HeaderStrip.swift`, `FooterStrip.swift`, `Sparkline.swift` (retained for future micro-spark use), `MenuBarController.swift`, `RustBridge.swift`, `LoginItem.swift`, `App.swift`, `AppDelegate.swift`.

---

## Task 1: Split logic into a library target + add test target

**Files:**
- Modify: `Package.swift`
- Create: `Sources/MonitorRSLogic/.gitkeep` (placeholder so the target compiles before any sources land)
- Create: `Tests/MonitorRSLogicTests/.gitkeep`

- [ ] **Step 1: Create the new directories with a placeholder source**

`Sources/MonitorRSLogic/` and `Tests/MonitorRSLogicTests/` must exist before SwiftPM will accept the new targets. Create a placeholder Swift file in each so the targets compile empty.

Create `Sources/MonitorRSLogic/Placeholder.swift`:

```swift
// Placeholder so SwiftPM accepts the empty target. Removed once
// MetricKind.swift is added.
```

Create `Tests/MonitorRSLogicTests/PlaceholderTests.swift`:

```swift
import XCTest

final class PlaceholderTests: XCTestCase {
    func testPlaceholder() {
        XCTAssertTrue(true)
    }
}
```

- [ ] **Step 2: Update Package.swift**

Replace the full contents of `Package.swift` with:

```swift
// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "MonitorRSApp",
    platforms: [.macOS(.v14)],
    targets: [
        .target(
            name: "MonitorRSC",
            path: "Sources/MonitorRSC",
            publicHeadersPath: "include"
        ),
        .target(
            name: "MonitorRSLogic",
            path: "Sources/MonitorRSLogic"
        ),
        .testTarget(
            name: "MonitorRSLogicTests",
            dependencies: ["MonitorRSLogic"],
            path: "Tests/MonitorRSLogicTests"
        ),
        .executableTarget(
            name: "MonitorRSApp",
            dependencies: ["MonitorRSC", "MonitorRSLogic"],
            path: "Sources/MonitorRSApp",
            linkerSettings: [
                .linkedLibrary("monitor_rs"),
                .unsafeFlags(["-L", "target/release"])
            ]
        )
    ]
)
```

- [ ] **Step 3: Verify the test target builds and the placeholder test passes**

Run:

```bash
swift test --filter MonitorRSLogicTests
```

Expected: `Test Suite 'MonitorRSLogicTests' passed` with `PlaceholderTests.testPlaceholder` passing.

Note: `swift test` of the full package will fail because the executable target links against `target/release/libmonitor_rs.a`, which is built by `cargo`. The `--filter` keeps us scoped to the logic test target which has no FFI dependency.

- [ ] **Step 4: Commit**

```bash
git add Package.swift Sources/MonitorRSLogic/ Tests/MonitorRSLogicTests/
git commit -m "build: add MonitorRSLogic library + test target"
```

---

## Task 2: Define `MetricKind`

**Files:**
- Create: `Sources/MonitorRSLogic/MetricKind.swift`
- Delete: `Sources/MonitorRSLogic/Placeholder.swift`
- Modify: `Tests/MonitorRSLogicTests/PlaceholderTests.swift` → `Tests/MonitorRSLogicTests/MetricKindTests.swift`

- [ ] **Step 1: Write the failing test**

Replace `Tests/MonitorRSLogicTests/PlaceholderTests.swift` with `Tests/MonitorRSLogicTests/MetricKindTests.swift`:

```swift
import XCTest
@testable import MonitorRSLogic

final class MetricKindTests: XCTestCase {
    func testAllCasesArePresent() {
        XCTAssertEqual(MetricKind.allCases, [.cpu, .gpu, .mem, .net, .disk])
    }

    func testDisplayLabels() {
        XCTAssertEqual(MetricKind.cpu.displayLabel, "CPU")
        XCTAssertEqual(MetricKind.gpu.displayLabel, "GPU")
        XCTAssertEqual(MetricKind.mem.displayLabel, "MEM")
        XCTAssertEqual(MetricKind.net.displayLabel, "NET")
        XCTAssertEqual(MetricKind.disk.displayLabel, "DSK")
    }
}
```

Delete the old file:

```bash
rm Tests/MonitorRSLogicTests/PlaceholderTests.swift
```

- [ ] **Step 2: Run test to verify it fails**

```bash
swift test --filter MetricKindTests
```

Expected: FAIL — "cannot find 'MetricKind' in scope".

- [ ] **Step 3: Implement MetricKind**

Delete `Sources/MonitorRSLogic/Placeholder.swift`:

```bash
rm Sources/MonitorRSLogic/Placeholder.swift
```

Create `Sources/MonitorRSLogic/MetricKind.swift`:

```swift
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
```

- [ ] **Step 4: Run test to verify it passes**

```bash
swift test --filter MetricKindTests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSLogic/ Tests/MonitorRSLogicTests/
git commit -m "feat(logic): add MetricKind enum"
```

---

## Task 3: Define `LoadScores`

**Files:**
- Create: `Sources/MonitorRSLogic/LoadScores.swift`
- Create: `Tests/MonitorRSLogicTests/LoadScoresTests.swift`

- [ ] **Step 1: Write the failing test**

Create `Tests/MonitorRSLogicTests/LoadScoresTests.swift`:

```swift
import XCTest
@testable import MonitorRSLogic

final class LoadScoresTests: XCTestCase {
    func testScoreLookup() {
        let s = LoadScores(cpu: 0.42, gpu: 0.08, mem: 0.61, net: 0.10, disk: 0.05)
        XCTAssertEqual(s.score(for: .cpu), 0.42, accuracy: 1e-9)
        XCTAssertEqual(s.score(for: .gpu), 0.08, accuracy: 1e-9)
        XCTAssertEqual(s.score(for: .mem), 0.61, accuracy: 1e-9)
        XCTAssertEqual(s.score(for: .net), 0.10, accuracy: 1e-9)
        XCTAssertEqual(s.score(for: .disk), 0.05, accuracy: 1e-9)
    }

    func testZero() {
        XCTAssertEqual(LoadScores.zero.score(for: .cpu), 0)
        XCTAssertEqual(LoadScores.zero.score(for: .disk), 0)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
swift test --filter LoadScoresTests
```

Expected: FAIL — "cannot find 'LoadScores' in scope".

- [ ] **Step 3: Implement LoadScores**

Create `Sources/MonitorRSLogic/LoadScores.swift`:

```swift
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
```

- [ ] **Step 4: Run test to verify it passes**

```bash
swift test --filter LoadScoresTests
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSLogic/LoadScores.swift Tests/MonitorRSLogicTests/LoadScoresTests.swift
git commit -m "feat(logic): add LoadScores container"
```

---

## Task 4: `HeroSelector` — default state and single-tick spike rejection

**Files:**
- Create: `Sources/MonitorRSLogic/HeroSelector.swift`
- Create: `Tests/MonitorRSLogicTests/HeroSelectorTests.swift`

- [ ] **Step 1: Write the failing tests**

Create `Tests/MonitorRSLogicTests/HeroSelectorTests.swift`:

```swift
import XCTest
@testable import MonitorRSLogic

final class HeroSelectorTests: XCTestCase {
    func testDefaultsToCPU() {
        let s = HeroSelector()
        XCTAssertEqual(s.current, .cpu)
    }

    func testSingleSpikeDoesNotPromote() {
        let s = HeroSelector()
        // NET briefly dominates for one tick — should NOT promote.
        _ = s.observe(scores: LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0))
        XCTAssertEqual(s.current, .cpu)
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
swift test --filter HeroSelectorTests
```

Expected: FAIL — "cannot find 'HeroSelector' in scope".

- [ ] **Step 3: Implement the minimal HeroSelector**

Create `Sources/MonitorRSLogic/HeroSelector.swift`:

```swift
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
    /// the new hero. Doubled (effectively) when VoiceOver is active so
    /// the screen reader doesn't get interrupted mid-utterance.
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
            // No qualifying candidate — reset hysteresis counters.
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
```

- [ ] **Step 4: Run test to verify it passes**

```bash
swift test --filter HeroSelectorTests
```

Expected: PASS (two tests).

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSLogic/HeroSelector.swift Tests/MonitorRSLogicTests/HeroSelectorTests.swift
git commit -m "feat(logic): HeroSelector defaults to CPU, ignores single-tick spikes"
```

---

## Task 5: `HeroSelector` — sustained-lead promotion (5 ticks, ≥ 0.05 margin)

**Files:**
- Modify: `Tests/MonitorRSLogicTests/HeroSelectorTests.swift`

- [ ] **Step 1: Append failing tests**

Append to `Tests/MonitorRSLogicTests/HeroSelectorTests.swift` (inside the `HeroSelectorTests` class):

```swift
    func testFiveTickLeadPromotes() {
        let s = HeroSelector()
        let hot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        for _ in 0..<4 {
            _ = s.observe(scores: hot)
            XCTAssertEqual(s.current, .cpu, "should not promote before 5 ticks")
        }
        _ = s.observe(scores: hot)
        XCTAssertEqual(s.current, .net, "should promote on the 5th sustained tick")
    }

    func testMarginBelowThresholdNeverPromotes() {
        let s = HeroSelector()
        // NET is 0.03 ahead of CPU — under the 0.05 threshold.
        let close = LoadScores(cpu: 0.50, gpu: 0, mem: 0, net: 0.53, disk: 0)
        for _ in 0..<20 {
            _ = s.observe(scores: close)
        }
        XCTAssertEqual(s.current, .cpu)
    }

    func testInterruptedLeadResetsCounter() {
        let s = HeroSelector()
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        let calm   = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.10, disk: 0)
        for _ in 0..<3 { _ = s.observe(scores: netHot) }
        // One calm tick wipes the counter.
        _ = s.observe(scores: calm)
        // Five more hot ticks should be needed.
        for _ in 0..<4 {
            _ = s.observe(scores: netHot)
            XCTAssertEqual(s.current, .cpu)
        }
        _ = s.observe(scores: netHot)
        XCTAssertEqual(s.current, .net)
    }
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
swift test --filter HeroSelectorTests
```

Expected: PASS — all 5 tests now pass (the implementation from Task 4 already handles these cases).

- [ ] **Step 3: Commit**

```bash
git add Tests/MonitorRSLogicTests/HeroSelectorTests.swift
git commit -m "test(logic): cover HeroSelector hysteresis (5 ticks, 0.05 margin)"
```

---

## Task 6: `HeroSelector` — pin and unpin

**Files:**
- Modify: `Tests/MonitorRSLogicTests/HeroSelectorTests.swift`

- [ ] **Step 1: Append failing tests**

Append to `HeroSelectorTests`:

```swift
    func testPinOverridesAuto() {
        let s = HeroSelector()
        s.pin(.gpu)
        // Even with NET dominating, hero stays GPU.
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.95, disk: 0)
        for _ in 0..<20 { _ = s.observe(scores: netHot) }
        XCTAssertEqual(s.current, .gpu)
        XCTAssertTrue(s.isPinned)
    }

    func testUnpinResumesAuto() {
        let s = HeroSelector()
        s.pin(.gpu)
        s.unpin()
        XCTAssertFalse(s.isPinned)
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        for _ in 0..<5 { _ = s.observe(scores: netHot) }
        XCTAssertEqual(s.current, .net)
    }

    func testPinResetsHysteresisCounter() {
        let s = HeroSelector()
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        for _ in 0..<4 { _ = s.observe(scores: netHot) }
        s.pin(.disk)
        s.unpin()
        // Counter was reset — needs another 5 sustained ticks to promote NET.
        for _ in 0..<4 {
            _ = s.observe(scores: netHot)
            XCTAssertNotEqual(s.current, .net)
        }
        _ = s.observe(scores: netHot)
        XCTAssertEqual(s.current, .net)
    }
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
swift test --filter HeroSelectorTests
```

Expected: PASS — all 8 tests pass (the Task 4 implementation already handles pin/unpin).

- [ ] **Step 3: Commit**

```bash
git add Tests/MonitorRSLogicTests/HeroSelectorTests.swift
git commit -m "test(logic): cover HeroSelector pin/unpin overrides"
```

---

## Task 7: `HeroSelector` — VoiceOver extends hysteresis to 15 ticks

**Files:**
- Modify: `Tests/MonitorRSLogicTests/HeroSelectorTests.swift`

- [ ] **Step 1: Append failing tests**

Append to `HeroSelectorTests`:

```swift
    func testVoiceOverExtendsHysteresis() {
        let s = HeroSelector()
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        // 14 ticks with VoiceOver on — should NOT promote yet.
        for _ in 0..<14 {
            _ = s.observe(scores: netHot, voiceoverEnabled: true)
            XCTAssertEqual(s.current, .cpu)
        }
        _ = s.observe(scores: netHot, voiceoverEnabled: true)
        XCTAssertEqual(s.current, .net, "promotes on 15th tick when VoiceOver on")
    }

    func testVoiceOverFlagCanBeToggledMidStream() {
        let s = HeroSelector()
        let netHot = LoadScores(cpu: 0.10, gpu: 0, mem: 0, net: 0.90, disk: 0)
        // 5 ticks with VoiceOver off would normally promote, but here
        // we never give it 5 consecutive off-ticks.
        _ = s.observe(scores: netHot, voiceoverEnabled: false)
        _ = s.observe(scores: netHot, voiceoverEnabled: true)
        _ = s.observe(scores: netHot, voiceoverEnabled: false)
        _ = s.observe(scores: netHot, voiceoverEnabled: true)
        _ = s.observe(scores: netHot, voiceoverEnabled: false)
        // 5 hot ticks total — promotion happens on the 5th because the
        // current evaluation tick had voiceoverEnabled: false.
        XCTAssertEqual(s.current, .net)
    }
```

- [ ] **Step 2: Run tests to verify they pass**

```bash
swift test --filter HeroSelectorTests
```

Expected: PASS — all 10 tests pass. The implementation from Task 4 already reads `voiceoverEnabled` per call, so the threshold applied each tick is the one passed in.

- [ ] **Step 3: Commit**

```bash
git add Tests/MonitorRSLogicTests/HeroSelectorTests.swift
git commit -m "test(logic): cover HeroSelector VoiceOver-extended hysteresis"
```

---

## Task 8: Wire `HeroSelector` into `MonitorViewModel`

**Files:**
- Modify: `Sources/MonitorRSApp/ViewModel.swift`

- [ ] **Step 1: Update ViewModel.swift**

Replace the contents of `Sources/MonitorRSApp/ViewModel.swift` with:

```swift
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

    /// Currently-promoted metric. Set by `refresh(with:)` after the
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
        // Re-evaluate immediately so the hero doesn't stay stuck if the
        // underlying load has shifted.
        if let latest {
            hero = selector.observe(scores: loadScores(from: latest), voiceoverEnabled: voiceoverEnabled)
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
```

- [ ] **Step 2: Update MenuBarController to use `refresh(latest:recent:)`**

In `Sources/MonitorRSApp/MenuBarController.swift`, replace the body of `refreshTick()`:

Find:

```swift
    private func refreshTick() {
        guard let bridge = bridge else { return }
        let latest = bridge.latest()
        let recent = bridge.recent(120)
        viewModel.latest = latest
        viewModel.recent = recent

        if let s = latest {
```

Replace with:

```swift
    private func refreshTick() {
        guard let bridge = bridge else { return }
        let latest = bridge.latest()
        let recent = bridge.recent(120)
        viewModel.refresh(latest: latest, recent: recent)

        if let s = latest {
```

- [ ] **Step 3: Build to confirm everything still compiles**

The full `swift build` requires the Rust static library. Build the whole thing the way the project normally does:

```bash
./build.sh
```

Expected: success, producing `target/release/monitor-rs.app`. Build time on a clean tree is ~30s.

- [ ] **Step 4: Commit**

```bash
git add Sources/MonitorRSApp/ViewModel.swift Sources/MonitorRSApp/MenuBarController.swift
git commit -m "feat(app): wire HeroSelector into MonitorViewModel"
```

---

## Task 9: Build `HeroChart`

**Files:**
- Create: `Sources/MonitorRSApp/Components/HeroChart.swift`

- [ ] **Step 1: Implement HeroChart**

Create `Sources/MonitorRSApp/Components/HeroChart.swift`:

```swift
import SwiftUI
import Charts

/// Area + line chart used inside `HeroCard`. Tinted via `color`.
///
/// `values` are the recent samples in display units (any scale — the chart
/// auto-scales). They are rendered oldest-on-the-left.
struct HeroChart: View {
    let values: [Float]
    let color: Color

    var body: some View {
        Chart {
            ForEach(Array(values.enumerated()), id: \.offset) { idx, value in
                AreaMark(
                    x: .value("t", idx),
                    y: .value("v", value)
                )
                .foregroundStyle(
                    LinearGradient(
                        colors: [color.opacity(0.55), color.opacity(0)],
                        startPoint: .top,
                        endPoint: .bottom
                    )
                )
                .interpolationMethod(.monotone)

                LineMark(
                    x: .value("t", idx),
                    y: .value("v", value)
                )
                .foregroundStyle(color)
                .interpolationMethod(.monotone)
                .lineStyle(StrokeStyle(lineWidth: 1.4))
            }
        }
        .chartXAxis(.hidden)
        .chartYAxis(.hidden)
        .chartLegend(.hidden)
        .chartPlotStyle { plot in plot.background(Color.clear) }
        .accessibilityHidden(true)   // outer card carries the spoken label
    }
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
./build.sh
```

Expected: success.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/HeroChart.swift
git commit -m "feat(ui): add HeroChart (Swift Charts area+line)"
```

---

## Task 10: Build `HeroCard`

**Files:**
- Create: `Sources/MonitorRSApp/Components/HeroCard.swift`

- [ ] **Step 1: Implement HeroCard**

Create `Sources/MonitorRSApp/Components/HeroCard.swift`:

```swift
import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// The large tinted card showing the currently-promoted metric.
struct HeroCard: View {
    let kind: MetricKind
    let sample: MrsSample
    let history: [Float]
    let isPinned: Bool
    let onTap: () -> Void

    var body: some View {
        let tint = effectiveTint
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(kind.displayLabel)
                        .font(.system(.caption, design: .rounded).weight(.medium))
                        .tracking(0.5)
                        .foregroundStyle(tint)
                    if isPinned {
                        Circle()
                            .fill(tint)
                            .frame(width: 4, height: 4)
                            .accessibilityHidden(true)
                    }
                }
                Text(bigValue)
                    .font(.system(size: 32, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
                Text(metaLine)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            HeroChart(values: history, color: tint)
                .frame(width: 110, height: 60)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 12)
        .background(
            LinearGradient(
                colors: [tint.opacity(0.18), tint.opacity(0.05)],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            ),
            in: RoundedRectangle(cornerRadius: 12, style: .continuous)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(tint.opacity(0.16), lineWidth: 1)
        )
        .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onTapGesture(perform: onTap)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(accessibilityDescription)
        .accessibilityHint(isPinned ? "Activate to unpin" : "Currently auto-selected")
        .accessibilityAddTraits(.isButton)
    }

    /// Memory-pressure-aware tint for MEM; otherwise the metric's own color.
    private var effectiveTint: Color {
        guard kind == .mem else { return kind.color }
        switch sample.mem_pressure {
        case 0:  return .orange
        case 1:  return Color(red: 0.95, green: 0.55, blue: 0.20)
        default: return .red
        }
    }

    private var bigValue: String {
        switch kind {
        case .cpu: return "\(Int(sample.cpu_total_pct.rounded()))%"
        case .gpu:
            return sample.gpu_present == 1
                ? "\(Int(sample.gpu_pct.rounded()))%"
                : "n/a"
        case .mem:
            let pct = sample.mem_total_bytes > 0
                ? Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes) * 100.0
                : 0
            return "\(Int(pct.rounded()))%"
        case .net:
            let mb = Double(sample.net_rx_bps + sample.net_tx_bps) / (1024.0 * 1024.0)
            return String(format: "%.1f MB/s", mb)
        case .disk:
            let mb = Double(sample.disk_read_bps + sample.disk_write_bps) / (1024.0 * 1024.0)
            return String(format: "%.1f MB/s", mb)
        }
    }

    private var metaLine: String {
        switch kind {
        case .cpu:
            let cores = sample.perCoreUsage
            let hottest = Int((cores.max() ?? 0).rounded())
            return "\(cores.count)-core · hot core \(hottest)%"
        case .gpu:
            return sample.gpu_present == 1 ? "Metal active" : "Metal idle"
        case .mem:
            let usedGB = Double(sample.mem_used_bytes) / (1024.0 * 1024.0 * 1024.0)
            let totalGB = Double(sample.mem_total_bytes) / (1024.0 * 1024.0 * 1024.0)
            let pressure: String
            switch sample.mem_pressure {
            case 0:  pressure = "normal"
            case 1:  pressure = "warn"
            default: pressure = "crit"
            }
            return String(format: "%.1f / %.1f GB · pressure %@", usedGB, totalGB, pressure)
        case .net:
            let rx = Double(sample.net_rx_bps) / (1024.0 * 1024.0)
            let tx = Double(sample.net_tx_bps) / (1024.0 * 1024.0)
            let peak = max(1.0, history.map { Double($0) }.max() ?? 0)
            return String(format: "↓ %.1f ↑ %.1f · peak %.1f", rx, tx, peak)
        case .disk:
            let rd = Double(sample.disk_read_bps) / (1024.0 * 1024.0)
            let wr = Double(sample.disk_write_bps) / (1024.0 * 1024.0)
            let peak = max(1.0, history.map { Double($0) }.max() ?? 0)
            return String(format: "↓ %.1f ↑ %.1f · peak %.1f", rd, wr, peak)
        }
    }

    private var accessibilityDescription: String {
        "\(kind.displayLabel), \(bigValue). \(metaLine)."
    }
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
./build.sh
```

Expected: success. The card isn't wired into `PopoverView` yet — that happens in Task 13 — so visual verification has to wait.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/HeroCard.swift
git commit -m "feat(ui): add HeroCard with metric-tinted background"
```

---

## Task 11: Build `MetricPill`

**Files:**
- Create: `Sources/MonitorRSApp/Components/MetricPill.swift`

- [ ] **Step 1: Implement MetricPill**

Create `Sources/MonitorRSApp/Components/MetricPill.swift`:

```swift
import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// One pill in the non-hero row. Tapping pins the metric as hero.
struct MetricPill: View {
    let kind: MetricKind
    let sample: MrsSample
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            VStack(alignment: .leading, spacing: 1) {
                Text(kind.displayLabel)
                    .font(.system(size: 9, design: .rounded).weight(.semibold))
                    .tracking(0.5)
                    .foregroundStyle(kind.color)
                Text(displayValue)
                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                    .monospacedDigit()
                    .foregroundStyle(.primary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(.white.opacity(0.04))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .strokeBorder(.white.opacity(0.05), lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(kind.displayLabel), \(displayValue)")
        .accessibilityHint("Tap to pin as hero")
    }

    private var displayValue: String {
        switch kind {
        case .cpu: return "\(Int(sample.cpu_total_pct.rounded()))%"
        case .gpu:
            return sample.gpu_present == 1
                ? "\(Int(sample.gpu_pct.rounded()))%"
                : "n/a"
        case .mem:
            let pct = sample.mem_total_bytes > 0
                ? Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes) * 100.0
                : 0
            return "\(Int(pct.rounded()))%"
        case .net:
            let mb = Double(sample.net_rx_bps + sample.net_tx_bps) / (1024.0 * 1024.0)
            return String(format: "%.1fM", mb)
        case .disk:
            let mb = Double(sample.disk_read_bps + sample.disk_write_bps) / (1024.0 * 1024.0)
            return String(format: "%.1fM", mb)
        }
    }
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
./build.sh
```

Expected: success.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/MetricPill.swift
git commit -m "feat(ui): add MetricPill"
```

---

## Task 12: Build `PillsRow`

**Files:**
- Create: `Sources/MonitorRSApp/Components/PillsRow.swift`

- [ ] **Step 1: Implement PillsRow**

Create `Sources/MonitorRSApp/Components/PillsRow.swift`:

```swift
import SwiftUI
import MonitorRSC
import MonitorRSLogic

/// Horizontal row of pills for the four non-hero metrics. The order is
/// fixed (CPU, GPU, MEM, NET, DSK with the current hero filtered out)
/// so the layout doesn't churn as the hero swaps.
struct PillsRow: View {
    let hero: MetricKind
    let sample: MrsSample
    let onPin: (MetricKind) -> Void

    var body: some View {
        HStack(spacing: 6) {
            ForEach(MetricKind.allCases.filter { $0 != hero }, id: \.self) { kind in
                MetricPill(kind: kind, sample: sample, onTap: { onPin(kind) })
            }
        }
    }
}
```

- [ ] **Step 2: Build to confirm it compiles**

```bash
./build.sh
```

Expected: success.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/PillsRow.swift
git commit -m "feat(ui): add PillsRow"
```

---

## Task 13: Rewrite `PopoverView` and delete `MetricTile.swift`

**Files:**
- Modify: `Sources/MonitorRSApp/PopoverView.swift`
- Delete: `Sources/MonitorRSApp/Components/MetricTile.swift`

- [ ] **Step 1: Replace PopoverView contents**

Replace the full contents of `Sources/MonitorRSApp/PopoverView.swift` with:

```swift
import SwiftUI
import MonitorRSC
import MonitorRSLogic

struct PopoverView: View {
    @Bindable var model: MonitorViewModel
    let onQuit: () -> Void

    @Environment(\.accessibilityVoiceOverEnabled) private var voiceoverEnabled
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HeaderStrip(onQuit: onQuit)

            if let latest = model.latest {
                heroSection(latest: latest)

                PillsRow(
                    hero: model.hero,
                    sample: latest,
                    onPin: { kind in
                        withAnimation(swapAnimation) { model.pin(kind) }
                    }
                )

                if model.hero == .cpu {
                    CoreGrid(perCoreUsage: latest.perCoreUsage)
                }

                Divider()

                Text("TOP PROCESSES")
                    .font(.system(.caption, design: .rounded).weight(.medium))
                    .tracking(0.5)
                    .foregroundStyle(.secondary)
                ProcessList(procs: latest.topProcesses)

                Divider()

                FooterStrip(
                    swapUsedBytes: latest.swap_used_bytes,
                    swapTotalBytes: latest.swap_total_bytes,
                    sampleRateHz: 1.0,
                    batteryPresent: latest.battery_present == 1,
                    batteryPct: latest.battery_pct,
                    batteryCharging: latest.battery_charging == 1,
                    cpuTempC: latest.cpu_temp_present == 1 ? latest.cpu_temp_c : nil,
                    gpuTempC: latest.gpu_temp_present == 1 ? latest.gpu_temp_c : nil
                )
            } else {
                VStack {
                    Spacer()
                    Text("Sampling…")
                        .foregroundStyle(.secondary)
                    Spacer()
                }
                .frame(maxWidth: .infinity, minHeight: 200)
            }
        }
        .padding(14)
        .frame(width: 300)
        .dynamicTypeSize(...DynamicTypeSize.xLarge)
        .onAppear { model.voiceoverEnabled = voiceoverEnabled }
        .onChange(of: voiceoverEnabled) { _, new in
            model.voiceoverEnabled = new
        }
        .onChange(of: model.hero) { _, _ in
            // Animation handled by the .id() transition below; nothing to do.
        }
    }

    @ViewBuilder
    private func heroSection(latest: MrsSample) -> some View {
        HeroCard(
            kind: model.hero,
            sample: latest,
            history: history(for: model.hero),
            isPinned: model.isHeroPinned,
            onTap: {
                guard model.isHeroPinned else { return }
                withAnimation(swapAnimation) { model.unpinHero() }
            }
        )
        .id(model.hero)
        .transition(.opacity.combined(with: .scale(scale: 0.98)))
    }

    private func history(for kind: MetricKind) -> [Float] {
        switch kind {
        case .cpu:  return model.cpuHistory
        case .gpu:  return model.gpuHistory
        case .mem:  return model.memHistory
        case .net:  return model.netHistory
        case .disk: return model.diskHistory
        }
    }

    private var swapAnimation: Animation? {
        reduceMotion ? nil : .snappy(duration: 0.22)
    }
}
```

- [ ] **Step 2: Delete the no-longer-used MetricTile**

```bash
rm Sources/MonitorRSApp/Components/MetricTile.swift
```

- [ ] **Step 3: Build to confirm everything compiles**

```bash
./build.sh
```

Expected: success.

- [ ] **Step 4: Smoke-test the popover visually**

```bash
open target/release/monitor-rs.app
```

Click the menu bar icon and confirm:

1. Header strip looks the same as before (wordmark + gear + quit).
2. A single large tinted hero card is visible (CPU by default, green tint).
3. Four pills sit below (GPU, MEM, NET, DSK).
4. Per-core grid is visible (because CPU is hero).
5. Top processes and footer look the same as before.
6. Tap a pill (e.g., GPU) → the hero swaps to GPU with a brief fade. A small dot appears next to the GPU label indicating it's pinned. The CPU pill reappears in the pills row; the per-core grid disappears.
7. Tap the hero card → unpins. Auto-promotion resumes.

If any of these are wrong, stop and report before committing.

Quit the running app from its menu before continuing so further builds don't fight an in-use bundle:

```bash
osascript -e 'tell application "monitor-rs" to quit' 2>/dev/null || true
```

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSApp/PopoverView.swift Sources/MonitorRSApp/Components/MetricTile.swift
git commit -m "feat(ui): replace tile grid with hero + pills layout"
```

---

## Task 14: SwiftUI-pro polish pass

**Files:**
- Audit & modify (as needed): all files under `Sources/MonitorRSApp/`

- [ ] **Step 1: Audit for `foregroundColor` usage**

Run:

```bash
rg "foregroundColor\(" Sources/MonitorRSApp/ Sources/MonitorRSLogic/
```

Expected: no matches. If anything turns up, replace with `foregroundStyle(...)` using the same argument. (At the time the plan was written, the project already uses `foregroundStyle` throughout — this step is a guard.)

- [ ] **Step 2: Audit accessibility labels on icon-only buttons**

Open `Sources/MonitorRSApp/Components/HeaderStrip.swift` and verify both buttons have `accessibilityLabel(...)` or a string title. If the gear/quit buttons are still icon-only without labels, add labels:

```swift
Button(action: {}) {
    Image(systemName: "gearshape")
        .font(.caption)
        .foregroundStyle(.secondary)
}
.buttonStyle(.plain)
.help("Settings (coming soon)")
.disabled(true)
.accessibilityLabel("Settings (coming soon)")

Button(action: onQuit) {
    Image(systemName: "power")
        .font(.caption)
        .foregroundStyle(.secondary)
}
.buttonStyle(.plain)
.help("Quit monitor-rs")
.accessibilityLabel("Quit monitor-rs")
```

Only edit if labels are missing.

- [ ] **Step 3: Build and re-verify**

```bash
./build.sh
```

Expected: success.

- [ ] **Step 4: Commit (only if Step 2 changed anything)**

```bash
git add Sources/MonitorRSApp/Components/HeaderStrip.swift
git commit -m "chore(a11y): add VoiceOver labels to header buttons"
```

If there were no changes, skip the commit.

---

## Task 15: Update README smoke checklist

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the popover-related rows in the smoke checklist**

In `README.md`, find the section starting with `## Smoke test checklist` and locate these existing bullets:

```
- [ ] Clicking the status item shows a translucent popover anchored beneath it.
- [ ] Popover row 1 shows three summary tiles (CPU / GPU / MEM) with
      sparklines and a per-core grid under the CPU tile.
- [ ] Popover row 2 shows two tiles (NET / DSK) with sparklines that
      auto-scale to the recent peak.
```

Replace them with:

```
- [ ] Clicking the status item shows a translucent popover anchored beneath it.
- [ ] Popover shows ONE large tinted hero card (CPU by default — green) with
      a big percentage, a meta line (`N-core · hot core M%`), and an area
      sparkline on the right.
- [ ] Below the hero, four pills (GPU / MEM / NET / DSK) show their
      current values.
- [ ] Per-core grid appears only when CPU is the hero, directly under the
      pills.
- [ ] Tapping a pill pins that metric as the new hero with a brief
      fade-in. A small filled dot appears next to its label.
- [ ] Tapping the pinned hero unpins it; auto-promotion resumes.
- [ ] Running `yes > /dev/null` × N keeps CPU as the hero (already #1).
- [ ] Downloading a large file with curl
      (`curl -o /dev/null https://speed.cloudflare.com/__down\?bytes\=200000000`)
      swaps the hero to NET within ~5 s of sustained transfer; ending the
      transfer returns the hero to CPU after the matching hysteresis window.
- [ ] Sustained `dd if=/dev/zero of=/tmp/iotest bs=1m count=2000` swaps the
      hero to DSK; finishing returns it to CPU. Clean up: `rm /tmp/iotest`.
- [ ] With macOS "Reduce motion" enabled (System Settings → Accessibility
      → Display), hero swaps happen with no animation.
```

- [ ] **Step 2: Update the Architecture reference**

In the same file, change:

```
See `docs/superpowers/specs/2026-05-11-swiftui-popover-redesign.md`.
```

to:

```
See `docs/superpowers/specs/2026-05-11-popover-hero-redesign-design.md`
(supersedes the original popover spec for the UI layer).
```

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: update smoke checklist for hero + pills layout"
```

---

## Task 16: Full smoke run

- [ ] **Step 1: Clean build from scratch**

```bash
rm -rf .build target
./build.sh
```

Expected: clean build succeeds in ~60-90s.

- [ ] **Step 2: Launch and walk the full README smoke checklist**

```bash
open target/release/monitor-rs.app
```

Step through every checkbox in `README.md`'s "Smoke test checklist" section. The menu-bar rotation, footer chips, light/dark modes, and `LSUIElement` behavior should all be unchanged. New popover-specific checks (hero, pills, pinning, reduce-motion) should all pass.

- [ ] **Step 3: Run unit tests one more time**

```bash
swift test --filter MonitorRSLogicTests
```

Expected: 10+ tests, all passing.

- [ ] **Step 4: Quit the running app**

```bash
osascript -e 'tell application "monitor-rs" to quit' 2>/dev/null || true
```

- [ ] **Step 5: Final commit (only if Step 2 surfaced any tweaks)**

If everything passed without further edits, no extra commit is needed. Otherwise, commit any fixes with a descriptive message.

---

## Self-Review Notes (already applied)

- **Spec coverage:** Every spec requirement maps to a task — `HeroSelector` logic (T4–7), color tokens with pressure override (T10 `effectiveTint`), per-core grid gating on `hero == .cpu` (T13), Swift Charts hero sparkline (T9), `MetricTile.swift` deletion (T13), README smoke checks (T15), test target for `HeroSelector` (T1, T4–7), VoiceOver-extended hysteresis (T7, T13 env wiring).
- **Placeholder scan:** Every code step includes full code; every command includes expected output; no "TBD" / "TODO" survived.
- **Type consistency:** `HeroSelector` interface (`current`, `observe(scores:voiceoverEnabled:)`, `pin(_:)`, `unpin()`, `isPinned`) is identical across the test, view-model, and view files. `MetricKind` cases and `displayLabel` are referenced consistently.
- **Settings/gear placeholder:** intentionally kept as-is (spec says out of scope); the optional a11y polish in T14 only adds a label.
