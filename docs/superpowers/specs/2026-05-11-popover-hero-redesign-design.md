# Popover Hero Redesign

**Date:** 2026-05-11
**Status:** Approved (design)
**Supersedes (UI layer only):** `2026-05-11-swiftui-popover-redesign.md`

## Goal

Replace the popover's three-tile-plus-two-tile grid with a **hero + glance** layout:
one prominent card for the metric under the most pressure, the remaining four
metrics as small tappable pills, and CPU per-core detail revealed only when CPU
is the hero. The aim is a popover that reads instantly at a glance, scales its
emphasis to current system state, and still gives access to every metric on
demand.

## Non-goals

- No changes to the Rust sampling core, the C ABI, or `MrsSample`. Every datum
  the redesign renders is already exposed.
- The menu bar status item keeps its 7-step rotation, but its pixel width is
  now locked to the widest possible rotation entry (see "Status item width"
  below) so the popover anchor doesn't drift each tick.
- No settings sheet. The gear icon in the header stays as a disabled placeholder.
- No new system permissions, no Dock icon, no window mode.

## Surface inventory (top → bottom)

1. **HeaderStrip** (existing) — wordmark + disabled gear + quit. Unchanged.
2. **HeroCard** (new) — the currently-promoted metric, large.
3. **PillsRow** (new) — the four non-hero metrics, each tappable to pin/unpin.
4. **CoreGrid** (existing) — always rendered (see "Stability" below).
5. **Top processes** (existing `ProcessList`, with section label) — unchanged.
6. **FooterStrip** (existing) — `swap · battery · temps · sample rate`.
   Unchanged in v1; minor color polish if it falls out naturally.

Width remains 300 pt. Height is constant across hero swaps (the per-core grid
is always present), so the popover's bottom edge doesn't shift as the hero
rotates.

### Stability

Two visual jumpiness sources are explicitly fixed:

- **Status item width**: `NSStatusItem.length` is locked to
  `ceil(iconSlot + monospacedDigitWidth("NET ↓99.9 ↑99.9") + padding)`
  using `NSFont.menuBarFont(ofSize: 0).pointSize` for the monospaced-digit
  font. This is computed once at init. The status item button frame never
  changes width, so the popover anchor X never moves.
- **Popover height**: the per-core grid is rendered for every hero, not just
  CPU. The cost is ~14 pt of vertical space when CPU isn't the hero; the
  benefit is a popover whose bottom edge never jumps.

### Status item width

The button uses `NSFont.monospacedDigitSystemFont(ofSize:weight:)` at menu
bar font size so digits don't shuffle horizontally as values tick within a
single category (e.g., 42% → 43%).

## Hero promotion logic

A new `HeroSelector` helper owns this. It is queried every tick by
`MonitorViewModel` after a sample lands and writes the selected `MetricKind`
back onto the model.

### Load score per metric

Each of the five metric kinds reports a load score in `[0, 1]`:

| Metric | Score formula |
|---|---|
| CPU  | `cpu_total_pct / 100` |
| GPU  | `gpu_pct / 100` (0 when sample reports `n/a`) |
| MEM  | `mem_used_bytes / mem_total_bytes` |
| NET  | `(rx + tx) / rolling_peak_net`; `rolling_peak_net = max(1 MB/s, max over last 120 samples)` |
| DSK  | `(rd + wr) / rolling_peak_disk`; same floor |

### State machine

- Default hero: **CPU**.
- Hysteresis: `candidate = argmax(score)`. A swap happens only when
  `score[candidate] - score[currentHero] >= 0.05` for **5 consecutive ticks**
  (5 s at 1 Hz default). Less-than-threshold differences are treated as no-op.
- Manual pin: tapping a pill calls `selector.pin(.kind)`. Pinned heroes ignore
  auto-promotion until unpinned. Tapping the hero (or the pinned pill again)
  calls `selector.unpin()` and auto resumes from the latest sample.
- The hero card shows a small dot next to its label when pinned.

### Why this works

- Hysteresis prevents flicker between two near-tied metrics.
- A 0.05 (5%) threshold survives the sub-second noise inherent in 1 Hz sampling.
- Manual pin gives the user authority when investigating a specific resource.

## Visual design

### Color tokens (one per metric)

| Kind | SwiftUI color |
|---|---|
| CPU | `.green` |
| GPU | `.blue` |
| MEM | `.orange` (pressure-aware — see below) |
| NET | `.teal` |
| DSK | `.purple` |

Colors are taken from the system palette so they adapt across light and dark
mode. Memory pressure overrides the MEM hue, matching the existing
`memColor` mapping in `PopoverView.swift`:

| `mem_pressure` | MEM color |
|---|---|
| 0 (normal) | `.orange` |
| 1 (warn)   | `Color(red: 0.95, green: 0.55, blue: 0.20)` (rust) |
| ≥ 2 (crit) | `.red` |

### Hero card

- Background: `LinearGradient` from `tint.opacity(0.18)` → `tint.opacity(0.05)`.
- Hairline border: `tint.opacity(0.16)`, 1 pt.
- Corner radius: 12 pt, padding 12 pt × 14 pt.
- Layout:
  - Left column: metric label (small, uppercase, tinted) → big number (32 pt,
    semibold, tabular figures) → meta line (caption, secondary).
  - Right column: Swift Charts area+line chart of the metric's history,
    fixed ~110 × 60 pt, gradient fill from `tint.opacity(0.55)` → 0.
- Pinned indicator: tiny filled circle (4 pt) next to the label, same tint.

### Hero meta line per metric

| Metric | Meta line |
|---|---|
| CPU | `N-core · hot core M%` where `M` = max of `perCoreUsage` |
| GPU | `Metal active` when sample has GPU; `n/a` otherwise (hero hidden if everything is n/a; CPU stays default) |
| MEM | `X.X / Y.Y GB · pressure {normal\|warn\|crit}` |
| NET | `↓ X.X ↑ Y.Y MB/s · peak Z.Z` (peak = rolling max) |
| DSK | `↓ X.X ↑ Y.Y MB/s · peak Z.Z` |

### Pills row

- 4 pills laid out in an `HStack` with `spacing: 6`, each flexes to equal width.
- Pill: corner radius 8, background `.white.opacity(0.04)`, border
  `.white.opacity(0.05)`, padding 6 × 8.
- Contents (vertical): small tinted label → current value (13 pt, semibold,
  tabular figures).
- Tap target: entire pill is a `Button` with `.plain` style. Pressing pins
  that metric as hero.
- A pill that is currently hero is not shown in the row (so only 4 visible).

### Per-core grid

- Rendered immediately below the pills row, **always** — for every hero, not
  just CPU. This is what keeps the popover height stable across hero swaps
  (see "Stability" above).
- Same `CoreGrid` view that ships today; no changes.

### Top processes

- Reused as-is: `Text("TOP PROCESSES")` label (caption, tracked, secondary)
  followed by `ProcessList`.
- A `Divider` precedes it; a `Divider` precedes the footer.

### Footer

- `FooterStrip` reused unchanged.

### Animations

- Hero swap: `HeroCard(...).id(model.hero).transition(...)` keys the hero
  subtree by `MetricKind` so SwiftUI applies a transition on swap. Use
  `.opacity.combined(with: .scale(scale: 0.98))` inside
  `withAnimation(.snappy(duration: 0.22))`.
- Suppress the transition when `@Environment(\.accessibilityReduceMotion)` is
  true — fall back to an instant swap.
- Sparkline updates animate implicitly via Swift Charts'
  `.animation(.linear(duration: 0.25), value: data)`.

## Data flow

- `MonitorViewModel` (existing `@Observable` class) gains:
  - `var heroSelector: HeroSelector` (private, owned by the model)
  - `var hero: MetricKind` (computed from `heroSelector.current`)
  - `func pin(_ kind: MetricKind)` / `func unpinHero()`
- After each sample is appended, the model calls
  `heroSelector.observe(sample: ..., histories: ...)` which returns the new
  hero (or unchanged). The model reassigns `hero` only when it actually
  changes — avoids spurious view invalidations.
- `PopoverView` reads `model.hero` and routes the hero card, pills, and
  the conditional core grid accordingly. Pills bind to `model.pin(_:)`.

## File layout

New files under `Sources/MonitorRSApp/`:

```
Components/
  HeroCard.swift          // the big tinted hero card
  HeroChart.swift         // Swift Charts area+line, isolated for reuse
  MetricPill.swift        // one pill in the row
  PillsRow.swift          // the 4-pill HStack, hides the current hero
HeroSelector.swift        // pure logic for auto-promote + manual pin
MetricKind.swift          // enum + color + label + value-formatting helpers
```

Removed:

```
Components/MetricTile.swift   // no longer used after the redesign lands
```

Retained, unchanged:

```
Components/CoreGrid.swift
Components/ProcessList.swift
Components/HeaderStrip.swift
Components/FooterStrip.swift
Components/Sparkline.swift    // kept for any future micro-spark needs
```

`PopoverView.swift` is simplified into a pure composer: header → hero →
pills → optional core grid → procs → footer. The current `summaryGrid` /
`ioGrid` / `normalize*` helpers move to `MetricKind` (formatting) and
`HeroSelector` (load math), so `PopoverView` becomes ~30 lines.

## Modernization (swiftui-pro pass)

- `foregroundStyle(_:)` everywhere; remove any `foregroundColor(_:)`.
- Buttons use `Button(action:)` with text labels for accessibility; icon
  buttons add `.accessibilityLabel("…")`.
- VoiceOver:
  - Hero card: combined label "CPU, 42 percent, trending up".
  - Pill: "GPU, 8 percent, button. Activate to pin as hero."
- Dynamic Type capped at `.xLarge` via `.dynamicTypeSize(...DynamicTypeSize.xLarge)`
  on `PopoverView`. Justification: 300 pt is a hard popover constraint.
- `@Observable` on `MonitorViewModel` (already there); views consume via
  `@Bindable`.
- Swift Charts pulled in via SwiftPM for the hero sparkline only (one
  `Chart` site, kept inside `HeroChart.swift`). The existing custom
  `Sparkline` is retained for any future micro-spark needs but is unused
  in v1.
- Concurrency: no new actors needed; sampling continues to land on the
  main actor inside `MonitorViewModel` as today.

## Error / edge cases

| Case | Behavior |
|---|---|
| `gpu_present == 0` (GPU sample n/a) | GPU's load score is 0 → never auto-promoted. If user pins it manually, hero shows `n/a` in the big number and `Metal idle` in the meta line. |
| No samples yet | Same "Sampling…" placeholder as today (centered, no hero card, no pills). |
| `mem_total_bytes == 0` (sampler error) | MEM load = 0; hero shows `—`. |
| All metrics flat at 0 | Default hero (CPU) stays selected; no swap. |
| Reduce Motion on | Transition replaced by an instant cut; Charts animation duration drops to 0. |
| VoiceOver running | Auto-promotion is debounced to **15 ticks** of stability to avoid mid-utterance hero swaps. Detected via SwiftUI's `@Environment(\.accessibilityVoiceOverEnabled)` and passed into `HeroSelector` from the view layer. |
| Sample rate ≠ 1 Hz | Hysteresis window stays at **5 ticks**, not 5 seconds (i.e., scales with sample rate). Documented behavior. |

## Testing

The redesign is mostly view code, but `HeroSelector` is pure logic.

A SwiftPM test target was originally planned, but the build environment
on this machine (Command Line Tools only — no Xcode.app) makes it
impractical: XCTest is bundled exclusively with Xcode.app, and while
Swift Testing's `Testing.framework` ships with CLT it isn't on the
default search path, and `swift test` insists on linking the executable
target (whose Rust static lib needs OpenDirectory wired by the build
script). Adding a test target meant brittle `-F` flags plus per-CI
plumbing for marginal value on ~30 lines of logic. We dropped the
target and verify `HeroSelector` via:

- **Careful code review** of `HeroSelector.swift` — the logic is small,
  clearly structured (pin check → threshold/argmax → hysteresis counter),
  and behavior is documented in the doc comments and the spec sections
  above (Hero promotion logic, Error / edge cases).
- **Manual smoke test** (the README checklist), which exercises every
  behavior end-to-end against the real sampler:
  - With nothing running, hero is CPU.
  - Running `yes > /dev/null` × N keeps CPU as hero (it's already #1).
  - Downloading 50 MB via curl swaps hero to NET within ~5 s.
  - `dd` write burst swaps hero to DSK; ends → returns to CPU.
  - Tapping a pill pins; tapping the pinned hero unpins.
  - With CPU as hero, the per-core grid is visible. With anything else as
    hero, the grid is hidden and popover height shrinks.
  - Reduce Motion: hero swap has no animation.

There are no FFI changes, so the Rust side requires no new tests.

## Risk register

| Risk | Mitigation |
|---|---|
| Variable popover height feels janky | NSPopover animates frame changes automatically; the height delta (~40 pt) is small. Acceptable. |
| Swift Charts pulls in a sizable dependency | It's a system framework on macOS 13+, not a third party. Zero extra binary cost. |
| Hero "n/a" for GPU looks broken | Hero auto-logic excludes GPU when n/a; pinning is the only way to land there, and the message ("Metal idle") explains it. |
| Pinning interaction discoverable? | Pills have hover state (cursor pointer); README + accessibility hint explain. Not optimal but acceptable for v1. A future tooltip on first run could help. |

## Open questions resolved during brainstorming

- Hero promotion: auto-by-load with manual pin (chosen).
- Per-core grid placement: shown only when CPU is hero (chosen).
- Footer treatment: keep as-is (chosen — out of scope).
- Charts dependency: pull in Swift Charts for the hero only (chosen).
- Menu bar redesign: out of scope (chosen).

## Implementation order (preview, full plan to follow)

1. Add `MetricKind` enum with color + label + value/meta formatting.
2. Add `HeroSelector` with unit tests.
3. Wire `HeroSelector` into `MonitorViewModel` (`hero`, `pin`, `unpinHero`).
4. Build `HeroChart` (Swift Charts area+line).
5. Build `HeroCard` consuming `MetricKind` + sample + history.
6. Build `MetricPill` + `PillsRow`.
7. Rewrite `PopoverView` to compose the new pieces; gate `CoreGrid` on
   `hero == .cpu`.
8. Apply swiftui-pro pass (foregroundStyle, accessibility labels,
   dynamic type cap, reduce-motion check).
9. Delete `MetricTile.swift` once nothing references it.
10. Update README smoke checklist; verify all checks pass.
