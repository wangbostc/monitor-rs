# Additional Monitoring Metrics — Design

**Status:** Draft
**Date:** 2026-05-11
**Owner:** bo wang
**Target chip:** Apple M4 (M1–M3 best-effort)

## Problem

`monitor-rs` currently exposes CPU, GPU, memory, and top processes. The menu
bar rotates `CPU% → GPU% → MEM%` every two seconds, and the popover shows
three tiles plus a per-core grid and process list. Users want richer
at-a-glance system health: network throughput, disk throughput, battery, and
real die temperatures. This spec adds those four metric families end-to-end
(Rust sampler → FFI → Swift menu bar + popover) in a single increment.

## Goals

- Surface four new metric families: **Network I/O**, **Disk I/O**, **Battery**,
  **Temperature** (CPU + GPU dies in °C).
- Integrate them into both the menu-bar rotation and the popover.
- Use public Apple APIs for the easy three; isolate the private-API risk for
  temperature so it can be swapped for thermal-pressure fallback without
  reshaping the rest of the work.
- Keep the menu bar item narrow enough to clear the camera notch on a 14"
  MacBook Pro (≤ ~110 points wide at any rotation step).

## Non-goals

- Per-interface network breakdown (one combined rate is enough at this stage).
- Per-volume disk breakdown.
- Historical persistence of the new metrics beyond the existing
  `history_seconds` ring buffer.
- Configurable rotation order or per-metric show/hide toggles (deferred — the
  cycle is a fixed `mod 7` for now).
- Per-process I/O attribution.
- Power draw in watts (a separate future metric — IOReport-based).

## Approach

The work touches three layers; each is kept narrow and self-contained.

### 1. Rust sampler

Four new modules under `src/metrics/`:

- **`net.rs`** — uses the existing `sysinfo::Networks` to read per-interface
  rx/tx byte counters. The sampler keeps the previous tick's totals and
  computes byte-per-second rates summed across non-loopback interfaces.
- **`disk.rs`** — uses `sysinfo::Disks` for read/written byte counters; same
  rate-delta pattern as `net.rs`.
- **`battery.rs`** — binds `IOPSCopyPowerSourcesInfo` /
  `IOPSGetProvidingPowerSourceType` via the `core-foundation` crate already
  on the dep list. Returns `{ percent: f32, is_charging: bool, present: bool }`.
  `present = false` cleanly covers desktop Macs.
- **`thermal.rs`** — binds the private `IOHIDEventSystemClient` framework via
  `libloading`, mirroring the loading pattern already established in
  `gpu.rs`. Enumerates thermal sensors, filters by chip-specific name table
  (M4 keys are canonical; M1–M3 entries best-effort), and returns
  `{ cpu_c: Option<f32>, gpu_c: Option<f32> }`. On total enumeration failure
  the module logs once at WARN and returns `None` for both — see Risks below.

Snapshot state for rate-based metrics (previous `Networks` / `Disks` totals
and the timestamp at which they were read) lives in the existing
`SamplerHandle` so the rate computation is part of the regular tick.

`Sample` (in `src/sample.rs`) gains four new fields:

```rust
pub struct NetIo {  pub rx_bps: u64, pub tx_bps: u64 }
pub struct DiskIo { pub read_bps: u64, pub write_bps: u64 }
pub struct BatteryInfo {
    pub percent: f32,
    pub is_charging: bool,
    pub present: bool,
}
pub struct ThermalInfo {
    pub cpu_c: Option<f32>,
    pub gpu_c: Option<f32>,
}

pub struct Sample {
    /* …existing fields… */
    pub net: NetIo,
    pub disk: DiskIo,
    pub battery: BatteryInfo,
    pub thermal: ThermalInfo,
}
```

Unit tests cover: zero-elapsed-time guard in rate math, monotonic-counter
wrap (drop the delta on regress), a captured-dictionary fixture for the
battery parser, and a captured sensor-enumeration fixture for the thermal
parser.

### 2. FFI

`MrsSample` (the C struct in `src/ffi.rs`) is extended with a flat set of
fields so the Swift side does not need to unpack nested structs:

```c
uint64_t net_rx_bps;
uint64_t net_tx_bps;
uint64_t disk_read_bps;
uint64_t disk_write_bps;

uint8_t  battery_present;     // 0/1
uint8_t  battery_charging;    // 0/1
float    battery_pct;

uint8_t  cpu_temp_present;    // 0/1
float    cpu_temp_c;
uint8_t  gpu_temp_present;    // 0/1
float    gpu_temp_c;
```

`cbindgen` regenerates `include/monitor_rs.h`; the generated header is
committed as today.

### 3. Swift menu bar

`MenuBarController.formatStatus(_:index:)` is generalised to seven entries
cycling with the existing 2 s period (so the full loop is 14 s). Format
strings are tuned for narrowness so the item stays clear of the notch:

| index | text                | example         |
|-------|---------------------|-----------------|
| 0     | `CPU N%`            | `CPU 22%`       |
| 1     | `GPU N%`            | `GPU 7%`        |
| 2     | `MEM N%`            | `MEM 64%`       |
| 3     | `NET ↓X.X ↑Y.Y`     | `NET ↓3.2 ↑1.1` |
| 4     | `DSK ↓X.X ↑Y.Y`     | `DSK ↓5.4 ↑2.1` |
| 5     | `BAT N%⚡` / `BAT N%`| `BAT 85%⚡`     |
| 6     | `TMP N°C`           | `TMP 65°C`      |

Rate values are formatted with one decimal in MB/s, clamping to `0.0` when
below 0.05 MB/s to avoid jitter. Missing-sensor states render as `BAT —` and
`TMP —` respectively. The gauge SF Symbol icon stays in place.

A small pure helper, `formatMetric(sample:, index:) -> String`, keeps the
formatting logic testable and free of `NSStatusItem` coupling. Existing
rotation-index plumbing is unchanged.

### 4. Swift popover

Layout grows by one row of tiles plus footer enrichments. No new
top-level views are introduced.

```
┌─ HeaderStrip ───────────────────────────────────┐
│                                                 │
├─ Row 1: [CPU tile] [GPU tile] [MEM tile] ───────┤
│ (unchanged; sparklines + numbers)               │
├─ Row 2: [Network tile] [Disk tile] ─────────────┤
│ Each tile: title, current down/up MB/s,         │
│ one Sparkline of total throughput over the      │
│ existing history window.                        │
├─ CoreGrid (unchanged) ──────────────────────────┤
├─ ProcessList (unchanged) ───────────────────────┤
├─ FooterStrip ───────────────────────────────────┤
│ left: existing controls                         │
│ right: `🔋 85% ⚡   🌡 CPU 65° GPU 42°`           │
└─────────────────────────────────────────────────┘
```

`MetricTile` is reused for the new row (it already accepts a title, current
value, and sparkline source). `FooterStrip` gains two trailing labels.
`ViewModel` exposes the new fields and recent-history arrays for net/disk
that the Sparkline component consumes.

## Data flow

```
sysinfo / IOPS / IOHID  ─►  per-metric collectors (net/disk/battery/thermal)
                          ─►  Sampler tick (rate deltas, snapshot state)
                          ─►  Sample (Rust)
                          ─►  monitor_rs_latest / monitor_rs_recent  (FFI)
                          ─►  MrsSample (Swift)
                          ─►  ViewModel  ─►  MenuBarController / PopoverView
```

The flow is the same as for the existing CPU/GPU/MEM metrics; the only new
layer concept is the previous-tick snapshot state for rate-based metrics,
which lives inside the sampler and is not exposed across the FFI.

## Error handling

- **Missing sensor (battery on desktops, thermal on enumeration failure):**
  the corresponding `present` flag in `MrsSample` is `0`; Swift renders `—`
  in the menu bar slot and hides the footer chip.
- **Counter regress (rare wrap or interface reset):** the rate is clamped to
  `0` for that tick rather than reporting a negative or absurd value.
- **HID enumeration failure:** logged once at WARN with the chip identifier
  and the list of sensor names that were probed, so we can extend the table
  later without recompiling. Subsequent ticks proceed silently with
  `thermal = ThermalInfo { cpu_c: None, gpu_c: None }`.

## Risks and mitigations

1. **`IOHIDEventSystemClient` is a private framework.** Sensor names differ
   per chip generation. Mitigation: ship M4 keys as the authoritative path
   (the user's machine); list M1/M2/M3 as best-effort with their known keys
   from public reverse-engineering work; degrade to `None` rather than
   crashing if enumeration fails. If during implementation this turns out
   unreliable on M4 specifically, the user has pre-approved falling back to
   `ProcessInfo.thermalState` (nominal/fair/serious/critical) for the
   temperature slot — that fallback only changes the contents of one menu-bar
   entry and one footer chip, so the surrounding work is unaffected.

2. **Rotation feels slow at 14 s.** Mitigated by keeping each entry narrow
   and by the FooterStrip showing battery+temp continuously inside the
   popover. If users find the cycle too long after using it, a follow-up
   spec can introduce a configurable subset; it is explicitly out of scope
   here.

3. **Menu bar item still hidden by the notch.** Each rotation entry is
   bounded at ~110 points (CPU/MEM/BAT/TMP are well under; NET/DSK with two
   decimals are the widest). If the user's menu bar is so crowded that even
   `CPU 22%` is hidden, this spec does not fix that — they need to remove
   other items or use a menu-bar manager. Documented in the README.

4. **`sysinfo` may not refresh `Networks`/`Disks` on every call without an
   explicit `refresh()`.** Verify in implementation; if so, call the
   appropriate refresh inside the sampler tick.

## Testing

- **Rust unit tests** (`#[cfg(test)]` blocks alongside each new module):
  - `net::rate_delta_basic`, `net::rate_delta_handles_zero_dt`,
    `net::rate_delta_drops_negative` (counter regress)
  - `disk::*` mirroring the above
  - `battery::parse_present_charging`, `battery::parse_absent`
  - `thermal::filter_by_chip_table` using a fixture sensor list
- **Rust integration test** (existing `tests/` directory): one round-trip
  test that calls `monitor_rs_start`, waits two tick periods, and asserts
  the new FFI fields are populated within sensible ranges. Sensor-dependent
  fields are checked for *shape* (`battery_present == 0 || 0 ≤ pct ≤ 100`)
  rather than exact values.
- **Manual smoke test (extends the README checklist):**
  - Status item cycles through all 7 entries within 14 s.
  - Network rate rises when running `curl -o /dev/null https://speed.cloudflare.com/__down?bytes=10000000`.
  - Disk rate rises when running `dd if=/dev/zero of=/tmp/x bs=1m count=200`.
  - Battery: unplug → `BAT N%`; plug in → `BAT N%⚡`.
  - Temp: CPU °C rises under `yes > /dev/null & yes > /dev/null & yes > /dev/null`.

## Out-of-scope follow-ups

- Configurable rotation (which of the 7 entries to include, custom order).
- Power draw in watts via IOReport energy counters.
- Per-process network and disk I/O.
- Live thermal-sensor table refresh from a JSON resource so we can ship new
  chip support without rebuilding.

## Open questions

None at write-time; both deferred-decision points (HID fallback strategy,
fixed rotation order) were resolved during brainstorming.
