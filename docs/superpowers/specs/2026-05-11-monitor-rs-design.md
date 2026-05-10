# monitor-rs — Design

A small macOS menu bar app that shows live CPU, GPU, and memory usage plus the top resource-consuming processes, written in Rust with egui.

## Goals

- Single-binary native macOS app, lives in the menu bar.
- Live CPU (total + per-core), GPU, memory (used / pressure / swap), and top-N processes, sampled at 1 Hz.
- Click the menu-bar icon to open a small popover with sparklines and a per-core grid.
- Low overhead: sampler thread does work; UI redraws read-only from a ring buffer.
- Apple Silicon only for v1.

## Non-goals (v1)

- Network or disk graphs.
- Power, thermal, or fan metrics.
- Notifications / threshold alerts.
- Codesigning and notarization (ship as unsigned `.app`; users right-click → Open the first time).
- Intel Mac support — IOReport channels and PMU naming differ; design keeps the GPU module isolated so this can be added later.

## Architecture

```
┌─────────────────────────────────────────┐
│  main thread (eframe app)               │
│    - NSStatusItem (tray)                │
│    - egui popover window (toggled)      │
│    - reads SampleStore (RwLock)         │
└──────────────▲──────────────────────────┘
               │
        (RwLock<SampleStore>)
               │
┌──────────────┴──────────────────────────┐
│  sampler thread (std::thread, 1 Hz)     │
│    cpu · mem · gpu · procs              │
│    push Sample into ring buffer (N=120) │
└─────────────────────────────────────────┘
```

One dedicated sampler thread, sleeps `1 / sample_rate_hz` seconds between cycles. UI reads the latest N samples under a read lock — no allocation in the hot path. Ring buffer capacity is computed as `ceil(history_seconds * sample_rate_hz)` (default 120 samples = 2 min at 1 Hz) so history length is rate-independent. Sparklines render the last `min(60, history_seconds)` seconds of samples regardless of rate.

## Components

| File | Responsibility |
| --- | --- |
| `src/main.rs` | eframe entry; spawns sampler; wires tray + popover |
| `src/sample.rs` | `Sample` struct, `SampleStore` ring buffer |
| `src/sampler.rs` | thread loop, fans out to each metric module, builds `Sample` |
| `src/metrics/cpu.rs` | total + per-core CPU % via `sysinfo` |
| `src/metrics/mem.rs` | used / total via `sysinfo`; memory pressure via `mach` `host_statistics64` (`vm_statistics64`); swap via `xsw_usage` sysctl |
| `src/metrics/gpu.rs` | Apple Silicon GPU % via `IOReport` private framework FFI |
| `src/metrics/procs.rs` | top-N processes by CPU and by RSS via `sysinfo` |
| `src/ui/tray.rs` | NSStatusItem creation + click handler via `objc2-app-kit` |
| `src/ui/popover.rs` | egui panel: sparklines, per-core grid, process list |
| `src/ui/sparkline.rs` | minimal sparkline widget on top of `egui` painter |
| `src/settings.rs` | sample rate, menu-bar text format; persisted to `~/Library/Application Support/monitor-rs/config.json` |

Each metric module owns its own `Sampler` struct (e.g. `CpuSampler`, `GpuSampler`) holding any cross-cycle state it needs (previous CPU times, IOReport subscription handle, etc.) and exposes a single `tick(&mut self) -> Result<Reading, MetricError>` method. The top-level `sampler.rs` composes them and assembles a `Sample`. This keeps each metric independently testable.

## Data model

```rust
pub struct Sample {
    pub ts: Instant,
    pub cpu_total: f32,            // 0.0 - 100.0
    pub cpu_per_core: Vec<f32>,    // length = core count, 0.0 - 100.0
    pub gpu_pct: Option<f32>,      // None if IOReport unavailable
    pub mem: MemInfo,
    pub swap: SwapInfo,
    pub top_procs: Vec<ProcInfo>,  // length <= TOP_N (default 5)
}

pub struct MemInfo {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub pressure: MemPressure,     // Normal | Warning | Critical
}

pub struct SwapInfo { pub used_bytes: u64, pub total_bytes: u64 }

pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub rss_bytes: u64,
}

pub struct SampleStore {
    buf: VecDeque<Sample>, // capacity = 120
}
```

`SampleStore::push` evicts the oldest sample when at capacity. `SampleStore::latest()` returns `Option<&Sample>`. `SampleStore::recent(n)` returns an iterator over the last `n` samples for sparklines.

## Concurrency

- Single `Arc<RwLock<SampleStore>>` shared between sampler and UI.
- Sampler holds the write lock only long enough to push one sample (`< 1 ms`).
- UI reads under `read()`; reads are non-blocking in practice.
- No async runtime — `std::thread` + `Instant`-based sleep with drift correction (sleep until `next_tick`, not `+ 1s`).

## GPU sampling (Apple Silicon)

No public macOS API exposes integrated-GPU utilization. The proven approach used by `macmon`, `asitop`, and `mactop` is the private `IOReport` framework. The crate `metrics/gpu.rs` will:

1. On startup, call `IOReportCopyChannelsInGroup("GPU Stats", ...)` and `IOReportCreateSubscription(...)` to subscribe to the GPU performance-state channel; cache the subscription.
2. Each tick, `IOReportCreateSamples(...)` returns a `CFDictionary`; iterate `IOReportChannelGetGroup` / `IOReportSimpleGetIntegerValue` to read residency in the active P-state.
3. Convert P-state residency over the elapsed interval into a 0–100 utilization figure.

All FFI lives behind a small safe wrapper that returns `Option<f32>`. If any step fails (future macOS removes channels, missing entitlement, etc.), `gpu_pct` becomes `None` and the UI shows `GPU: n/a` without affecting other metrics.

References (reading only): `macmon` (`github.com/vladkens/macmon`), `asitop`, `mactop`.

## Memory pressure

- `host_statistics64(host, HOST_VM_INFO64, …)` returns `vm_statistics64` with `compressor_page_count`, `internal_page_count`, etc.
- Apple's pressure formula (matching Activity Monitor) approximated as
  `(used_wired + used_compressed) / total_memory`, classified Normal / Warning / Critical at the published thresholds.
- Swap via `sysctl` `vm.swapusage` (`xsw_usage` struct).

## Tray + popover

`tray-icon` only supports NSMenu, not a rich popover with charts. We use `objc2-app-kit::NSStatusBar::system_status_bar` directly:

1. Create an `NSStatusItem` with variable length; set its button title from the latest sample (`C 42 G 18 M 64`, format configurable in settings).
2. On button click, toggle a borderless eframe window (`decorations: false`, `transparent: true`, `always_on_top: true`) positioned under the status item using the button's `window().frame()`.
3. Click outside or press Esc hides the window. The window is created once and shown/hidden — not destroyed.

The objc2 surface area is roughly 50–80 lines, all in `src/ui/tray.rs`.

## UI layout (popover)

```
┌─────────────────────────────────────┐
│  CPU  42%   ▁▂▄▆█▆▅▃▂▁▂▃            │
│  ╔═╦═╦═╦═╦═╦═╦═╦═╗ (per-core grid)  │
│  GPU  18%   ▁▁▂▃▂▁▁▂▃▄▃▂            │
│  MEM  64%   ▂▃▃▄▄▄▅▅▅▅▆▆  swap 0.2G │
│  ─────────────────────────────────  │
│  Top processes                      │
│   chrome   28%  1.4G                │
│   Xcode    14%  3.2G                │
│   …                                 │
│  ─────────────────────────────────  │
│  ⚙ Settings    Quit                 │
└─────────────────────────────────────┘
```

Width ~280 px, height auto. Sparklines render the last 60 seconds of samples (count derived from sample rate). The per-core grid is one tiny block per core, color-mapped by current usage.

## Settings

Stored as JSON at `~/Library/Application Support/monitor-rs/config.json`:

```json
{
  "sample_rate_hz": 1.0,
  "menu_bar_format": "C {cpu} G {gpu} M {mem}",
  "top_n_procs": 5,
  "history_seconds": 120
}
```

Loaded on launch, written on change. Missing/corrupt file → defaults.

## Error handling

- Each metric `sample()` returns `Result<Reading, MetricError>`.
- The sampler logs the error once per metric (rate-limited) and stores `None` / last-known value as appropriate.
- UI shows `n/a` for any unavailable metric. The app never panics on metric failure.
- Logger: `tracing` with a rolling file in `~/Library/Logs/monitor-rs/monitor-rs.log` plus stderr in debug builds.

## Testing

**Unit (cross-platform):**
- `SampleStore` ring buffer: push past capacity, latest, recent(n).
- Percent formatting and menu-bar template substitution.
- Settings serde round-trip; defaults on missing file; recovery from corrupt file.

**macOS-only integration (`#[cfg(target_os = "macos")]`):**
- One cycle of `cpu::sample` returns 0 ≤ total ≤ 100, len(per_core) == num_cores.
- `mem::sample` returns `used_bytes > 0` and `used_bytes ≤ total_bytes`.
- `gpu::sample` returns `Some(pct)` on Apple Silicon when IOReport channels are present (test skipped if `None` after warmup).
- `procs::sample` returns at least one process and respects `top_n`.

**Manual:**
- Launch, click tray, observe live updates.
- CPU stress: `yes > /dev/null` × N — verify per-core grid lights up.
- GPU stress: a small Metal compute load — verify GPU sparkline rises.
- Hold the tray button down for ≥ 5 minutes to confirm no leak / drift in the sampler cadence.

## Dependencies (key crates)

- `eframe` / `egui` — UI
- `egui_plot` — used optionally; sparkline is hand-rolled to keep the binary small
- `sysinfo` — CPU, memory used/total, processes
- `objc2`, `objc2-app-kit`, `objc2-foundation` — NSStatusItem
- `core-foundation`, `core-foundation-sys` — IOReport CFType bindings
- `mach2` — `host_statistics64`
- `serde`, `serde_json` — settings
- `directories` — config / log paths
- `tracing`, `tracing-subscriber`, `tracing-appender` — logging

## Build & distribution (v1)

- `cargo build --release` produces a single binary.
- `cargo bundle` (or hand-rolled `Info.plist`) wraps it as `monitor-rs.app` with `LSUIElement = true` so it has no Dock icon.
- Unsigned. README documents the right-click → Open first-launch step.

## Open risks

- **IOReport stability across macOS versions.** Mitigated by isolating GPU FFI behind `Option<f32>` and feature-detecting channels at startup.
- **objc2 + eframe interaction.** eframe owns its NSApplication; we add the NSStatusItem after eframe initializes, before the event loop runs. If conflicts arise, fall back to `tray-icon` + a textual NSMenu and accept the loss of in-popover charts.
- **Sampler drift if the system sleeps.** Use `Instant` deadlines and reset the next deadline if `now > deadline + 2s` (system was asleep) so we don't fire a burst of catch-up ticks.
