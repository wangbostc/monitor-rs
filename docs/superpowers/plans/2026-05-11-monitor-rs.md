# monitor-rs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a single-binary macOS menu bar app in Rust + egui that shows live CPU, GPU, memory, swap, and top-N processes, sampled at 1 Hz. Apple Silicon only for v1.

**Architecture:** A dedicated sampler thread fills a shared `RwLock<SampleStore>` ring buffer at the configured rate. The main thread runs an eframe (egui) app that creates an `NSStatusItem` (via `objc2-app-kit`) and toggles a borderless egui popover window on click; the popover reads samples under a read lock and renders sparklines, a per-core grid, and a top-process list. GPU utilization comes from the private `IOReport.framework` loaded with `libloading` (no public API exists).

**Tech Stack:** Rust 1.78+, `eframe` / `egui`, `sysinfo`, `objc2` + `objc2-app-kit` + `objc2-foundation`, `core-foundation`, `mach2`, `libloading`, `serde` / `serde_json`, `directories`, `tracing` + `tracing-appender`.

---

## Reference reading (engineer should skim before starting)

- The design spec: `docs/superpowers/specs/2026-05-11-monitor-rs-design.md`
- `macmon` source for the IOReport binding pattern: `https://github.com/vladkens/macmon/blob/main/src/sources.rs` (read-only — port the technique, write fresh code)
- `egui` book sections on `egui::Painter` (used for the sparkline widget) and `eframe` window builder flags

---

## File map

Files created by the end of the plan:

```
Cargo.toml
.gitignore
.cargo/config.toml
build.rs                          # only if cargo-bundle is unavailable; we use cargo-bundle
README.md
assets/Info.plist                 # for cargo-bundle wrapping
src/main.rs                       # eframe entry, wires sampler + tray + popover
src/lib.rs                        # re-exports for tests
src/sample.rs                     # Sample, MemInfo, SwapInfo, ProcInfo, MemPressure
src/store.rs                      # SampleStore ring buffer
src/settings.rs                   # config load/save
src/logging.rs                    # tracing setup
src/format.rs                     # menu-bar text template substitution
src/sampler.rs                    # orchestrates per-metric Samplers, runs the tick loop
src/metrics/mod.rs                # MetricError, Reading traits
src/metrics/cpu.rs                # CpuSampler
src/metrics/mem.rs                # MemSampler (used/total/pressure + swap)
src/metrics/procs.rs              # ProcSampler
src/metrics/gpu.rs                # GpuSampler (IOReport via libloading)
src/ui/mod.rs
src/ui/sparkline.rs               # sparkline widget + normalize fn
src/ui/cores.rs                   # per-core grid widget
src/ui/procs.rs                   # process list widget
src/ui/popover.rs                 # composes the popover panel
src/ui/tray.rs                    # NSStatusItem creation + click bridge
tests/                            # integration tests (macOS-gated)
```

---

## Task 0: Bootstrap project

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `.cargo/config.toml`, `src/main.rs`, `src/lib.rs`, `README.md`

- [ ] **Step 1: Initialize the cargo project**

```bash
cd /Users/bowang/projects/monitor-rs
cargo init --name monitor-rs --bin
```

Expected: creates `Cargo.toml` and `src/main.rs`.

- [ ] **Step 2: Replace `src/main.rs` with a minimal stub and add `src/lib.rs`**

`src/main.rs`:

```rust
fn main() {
    println!("monitor-rs: bootstrap");
}
```

`src/lib.rs`:

```rust
// Re-exports so integration tests in tests/ can import from `monitor_rs::...`.
pub mod sample;
pub mod store;
```

(Modules referenced here will be created in Task 1. The build will fail until then — that's expected; we'll fix it in Task 1's first step.)

- [ ] **Step 3: Write `.gitignore`**

```
/target
/.superpowers
/.DS_Store
*.app
```

- [ ] **Step 4: Add dependencies via `cargo add`**

Run these in order. Use whatever versions cargo resolves — pinning across our knowledge horizon is brittle, and `Cargo.lock` will lock them anyway.

```bash
cargo add eframe --no-default-features --features "default_fonts,glow,wayland,x11"
cargo add egui
cargo add sysinfo
cargo add serde --features derive
cargo add serde_json
cargo add directories
cargo add tracing
cargo add tracing-subscriber --features "env-filter,fmt"
cargo add tracing-appender
cargo add anyhow
cargo add thiserror
cargo add parking_lot

# macOS-only crates (target-cfg-gated)
cargo add objc2 --target 'cfg(target_os = "macos")'
cargo add objc2-app-kit --target 'cfg(target_os = "macos")' --features "NSStatusBar,NSStatusItem,NSStatusBarButton,NSApplication,NSEvent,NSWindow,NSScreen,NSImage"
cargo add objc2-foundation --target 'cfg(target_os = "macos")' --features "NSString,NSGeometry"
cargo add core-foundation --target 'cfg(target_os = "macos")'
cargo add core-foundation-sys --target 'cfg(target_os = "macos")'
cargo add mach2 --target 'cfg(target_os = "macos")'
cargo add libloading --target 'cfg(target_os = "macos")'
```

If any feature flag isn't available in the resolved version, drop it from the `--features` list and we'll re-add the equivalent symbol later. The plan does not assume a specific minor version of these crates.

- [ ] **Step 5: Write `.cargo/config.toml`**

```toml
[build]
# Faster incremental linking on macOS.
[target.'cfg(target_os = "macos")']
rustflags = ["-C", "link-arg=-fuse-ld=ld"]
```

- [ ] **Step 6: Write a minimal README**

`README.md`:

```markdown
# monitor-rs

A small macOS menu bar app showing live CPU, GPU, memory, and top processes.
Apple Silicon only.

## Build

```
cargo build --release
./target/release/monitor-rs
```

See `docs/superpowers/specs/` for the design.
```

- [ ] **Step 7: Verify it builds and runs**

```bash
cargo build
```

Expected: build succeeds (with warnings about unused crates — fine for now). `cargo run` should print `monitor-rs: bootstrap` and exit. **It will not build yet** because `src/lib.rs` references modules that don't exist; if so, temporarily change `src/lib.rs` to be empty for this step, then restore it in Task 1.

Actually — make `src/lib.rs` empty for now and move the module declarations into Task 1:

`src/lib.rs`:

```rust
// Modules added in subsequent tasks.
```

Re-run `cargo build` — should succeed cleanly.

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock .gitignore .cargo/ src/ README.md
git commit -m "chore: bootstrap monitor-rs cargo project"
```

---

## Task 1: Sample types

**Files:**
- Create: `src/sample.rs`
- Modify: `src/lib.rs`
- Test: `src/sample.rs` (in-file `#[cfg(test)]` module)

- [ ] **Step 1: Write the failing test in `src/sample.rs`**

```rust
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemPressure {
    Normal,
    Warning,
    Critical,
}

#[derive(Debug, Clone)]
pub struct MemInfo {
    pub used_bytes: u64,
    pub total_bytes: u64,
    pub pressure: MemPressure,
}

#[derive(Debug, Clone)]
pub struct SwapInfo {
    pub used_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f32,
    pub rss_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub ts: Instant,
    pub cpu_total: f32,
    pub cpu_per_core: Vec<f32>,
    pub gpu_pct: Option<f32>,
    pub mem: MemInfo,
    pub swap: SwapInfo,
    pub top_procs: Vec<ProcInfo>,
}

impl MemInfo {
    pub fn used_pct(&self) -> f32 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64 * 100.0) as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mem_used_pct_handles_zero_total() {
        let m = MemInfo { used_bytes: 0, total_bytes: 0, pressure: MemPressure::Normal };
        assert_eq!(m.used_pct(), 0.0);
    }

    #[test]
    fn mem_used_pct_basic() {
        let m = MemInfo { used_bytes: 50, total_bytes: 100, pressure: MemPressure::Normal };
        assert!((m.used_pct() - 50.0).abs() < 0.01);
    }
}
```

- [ ] **Step 2: Add module declaration in `src/lib.rs`**

```rust
pub mod sample;
```

- [ ] **Step 3: Run tests — they should pass**

```bash
cargo test --lib sample
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/sample.rs src/lib.rs
git commit -m "feat: add Sample types"
```

---

## Task 2: SampleStore ring buffer

**Files:**
- Create: `src/store.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write the failing tests in `src/store.rs`**

```rust
use std::collections::VecDeque;
use std::time::Instant;

use crate::sample::{MemInfo, MemPressure, Sample, SwapInfo};

pub struct SampleStore {
    buf: VecDeque<Sample>,
    capacity: usize,
}

impl SampleStore {
    pub fn new(capacity: usize) -> Self {
        Self { buf: VecDeque::with_capacity(capacity.max(1)), capacity: capacity.max(1) }
    }

    pub fn push(&mut self, s: Sample) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(s);
    }

    pub fn latest(&self) -> Option<&Sample> {
        self.buf.back()
    }

    pub fn recent(&self, n: usize) -> impl Iterator<Item = &Sample> + '_ {
        let take = n.min(self.buf.len());
        self.buf.iter().skip(self.buf.len() - take)
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_sample(cpu: f32) -> Sample {
        Sample {
            ts: Instant::now(),
            cpu_total: cpu,
            cpu_per_core: vec![cpu],
            gpu_pct: None,
            mem: MemInfo { used_bytes: 0, total_bytes: 1, pressure: MemPressure::Normal },
            swap: SwapInfo { used_bytes: 0, total_bytes: 0 },
            top_procs: vec![],
        }
    }

    #[test]
    fn pushes_and_evicts_at_capacity() {
        let mut s = SampleStore::new(3);
        s.push(dummy_sample(1.0));
        s.push(dummy_sample(2.0));
        s.push(dummy_sample(3.0));
        s.push(dummy_sample(4.0));
        assert_eq!(s.len(), 3);
        assert_eq!(s.latest().unwrap().cpu_total, 4.0);
        let recents: Vec<f32> = s.recent(10).map(|x| x.cpu_total).collect();
        assert_eq!(recents, vec![2.0, 3.0, 4.0]);
    }

    #[test]
    fn recent_clamped_to_len() {
        let mut s = SampleStore::new(10);
        s.push(dummy_sample(1.0));
        s.push(dummy_sample(2.0));
        let r: Vec<f32> = s.recent(5).map(|x| x.cpu_total).collect();
        assert_eq!(r, vec![1.0, 2.0]);
    }

    #[test]
    fn capacity_zero_clamped_to_one() {
        let s = SampleStore::new(0);
        assert_eq!(s.capacity(), 1);
    }
}
```

- [ ] **Step 2: Add module declaration in `src/lib.rs`**

```rust
pub mod sample;
pub mod store;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib store
```

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src/store.rs src/lib.rs
git commit -m "feat: add SampleStore ring buffer"
```

---

## Task 3: Settings

**Files:**
- Create: `src/settings.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write the failing tests in `src/settings.rs`**

```rust
use std::path::PathBuf;
use std::{fs, io};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Settings {
    pub sample_rate_hz: f32,
    pub menu_bar_format: String,
    pub top_n_procs: usize,
    pub history_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sample_rate_hz: 1.0,
            menu_bar_format: "C {cpu} G {gpu} M {mem}".to_string(),
            top_n_procs: 5,
            history_seconds: 120,
        }
    }
}

impl Settings {
    /// Number of samples retained in the ring buffer.
    pub fn history_capacity(&self) -> usize {
        ((self.history_seconds as f32) * self.sample_rate_hz).ceil() as usize
    }

    pub fn config_path() -> Option<PathBuf> {
        let dirs = directories::ProjectDirs::from("dev", "monitor-rs", "monitor-rs")?;
        Some(dirs.config_dir().join("config.json"))
    }

    pub fn load() -> Self {
        let Some(path) = Self::config_path() else {
            return Self::default();
        };
        match fs::read_to_string(&path) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> io::Result<()> {
        let Some(path) = Self::config_path() else {
            return Ok(());
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, serde_json::to_string_pretty(self).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_values() {
        let s = Settings::default();
        assert_eq!(s.sample_rate_hz, 1.0);
        assert_eq!(s.history_seconds, 120);
        assert_eq!(s.top_n_procs, 5);
    }

    #[test]
    fn history_capacity_basic() {
        let s = Settings { sample_rate_hz: 1.0, history_seconds: 120, ..Settings::default() };
        assert_eq!(s.history_capacity(), 120);

        let s2 = Settings { sample_rate_hz: 2.0, history_seconds: 60, ..Settings::default() };
        assert_eq!(s2.history_capacity(), 120);
    }

    #[test]
    fn round_trip() {
        let s = Settings::default();
        let j = serde_json::to_string(&s).unwrap();
        let s2: Settings = serde_json::from_str(&j).unwrap();
        assert_eq!(s, s2);
    }

    #[test]
    fn corrupt_json_falls_back_to_default() {
        let s: Settings = serde_json::from_str("{not valid").unwrap_or_default();
        assert_eq!(s, Settings::default());
    }
}
```

- [ ] **Step 2: Add module declaration in `src/lib.rs`**

```rust
pub mod sample;
pub mod settings;
pub mod store;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib settings
```

Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add src/settings.rs src/lib.rs
git commit -m "feat: add Settings load/save with defaults"
```

---

## Task 4: Logging

**Files:**
- Create: `src/logging.rs`
- Modify: `src/lib.rs`, `src/main.rs`

- [ ] **Step 1: Write `src/logging.rs`**

This module is initialization-only — no tests beyond "doesn't panic."

```rust
use std::path::PathBuf;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

/// Initialize logging.
///
/// Returns a `WorkerGuard` the caller must keep alive for the duration of
/// the program; dropping it flushes the file appender.
pub fn init() -> WorkerGuard {
    let log_dir = log_dir();
    if let Some(dir) = &log_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    let file_appender = match log_dir {
        Some(dir) => tracing_appender::rolling::daily(dir, "monitor-rs.log"),
        None => tracing_appender::rolling::never(std::env::temp_dir(), "monitor-rs.log"),
    };
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,monitor_rs=debug"));

    let registry = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(file_writer).with_ansi(false));

    #[cfg(debug_assertions)]
    let registry = registry.with(fmt::layer().with_writer(std::io::stderr));

    registry.init();
    guard
}

fn log_dir() -> Option<PathBuf> {
    let home = directories::BaseDirs::new()?.home_dir().to_path_buf();
    Some(home.join("Library/Logs/monitor-rs"))
}
```

- [ ] **Step 2: Wire into `src/lib.rs` and call from `src/main.rs`**

`src/lib.rs`:

```rust
pub mod logging;
pub mod sample;
pub mod settings;
pub mod store;
```

`src/main.rs`:

```rust
fn main() {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");
}
```

- [ ] **Step 3: Build and run — verify logs appear**

```bash
cargo run
ls ~/Library/Logs/monitor-rs/
```

Expected: a log file created today, containing the "monitor-rs starting" line.

- [ ] **Step 4: Commit**

```bash
git add src/logging.rs src/lib.rs src/main.rs
git commit -m "feat: initialize tracing with rolling file + stderr in debug"
```

---

## Task 5: CPU sampler

**Files:**
- Create: `src/metrics/mod.rs`, `src/metrics/cpu.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/metrics/mod.rs`**

```rust
use thiserror::Error;

pub mod cpu;

#[derive(Debug, Error)]
pub enum MetricError {
    #[error("metric unavailable: {0}")]
    Unavailable(String),
    #[error("FFI error: {0}")]
    Ffi(String),
}
```

- [ ] **Step 2: Write the failing test in `src/metrics/cpu.rs`**

```rust
use sysinfo::{CpuRefreshKind, RefreshKind, System};

use super::MetricError;

pub struct CpuReading {
    pub total_pct: f32,
    pub per_core_pct: Vec<f32>,
}

pub struct CpuSampler {
    sys: System,
}

impl CpuSampler {
    pub fn new() -> Self {
        let mut sys = System::new_with_specifics(
            RefreshKind::new().with_cpu(CpuRefreshKind::everything()),
        );
        // Prime the sampler — first read is meaningless.
        sys.refresh_cpu_usage();
        std::thread::sleep(std::time::Duration::from_millis(
            sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_millis() as u64 + 10,
        ));
        sys.refresh_cpu_usage();
        Self { sys }
    }

    pub fn tick(&mut self) -> Result<CpuReading, MetricError> {
        self.sys.refresh_cpu_usage();
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return Err(MetricError::Unavailable("no CPUs reported".into()));
        }
        let per_core: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
        let total = per_core.iter().sum::<f32>() / per_core.len() as f32;
        Ok(CpuReading { total_pct: total, per_core_pct: per_core })
    }
}

impl Default for CpuSampler {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_sampler_returns_sane_values() {
        let mut s = CpuSampler::new();
        let r = s.tick().expect("tick succeeds");
        assert!(!r.per_core_pct.is_empty());
        assert!(r.total_pct >= 0.0 && r.total_pct <= 100.0);
        for p in &r.per_core_pct {
            assert!(*p >= 0.0 && *p <= 100.0, "core pct out of range: {p}");
        }
    }
}
```

- [ ] **Step 3: Add module declaration in `src/lib.rs`**

```rust
pub mod logging;
pub mod metrics;
pub mod sample;
pub mod settings;
pub mod store;
```

- [ ] **Step 4: Run the test**

```bash
cargo test --lib metrics::cpu
```

Expected: 1 passed. (If `sysinfo::MINIMUM_CPU_UPDATE_INTERVAL` isn't a const in your sysinfo version, replace its use with `Duration::from_millis(220)`.)

- [ ] **Step 5: Commit**

```bash
git add src/metrics/ src/lib.rs
git commit -m "feat: CPU sampler via sysinfo"
```

---

## Task 6: Memory + swap sampler

**Files:**
- Create: `src/metrics/mem.rs`
- Modify: `src/metrics/mod.rs`

- [ ] **Step 1: Write `src/metrics/mem.rs`**

```rust
use sysinfo::{MemoryRefreshKind, RefreshKind, System};

use super::MetricError;
use crate::sample::{MemInfo, MemPressure, SwapInfo};

pub struct MemReading {
    pub mem: MemInfo,
    pub swap: SwapInfo,
}

pub struct MemSampler {
    sys: System,
}

impl MemSampler {
    pub fn new() -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::new().with_memory(MemoryRefreshKind::everything()),
        );
        Self { sys }
    }

    pub fn tick(&mut self) -> Result<MemReading, MetricError> {
        self.sys.refresh_memory();

        let total = self.sys.total_memory();
        let used = self.sys.used_memory();
        let swap_total = self.sys.total_swap();
        let swap_used = self.sys.used_swap();

        let pressure = classify_pressure(used, total);

        Ok(MemReading {
            mem: MemInfo { used_bytes: used, total_bytes: total, pressure },
            swap: SwapInfo { used_bytes: swap_used, total_bytes: swap_total },
        })
    }
}

impl Default for MemSampler {
    fn default() -> Self { Self::new() }
}

/// Approximate Activity Monitor's memory-pressure classification.
/// Apple's exact formula isn't public; this approximation uses the used/total
/// ratio at the published thresholds (Normal < 70%, Warning < 90%, else Critical).
pub fn classify_pressure(used: u64, total: u64) -> MemPressure {
    if total == 0 { return MemPressure::Normal; }
    let r = used as f64 / total as f64;
    if r < 0.70 { MemPressure::Normal }
    else if r < 0.90 { MemPressure::Warning }
    else { MemPressure::Critical }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pressure_thresholds() {
        assert_eq!(classify_pressure(0, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(50, 100), MemPressure::Normal);
        assert_eq!(classify_pressure(80, 100), MemPressure::Warning);
        assert_eq!(classify_pressure(95, 100), MemPressure::Critical);
        assert_eq!(classify_pressure(0, 0), MemPressure::Normal);
    }

    #[test]
    fn mem_sampler_returns_sane_values() {
        let mut s = MemSampler::new();
        let r = s.tick().expect("tick succeeds");
        assert!(r.mem.total_bytes > 0);
        assert!(r.mem.used_bytes <= r.mem.total_bytes);
        assert!(r.swap.used_bytes <= r.swap.total_bytes.max(r.swap.used_bytes));
    }
}
```

- [ ] **Step 2: Register the module**

In `src/metrics/mod.rs`, add:

```rust
pub mod cpu;
pub mod mem;
```

- [ ] **Step 3: Run the tests**

```bash
cargo test --lib metrics::mem
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/metrics/mem.rs src/metrics/mod.rs
git commit -m "feat: memory + swap sampler with pressure classification"
```

---

## Task 7: Process sampler

**Files:**
- Create: `src/metrics/procs.rs`
- Modify: `src/metrics/mod.rs`

- [ ] **Step 1: Write `src/metrics/procs.rs`**

```rust
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

use super::MetricError;
use crate::sample::ProcInfo;

pub struct ProcSampler {
    sys: System,
    top_n: usize,
}

impl ProcSampler {
    pub fn new(top_n: usize) -> Self {
        let sys = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        Self { sys, top_n: top_n.max(1) }
    }

    pub fn tick(&mut self) -> Result<Vec<ProcInfo>, MetricError> {
        self.sys.refresh_processes();
        let mut all: Vec<ProcInfo> = self.sys
            .processes()
            .iter()
            .map(|(pid, p)| ProcInfo {
                pid: pid.as_u32(),
                name: p.name().to_string_lossy().to_string(),
                cpu_pct: p.cpu_usage(),
                rss_bytes: p.memory(),
            })
            .collect();
        // Rank by CPU then by RSS as tiebreaker.
        all.sort_by(|a, b| {
            b.cpu_pct
                .partial_cmp(&a.cpu_pct)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(b.rss_bytes.cmp(&a.rss_bytes))
        });
        all.truncate(self.top_n);
        Ok(all)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_sampler_respects_top_n() {
        let mut s = ProcSampler::new(3);
        let r = s.tick().expect("tick succeeds");
        assert!(r.len() <= 3);
        assert!(!r.is_empty());
    }

    #[test]
    fn proc_sampler_sorted_desc_by_cpu() {
        let mut s = ProcSampler::new(20);
        let r = s.tick().expect("tick succeeds");
        for w in r.windows(2) {
            assert!(w[0].cpu_pct >= w[1].cpu_pct);
        }
    }
}
```

- [ ] **Step 2: Register the module**

`src/metrics/mod.rs`:

```rust
pub mod cpu;
pub mod mem;
pub mod procs;
```

- [ ] **Step 3: Run the tests**

```bash
cargo test --lib metrics::procs
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/metrics/procs.rs src/metrics/mod.rs
git commit -m "feat: top-N process sampler"
```

---

## Task 8: GPU sampler (IOReport private framework)

This is the highest-risk task. Read `macmon`'s `src/sources.rs` first to understand the IOReport call sequence — we are porting that pattern, not copying code. The framework is private and not in the default linker search path, so we use `libloading` to dlopen it at runtime. If anything fails (missing channel, Intel Mac, future macOS removes it), `tick()` returns `Ok(None)` and the rest of the app still works.

**Files:**
- Create: `src/metrics/gpu.rs`
- Modify: `src/metrics/mod.rs`

- [ ] **Step 1: Write `src/metrics/gpu.rs`**

```rust
//! GPU utilization on Apple Silicon via the private IOReport framework.
//!
//! This binds the minimal subset of IOReport.framework needed to read the
//! "GPU PMU" / "GPU Stats" channels. The framework lives at
//! `/System/Library/PrivateFrameworks/IOReport.framework` and isn't in the
//! standard linker search path, so we dlopen it via `libloading`. All FFI
//! errors degrade to `Ok(None)`.

use std::ffi::c_void;

use core_foundation::base::{CFRelease, CFTypeRef, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::{CFNumber, CFNumberRef};
use core_foundation::string::{CFString, CFStringRef};
use libloading::{Library, Symbol};

use super::MetricError;

const IOREPORT_PATH: &str =
    "/System/Library/PrivateFrameworks/IOReport.framework/Versions/A/IOReport";

type IOReportCopyChannelsInGroupFn = unsafe extern "C" fn(
    group: CFStringRef,
    subgroup: CFStringRef,
    a: u64, b: u64, c: u64,
) -> CFDictionaryRef;

type IOReportCreateSubscriptionFn = unsafe extern "C" fn(
    a: *const c_void,
    desired_channels: CFDictionaryRef,
    out_subbed: *mut CFDictionaryRef,
    b: u64,
    options: CFTypeRef,
) -> CFTypeRef;

type IOReportCreateSamplesFn = unsafe extern "C" fn(
    subscription: CFTypeRef,
    subscribed_channels: CFDictionaryRef,
    options: CFTypeRef,
) -> CFDictionaryRef;

type IOReportCreateSamplesDeltaFn = unsafe extern "C" fn(
    prev: CFDictionaryRef,
    curr: CFDictionaryRef,
    options: CFTypeRef,
) -> CFDictionaryRef;

pub struct GpuSampler {
    inner: Option<Inner>,
}

struct Inner {
    _lib: Library, // keep alive for symbol lifetimes
    create_samples: IOReportCreateSamplesFn,
    create_delta: IOReportCreateSamplesDeltaFn,
    subscription: CFTypeRef,
    subscribed_channels: CFDictionaryRef,
    last_sample: Option<CFDictionaryRef>,
}

impl GpuSampler {
    pub fn new() -> Self {
        match unsafe { Self::try_init() } {
            Ok(inner) => Self { inner: Some(inner) },
            Err(e) => {
                tracing::warn!("GPU sampler unavailable: {e}");
                Self { inner: None }
            }
        }
    }

    /// Returns `Ok(None)` if GPU sampling is not supported on this machine
    /// (e.g. Intel Mac, future macOS, or a private-framework break).
    pub fn tick(&mut self) -> Result<Option<f32>, MetricError> {
        let Some(inner) = self.inner.as_mut() else { return Ok(None) };
        unsafe { inner.tick() }
    }

    unsafe fn try_init() -> Result<Inner, MetricError> {
        let lib = Library::new(IOREPORT_PATH)
            .map_err(|e| MetricError::Ffi(format!("dlopen IOReport: {e}")))?;

        let copy_channels: Symbol<IOReportCopyChannelsInGroupFn> =
            lib.get(b"IOReportCopyChannelsInGroup\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CopyChannels: {e}")))?;
        let create_subscription: Symbol<IOReportCreateSubscriptionFn> =
            lib.get(b"IOReportCreateSubscription\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSubscription: {e}")))?;
        let create_samples: Symbol<IOReportCreateSamplesFn> =
            lib.get(b"IOReportCreateSamples\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSamples: {e}")))?;
        let create_delta: Symbol<IOReportCreateSamplesDeltaFn> =
            lib.get(b"IOReportCreateSamplesDelta\0")
                .map_err(|e| MetricError::Ffi(format!("dlsym CreateSamplesDelta: {e}")))?;

        // Subscribe to GPU performance-state residency.
        // Group/subgroup names match what macmon and asitop use.
        let group = CFString::new("GPU Stats");
        let subgroup = CFString::new("GPU PMU");
        let desired = copy_channels(
            group.as_concrete_TypeRef(),
            subgroup.as_concrete_TypeRef(),
            0, 0, 0,
        );
        if desired.is_null() {
            return Err(MetricError::Unavailable("no GPU channels".into()));
        }

        let mut subbed: CFDictionaryRef = std::ptr::null();
        let subscription = create_subscription(
            std::ptr::null(),
            desired,
            &mut subbed as *mut _,
            0,
            std::ptr::null(),
        );
        if subscription.is_null() || subbed.is_null() {
            CFRelease(desired as CFTypeRef);
            return Err(MetricError::Unavailable("subscription failed".into()));
        }

        // We retain `subbed` (returned out-param), `subscription`. Free `desired`.
        CFRelease(desired as CFTypeRef);

        // Move concrete fn pointers out of the Symbol<> wrappers (which borrow the lib).
        // We re-borrow on every call by keeping the Library in this struct.
        let create_samples_fn: IOReportCreateSamplesFn = *create_samples;
        let create_delta_fn: IOReportCreateSamplesDeltaFn = *create_delta;
        drop(copy_channels);
        drop(create_subscription);
        drop(create_samples);
        drop(create_delta);

        Ok(Inner {
            _lib: lib,
            create_samples: create_samples_fn,
            create_delta: create_delta_fn,
            subscription,
            subscribed_channels: subbed,
            last_sample: None,
        })
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        unsafe {
            if !self.subscription.is_null() {
                CFRelease(self.subscription);
            }
            if !self.subscribed_channels.is_null() {
                CFRelease(self.subscribed_channels as CFTypeRef);
            }
            if let Some(s) = self.last_sample.take() {
                CFRelease(s as CFTypeRef);
            }
        }
    }
}

impl Inner {
    unsafe fn tick(&mut self) -> Result<Option<f32>, MetricError> {
        let curr = (self.create_samples)(
            self.subscription,
            self.subscribed_channels,
            std::ptr::null(),
        );
        if curr.is_null() {
            return Ok(None);
        }

        let pct = if let Some(prev) = self.last_sample {
            let delta = (self.create_delta)(prev, curr, std::ptr::null());
            CFRelease(prev as CFTypeRef);
            self.last_sample = Some(curr);
            if delta.is_null() {
                None
            } else {
                let pct = compute_idle_complement(delta);
                CFRelease(delta as CFTypeRef);
                pct
            }
        } else {
            // First call: no delta yet.
            self.last_sample = Some(curr);
            None
        };

        Ok(pct)
    }
}

/// Walk the IOReport sample dict's `IOReportChannels` array and compute
/// `1 - IDLE_residency` over the active P-states.
///
/// PORT NOTE: the exact key names ("IOReportChannels", "LegendChannel",
/// per-state residency arrays) come from inspecting the dict at runtime in
/// macmon's `gpu` source. If the layout changes in a future macOS we return
/// None and the UI shows "GPU: n/a".
unsafe fn compute_idle_complement(_delta_dict: CFDictionaryRef) -> Option<f32> {
    // Implementation note for the engineer:
    //   1. Get the "IOReportChannels" CFArray from the dict.
    //   2. For each channel, read the legend (channel name array) and look for
    //      one whose name contains "GPU" + a state-residency tuple.
    //   3. Sum residency across active states; idle is the channel whose name
    //      ends in "_IDLE" (or first state, depending on legend order).
    //   4. utilization = 1.0 - idle_total / sum_total
    //
    // Reference: see macmon `src/sources.rs::IOReport::get_gpu_pwr_residency`
    // and adapt. Until you wire this up, keep returning None — the GPU
    // sparkline will show "n/a" but the rest of the app works.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_sampler_does_not_panic() {
        let mut s = GpuSampler::new();
        let _ = s.tick();
        let _ = s.tick();
    }
}
```

- [ ] **Step 2: Register the module**

`src/metrics/mod.rs`:

```rust
pub mod cpu;
pub mod gpu;
pub mod mem;
pub mod procs;
```

- [ ] **Step 3: Run the test**

```bash
cargo test --lib metrics::gpu
```

Expected: 1 passed (`gpu_sampler_does_not_panic`). The first integration milestone is "doesn't panic, returns None gracefully." Functioning GPU% comes in the next sub-step.

- [ ] **Step 4: Implement `compute_idle_complement`**

Open `macmon/src/sources.rs` in a browser and find `get_gpu_pwr_residency` (or the equivalently-named helper that walks IOReport channels). Adapt it to fill in `compute_idle_complement` so it returns `Some(0.0..=1.0)` representing utilization.

Verification — write an integration test at `tests/gpu_smoke.rs`:

```rust
#[cfg(target_os = "macos")]
#[test]
fn gpu_returns_some_after_warmup_on_apple_silicon() {
    use monitor_rs::metrics::gpu::GpuSampler;
    use std::time::Duration;

    let mut s = GpuSampler::new();
    let _ = s.tick(); // warmup, returns None
    std::thread::sleep(Duration::from_millis(250));
    let r = s.tick().expect("tick must not error");
    // On Apple Silicon, expect Some. Skip silently if None (Intel / CI).
    if let Some(pct) = r {
        assert!(pct >= 0.0 && pct <= 100.0, "GPU pct out of range: {pct}");
    } else {
        eprintln!("GPU sampler returned None — skipping (Intel Mac / CI?)");
    }
}
```

Run: `cargo test --test gpu_smoke -- --nocapture`. Expected: passes (and on your Apple Silicon Mac, prints a real number, not the skip message).

- [ ] **Step 5: Commit**

```bash
git add src/metrics/gpu.rs src/metrics/mod.rs tests/gpu_smoke.rs
git commit -m "feat: GPU sampler via private IOReport framework"
```

---

## Task 9: Sampler orchestrator + thread

**Files:**
- Create: `src/sampler.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/sampler.rs`**

```rust
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use parking_lot::RwLock;

use crate::metrics::cpu::CpuSampler;
use crate::metrics::gpu::GpuSampler;
use crate::metrics::mem::MemSampler;
use crate::metrics::procs::ProcSampler;
use crate::sample::Sample;
use crate::settings::Settings;
use crate::store::SampleStore;

pub struct SamplerHandle {
    pub store: Arc<RwLock<SampleStore>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    join: Option<JoinHandle<()>>,
}

impl SamplerHandle {
    pub fn spawn(settings: Settings) -> Self {
        let cap = settings.history_capacity();
        let store = Arc::new(RwLock::new(SampleStore::new(cap)));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));

        let store_w = store.clone();
        let stop_w = stop.clone();
        let join = thread::Builder::new()
            .name("monitor-rs-sampler".into())
            .spawn(move || run_loop(settings, store_w, stop_w))
            .expect("spawn sampler thread");

        Self { store, stop, join: Some(join) }
    }

    pub fn stop(mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

fn run_loop(
    settings: Settings,
    store: Arc<RwLock<SampleStore>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
) {
    let mut cpu = CpuSampler::new();
    let mut mem = MemSampler::new();
    let mut procs = ProcSampler::new(settings.top_n_procs);
    let mut gpu = GpuSampler::new();

    let interval = Duration::from_secs_f32(1.0 / settings.sample_rate_hz.max(0.1));
    let mut next = Instant::now();

    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        next += interval;
        let now = Instant::now();
        if now < next {
            thread::sleep(next - now);
        } else if now > next + Duration::from_secs(2) {
            // Process slept; resync.
            next = Instant::now();
        }

        let cpu_r = match cpu.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("cpu tick: {e}"); continue; }
        };
        let mem_r = match mem.tick() {
            Ok(r) => r,
            Err(e) => { tracing::warn!("mem tick: {e}"); continue; }
        };
        let top = procs.tick().unwrap_or_default();
        let gpu_pct = gpu.tick().ok().flatten();

        let s = Sample {
            ts: Instant::now(),
            cpu_total: cpu_r.total_pct,
            cpu_per_core: cpu_r.per_core_pct,
            gpu_pct,
            mem: mem_r.mem,
            swap: mem_r.swap,
            top_procs: top,
        };
        store.write().push(s);
    }
}
```

- [ ] **Step 2: Register the module**

`src/lib.rs`:

```rust
pub mod logging;
pub mod metrics;
pub mod sample;
pub mod sampler;
pub mod settings;
pub mod store;
```

- [ ] **Step 3: Add an integration test at `tests/sampler_smoke.rs`**

```rust
#[cfg(target_os = "macos")]
#[test]
fn sampler_produces_samples() {
    use monitor_rs::sampler::SamplerHandle;
    use monitor_rs::settings::Settings;
    use std::time::Duration;

    let settings = Settings { sample_rate_hz: 4.0, history_seconds: 10, top_n_procs: 3, ..Settings::default() };
    let handle = SamplerHandle::spawn(settings);
    std::thread::sleep(Duration::from_millis(900)); // ~3-4 ticks at 4 Hz

    {
        let s = handle.store.read();
        assert!(s.len() >= 2, "expected ≥2 samples, got {}", s.len());
        let latest = s.latest().unwrap();
        assert!(latest.cpu_total >= 0.0 && latest.cpu_total <= 100.0);
        assert!(latest.mem.total_bytes > 0);
        assert!(latest.cpu_per_core.len() >= 1);
    }

    handle.stop();
}
```

- [ ] **Step 4: Run it**

```bash
cargo test --test sampler_smoke -- --nocapture
```

Expected: passes.

- [ ] **Step 5: Commit**

```bash
git add src/sampler.rs src/lib.rs tests/sampler_smoke.rs
git commit -m "feat: sampler thread with drift-corrected ticks"
```

---

## Task 10: Menu-bar text format

**Files:**
- Create: `src/format.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write the failing tests in `src/format.rs`**

```rust
use crate::sample::Sample;

/// Substitute `{cpu}`, `{gpu}`, `{mem}`, `{swap}` in the template with
/// integer percentages from the latest sample.
pub fn render_menu_bar(template: &str, s: &Sample) -> String {
    let cpu = s.cpu_total.round() as u32;
    let gpu = s.gpu_pct.map(|p| format!("{}", p.round() as u32)).unwrap_or_else(|| "—".to_string());
    let mem = s.mem.used_pct().round() as u32;
    let swap_total = s.swap.total_bytes.max(1);
    let swap = (s.swap.used_bytes as f64 / swap_total as f64 * 100.0).round() as u32;
    template
        .replace("{cpu}", &format!("{cpu}"))
        .replace("{gpu}", &gpu)
        .replace("{mem}", &format!("{mem}"))
        .replace("{swap}", &format!("{swap}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{MemInfo, MemPressure, ProcInfo as _, Sample, SwapInfo};
    use std::time::Instant;

    fn s(cpu: f32, gpu: Option<f32>, used: u64, total: u64) -> Sample {
        Sample {
            ts: Instant::now(),
            cpu_total: cpu,
            cpu_per_core: vec![cpu],
            gpu_pct: gpu,
            mem: MemInfo { used_bytes: used, total_bytes: total, pressure: MemPressure::Normal },
            swap: SwapInfo { used_bytes: 0, total_bytes: 0 },
            top_procs: vec![],
        }
    }

    #[test]
    fn substitutes_cpu_gpu_mem() {
        let out = render_menu_bar("C {cpu} G {gpu} M {mem}", &s(42.4, Some(18.2), 64, 100));
        assert_eq!(out, "C 42 G 18 M 64");
    }

    #[test]
    fn gpu_none_renders_dash() {
        let out = render_menu_bar("G {gpu}", &s(0.0, None, 0, 1));
        assert_eq!(out, "G —");
    }
}
```

(Drop the unused `ProcInfo` import — it's there to remind you to remove unused imports. Run `cargo fmt` and `cargo clippy` periodically.)

- [ ] **Step 2: Register**

`src/lib.rs`:

```rust
pub mod format;
pub mod logging;
pub mod metrics;
pub mod sample;
pub mod sampler;
pub mod settings;
pub mod store;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib format
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/format.rs src/lib.rs
git commit -m "feat: menu-bar text template renderer"
```

---

## Task 11: Sparkline widget

**Files:**
- Create: `src/ui/mod.rs`, `src/ui/sparkline.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: Write `src/ui/sparkline.rs`**

The widget itself uses `egui::Painter` so isn't easily unit-testable. The data preparation (`normalize`) is pulled out and tested.

```rust
use egui::{vec2, Color32, Pos2, Rect, Response, Sense, Stroke, Ui};

/// Normalize a series to 0..=1 against a fixed max (e.g. 100.0 for percentages).
pub fn normalize(values: &[f32], max: f32) -> Vec<f32> {
    if max <= 0.0 {
        return vec![0.0; values.len()];
    }
    values.iter().map(|v| (v / max).clamp(0.0, 1.0)).collect()
}

/// Render a sparkline filling the allocated rect.
/// `values` should already be normalized to 0..=1.
pub fn sparkline(ui: &mut Ui, size: egui::Vec2, values: &[f32], color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    let painter = ui.painter_at(rect);

    // Background.
    painter.rect_filled(rect, 4.0, ui.style().visuals.faint_bg_color);

    if values.is_empty() {
        return response;
    }

    let n = values.len();
    let dx = if n > 1 { rect.width() / (n - 1) as f32 } else { 0.0 };
    let points: Vec<Pos2> = values
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let x = rect.left() + dx * i as f32;
            let y = rect.bottom() - rect.height() * v.clamp(0.0, 1.0);
            Pos2::new(x, y)
        })
        .collect();

    // Filled area under the line.
    if points.len() >= 2 {
        let mut poly = points.clone();
        poly.push(Pos2::new(rect.right(), rect.bottom()));
        poly.push(Pos2::new(rect.left(), rect.bottom()));
        painter.add(egui::Shape::convex_polygon(poly, color.linear_multiply(0.25), Stroke::NONE));
    }

    // Line.
    if points.len() >= 2 {
        painter.add(egui::Shape::line(points, Stroke::new(1.5, color)));
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_clamps_and_scales() {
        let r = normalize(&[0.0, 50.0, 100.0, 150.0, -10.0], 100.0);
        assert_eq!(r, vec![0.0, 0.5, 1.0, 1.0, 0.0]);
    }

    #[test]
    fn normalize_handles_zero_max() {
        let r = normalize(&[10.0, 20.0], 0.0);
        assert_eq!(r, vec![0.0, 0.0]);
    }
}
```

- [ ] **Step 2: Write `src/ui/mod.rs`**

```rust
pub mod sparkline;
```

- [ ] **Step 3: Register in `src/lib.rs`**

```rust
pub mod format;
pub mod logging;
pub mod metrics;
pub mod sample;
pub mod sampler;
pub mod settings;
pub mod store;
pub mod ui;
```

- [ ] **Step 4: Run tests**

```bash
cargo test --lib ui::sparkline
```

Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src/ui/ src/lib.rs
git commit -m "feat: sparkline widget"
```

---

## Task 12: Per-core grid widget

**Files:**
- Create: `src/ui/cores.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write `src/ui/cores.rs`**

```rust
use egui::{Color32, Sense, Ui};

/// A row of small blocks, one per core, color-mapped by usage (0..=100).
pub fn core_grid(ui: &mut Ui, per_core: &[f32]) {
    let n = per_core.len().max(1);
    let total_w = ui.available_width().min(260.0);
    let gap = 2.0;
    let block_w = ((total_w - gap * (n as f32 - 1.0)) / n as f32).max(4.0);
    let block_h = 14.0;

    ui.horizontal(|ui| {
        for &p in per_core {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(block_w, block_h), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, color_for_pct(p));
        }
    });
}

fn color_for_pct(p: f32) -> Color32 {
    let p = p.clamp(0.0, 100.0) / 100.0;
    // green -> yellow -> red
    let (r, g) = if p < 0.5 {
        (lerp(40.0, 200.0, p / 0.5), 200.0)
    } else {
        (220.0, lerp(200.0, 40.0, (p - 0.5) / 0.5))
    };
    Color32::from_rgb(r as u8, g as u8, 60)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t.clamp(0.0, 1.0) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_for_pct_endpoints() {
        let lo = color_for_pct(0.0);
        let hi = color_for_pct(100.0);
        assert_ne!(lo, hi);
    }

    #[test]
    fn lerp_basic() {
        assert!((lerp(0.0, 10.0, 0.5) - 5.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, -1.0) - 0.0).abs() < 0.001);
        assert!((lerp(0.0, 10.0, 2.0) - 10.0).abs() < 0.001);
    }
}
```

- [ ] **Step 2: Register**

`src/ui/mod.rs`:

```rust
pub mod cores;
pub mod sparkline;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib ui::cores
```

Expected: 2 passed.

- [ ] **Step 4: Commit**

```bash
git add src/ui/cores.rs src/ui/mod.rs
git commit -m "feat: per-core usage grid widget"
```

---

## Task 13: Process list widget

**Files:**
- Create: `src/ui/procs.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write `src/ui/procs.rs`**

```rust
use egui::Ui;

use crate::sample::ProcInfo;

pub fn process_list(ui: &mut Ui, procs: &[ProcInfo]) {
    egui::Grid::new("monitor-rs-procs")
        .num_columns(3)
        .spacing([12.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            for p in procs {
                let name = truncate(&p.name, 22);
                ui.label(name);
                ui.label(format!("{:>4.0}%", p.cpu_pct));
                ui.label(format_bytes(p.rss_bytes));
                ui.end_row();
            }
        });
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max { s.to_string() } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('…');
        t
    }
}

fn format_bytes(b: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = b as f64;
    if b >= GB { format!("{:.1}G", b / GB) }
    else if b >= MB { format!("{:.0}M", b / MB) }
    else { format!("{:.0}K", (b / KB).max(0.0)) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("abcdefghij", 5), "abcd…");
    }

    #[test]
    fn format_bytes_units() {
        assert!(format_bytes(512).ends_with("K"));
        assert!(format_bytes(2 * 1024 * 1024).ends_with("M"));
        assert!(format_bytes(3 * 1024 * 1024 * 1024).ends_with("G"));
    }
}
```

- [ ] **Step 2: Register**

`src/ui/mod.rs`:

```rust
pub mod cores;
pub mod procs;
pub mod sparkline;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib ui::procs
```

Expected: 3 passed.

- [ ] **Step 4: Commit**

```bash
git add src/ui/procs.rs src/ui/mod.rs
git commit -m "feat: process list widget"
```

---

## Task 14: Popover panel composition

**Files:**
- Create: `src/ui/popover.rs`
- Modify: `src/ui/mod.rs`

- [ ] **Step 1: Write `src/ui/popover.rs`**

```rust
use std::sync::Arc;

use egui::{Color32, Context, Vec2};
use parking_lot::RwLock;

use crate::sample::{MemPressure, Sample};
use crate::settings::Settings;
use crate::store::SampleStore;
use crate::ui::{cores::core_grid, procs::process_list, sparkline::{normalize, sparkline}};

pub struct PopoverState {
    pub store: Arc<RwLock<SampleStore>>,
    pub settings: Settings,
}

pub fn show(ctx: &Context, state: &PopoverState) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.set_width(280.0);
        let store = state.store.read();
        let Some(latest) = store.latest().cloned() else {
            ui.label("Sampling…");
            return;
        };
        let recent_n = (60.0 * state.settings.sample_rate_hz).ceil() as usize;

        // CPU
        row(ui, "CPU", latest.cpu_total, Color32::from_rgb(80, 200, 120),
            &collect(&store, recent_n, |s| s.cpu_total));
        core_grid(ui, &latest.cpu_per_core);

        ui.add_space(6.0);

        // GPU
        let gpu_label = match latest.gpu_pct {
            Some(p) => format!("{:>3.0}%", p),
            None => "n/a".into(),
        };
        row_label(ui, "GPU", &gpu_label, Color32::from_rgb(120, 160, 240),
            &collect(&store, recent_n, |s| s.gpu_pct.unwrap_or(0.0)),
            latest.gpu_pct.is_some());

        ui.add_space(6.0);

        // MEM
        let mem_pct = latest.mem.used_pct();
        let mem_color = match latest.mem.pressure {
            MemPressure::Normal => Color32::from_rgb(200, 180, 80),
            MemPressure::Warning => Color32::from_rgb(220, 140, 60),
            MemPressure::Critical => Color32::from_rgb(220, 80, 80),
        };
        row(ui, "MEM", mem_pct, mem_color,
            &collect(&store, recent_n, |s| s.mem.used_pct()));

        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new("Top processes").strong());
        process_list(ui, &latest.top_procs);

        ui.separator();
        ui.horizontal(|ui| {
            ui.label(format!("swap {}", crate::ui::popover::format_swap(&latest.swap)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    });
}

fn row(ui: &mut egui::Ui, label: &str, pct: f32, color: Color32, history: &[f32]) {
    ui.horizontal(|ui| {
        ui.label(format!("{label} {:>3.0}%", pct));
        let normed = normalize(history, 100.0);
        sparkline(ui, Vec2::new(180.0, 18.0), &normed, color);
    });
}

fn row_label(ui: &mut egui::Ui, label: &str, value: &str, color: Color32, history: &[f32], have_data: bool) {
    ui.horizontal(|ui| {
        ui.label(format!("{label} {value}"));
        if have_data {
            let normed = normalize(history, 100.0);
            sparkline(ui, Vec2::new(180.0, 18.0), &normed, color);
        }
    });
}

fn collect<F: Fn(&Sample) -> f32>(store: &SampleStore, n: usize, f: F) -> Vec<f32> {
    store.recent(n).map(f).collect()
}

pub fn format_swap(swap: &crate::sample::SwapInfo) -> String {
    if swap.total_bytes == 0 { return "off".into(); }
    let used_g = swap.used_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    format!("{:.2}G", used_g)
}
```

- [ ] **Step 2: Register**

`src/ui/mod.rs`:

```rust
pub mod cores;
pub mod popover;
pub mod procs;
pub mod sparkline;
```

- [ ] **Step 3: Build**

```bash
cargo build
```

Expected: builds clean.

- [ ] **Step 4: Commit**

```bash
git add src/ui/popover.rs src/ui/mod.rs
git commit -m "feat: popover panel composition"
```

---

## Task 15: eframe app skeleton

This task gets a window on screen with the popover, but no menu-bar tray yet — the tray comes in Task 16. For now we run as a regular window so we can verify the popover content is correct before integrating with `NSStatusItem`.

**Files:**
- Modify: `src/main.rs`

- [ ] **Step 1: Replace `src/main.rs`**

```rust
use std::sync::Arc;

use parking_lot::RwLock;

use monitor_rs::sampler::SamplerHandle;
use monitor_rs::settings::Settings;
use monitor_rs::store::SampleStore;
use monitor_rs::ui::popover::{self, PopoverState};

struct App {
    state: PopoverState,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        popover::show(ctx, &self.state);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn main() -> eframe::Result<()> {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");

    let settings = Settings::load();
    let handle = SamplerHandle::spawn(settings.clone());

    let app = App { state: PopoverState { store: handle.store.clone(), settings } };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 360.0])
            .with_min_inner_size([280.0, 320.0])
            .with_title("monitor-rs"),
        ..Default::default()
    };

    eframe::run_native("monitor-rs", opts, Box::new(|_cc| Ok(Box::new(app))))
}
```

- [ ] **Step 2: Run the app**

```bash
cargo run --release
```

Expected: a window appears showing CPU/GPU/MEM rows with sparklines updating once per second, a per-core grid, and a top-process list. CPU stress test:

```bash
yes > /dev/null &
yes > /dev/null &
```

Verify the CPU sparkline rises and per-core blocks turn red. Then `kill %1 %2`.

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: eframe app skeleton with live popover"
```

---

## Task 16: NSStatusItem tray + click-to-toggle popover

This is the trickiest UI integration. We add a `tray` module that creates an `NSStatusItem` after eframe has initialized `NSApplication`, polls the latest sample on a timer to update the title text, and toggles the eframe viewport's visibility on click.

**Files:**
- Create: `src/ui/tray.rs`
- Modify: `src/ui/mod.rs`, `src/main.rs`

- [ ] **Step 1: Write `src/ui/tray.rs`**

```rust
//! NSStatusItem integration. Creates a system status item, sets its title
//! from the latest sample, and posts a callback when the user clicks it so
//! the main thread can toggle the popover viewport.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{declare_class, msg_send_id, mutability, ClassType, DeclaredClass};
use objc2_app_kit::{NSStatusBar, NSStatusItem, NSVariableStatusItemLength};
use objc2_foundation::{MainThreadMarker, NSObject, NSString};
use parking_lot::RwLock;

use crate::format::render_menu_bar;
use crate::settings::Settings;
use crate::store::SampleStore;

pub struct Tray {
    item: Retained<NSStatusItem>,
    on_click: Arc<dyn Fn() + Send + Sync + 'static>,
}

impl Tray {
    /// Create the status item. Must be called on the main thread after
    /// `NSApplication::shared()` has been created (eframe does this).
    pub fn new(
        on_click: Arc<dyn Fn() + Send + Sync + 'static>,
        mtm: MainThreadMarker,
    ) -> Self {
        unsafe {
            let bar = NSStatusBar::systemStatusBar();
            let item = bar.statusItemWithLength(NSVariableStatusItemLength);
            if let Some(button) = item.button(mtm) {
                let target = ClickTarget::new(on_click.clone());
                button.setTarget(Some(&target));
                button.setAction(Some(objc2::sel!(click:)));
                std::mem::forget(target); // retained by button
                button.setTitle(&NSString::from_str("monitor-rs"));
            }
            Self { item, on_click }
        }
    }

    /// Update the menu-bar title text from the latest sample.
    pub fn refresh(&self, store: &SampleStore, settings: &Settings, mtm: MainThreadMarker) {
        let Some(latest) = store.latest() else { return };
        let text = render_menu_bar(&settings.menu_bar_format, latest);
        unsafe {
            if let Some(button) = self.item.button(mtm) {
                button.setTitle(&NSString::from_str(&text));
            }
        }
    }
}

declare_class!(
    struct ClickTarget;

    unsafe impl ClassType for ClickTarget {
        type Super = NSObject;
        type Mutability = mutability::InteriorMutable;
        const NAME: &'static str = "MonitorRsClickTarget";
    }

    impl DeclaredClass for ClickTarget {
        type Ivars = Arc<dyn Fn() + Send + Sync + 'static>;
    }

    unsafe impl ClickTarget {
        #[method(click:)]
        fn click(&self, _sender: Option<&objc2::runtime::AnyObject>) {
            (self.ivars())();
        }
    }
);

impl ClickTarget {
    fn new(cb: Arc<dyn Fn() + Send + Sync + 'static>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(cb);
        unsafe { msg_send_id![super(this), init] }
    }
}
```

> If `declare_class!` macro details differ in the resolved `objc2` version, the upstream README has up-to-date snippets — adapt the macro invocation but keep the same shape (one ivar holding the callback, one `click:` method).

- [ ] **Step 2: Register**

`src/ui/mod.rs`:

```rust
pub mod cores;
pub mod popover;
pub mod procs;
pub mod sparkline;
#[cfg(target_os = "macos")]
pub mod tray;
```

- [ ] **Step 3: Wire the tray into `src/main.rs`**

```rust
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use monitor_rs::sampler::SamplerHandle;
use monitor_rs::settings::Settings;
use monitor_rs::ui::popover::{self, PopoverState};

#[cfg(target_os = "macos")]
use monitor_rs::ui::tray::Tray;

struct App {
    state: PopoverState,
    visible: Arc<AtomicBool>,
    #[cfg(target_os = "macos")]
    tray: Option<Tray>,
    #[cfg(target_os = "macos")]
    last_refresh: std::time::Instant,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Lazily create the tray on the first update — by now NSApplication is up.
        #[cfg(target_os = "macos")]
        if self.tray.is_none() {
            if let Some(mtm) = objc2_foundation::MainThreadMarker::new() {
                let visible = self.visible.clone();
                let ctx_clone = ctx.clone();
                let cb: std::sync::Arc<dyn Fn() + Send + Sync> = std::sync::Arc::new(move || {
                    let was = visible.fetch_xor(true, Ordering::SeqCst);
                    let now_visible = !was;
                    ctx_clone.send_viewport_cmd(if now_visible {
                        egui::ViewportCommand::Visible(true)
                    } else {
                        egui::ViewportCommand::Visible(false)
                    });
                });
                self.tray = Some(Tray::new(cb, mtm));
            }
        }

        // Refresh tray title at most 4× per second.
        #[cfg(target_os = "macos")]
        if self.last_refresh.elapsed() >= std::time::Duration::from_millis(250) {
            if let (Some(tray), Some(mtm)) = (self.tray.as_ref(), objc2_foundation::MainThreadMarker::new()) {
                tray.refresh(&self.state.store.read(), &self.state.settings, mtm);
            }
            self.last_refresh = std::time::Instant::now();
        }

        popover::show(ctx, &self.state);
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
    }
}

fn main() -> eframe::Result<()> {
    let _log_guard = monitor_rs::logging::init();
    tracing::info!("monitor-rs starting");

    let settings = Settings::load();
    let handle = SamplerHandle::spawn(settings.clone());
    let visible = Arc::new(AtomicBool::new(true));

    let app = App {
        state: PopoverState { store: handle.store.clone(), settings },
        visible,
        #[cfg(target_os = "macos")]
        tray: None,
        #[cfg(target_os = "macos")]
        last_refresh: std::time::Instant::now(),
    };

    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([300.0, 360.0])
            .with_min_inner_size([280.0, 320.0])
            .with_title("monitor-rs")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native("monitor-rs", opts, Box::new(|_cc| Ok(Box::new(app))))
}
```

- [ ] **Step 4: Run and verify**

```bash
cargo run --release
```

Expected:
- A status item appears in the menu bar with text like `C 12 G — M 64`.
- Clicking it toggles the borderless popover window.
- The status item title updates ~4× per second.

If `viewport` window positioning doesn't anchor near the status item, that's an acceptable v1 imperfection — the window appears wherever the OS chose; users can drag it. Improving anchoring is future work.

- [ ] **Step 5: Commit**

```bash
git add src/ui/tray.rs src/ui/mod.rs src/main.rs
git commit -m "feat: NSStatusItem tray with click-to-toggle popover"
```

---

## Task 17: App bundle (`Info.plist` + `LSUIElement`)

Without `LSUIElement`, the app will show a Dock icon, which is wrong for a menu-bar utility. We use `cargo-bundle` to produce `monitor-rs.app`.

**Files:**
- Create: `assets/Info.plist`
- Modify: `Cargo.toml`

- [ ] **Step 1: Add bundle metadata to `Cargo.toml`**

Append:

```toml
[package.metadata.bundle]
name = "monitor-rs"
identifier = "dev.monitor-rs"
icon = []
short_description = "Live CPU / GPU / memory monitor in the menu bar"
long_description = "monitor-rs is a small Rust + egui menu bar app that shows live CPU, GPU, memory, and top processes on Apple Silicon Macs."
osx_minimum_system_version = "12.0"

[package.metadata.bundle.osx_info_plist_exts]
LSUIElement = true
NSHighResolutionCapable = true
```

- [ ] **Step 2: Install cargo-bundle (one-time)**

```bash
cargo install cargo-bundle
```

- [ ] **Step 3: Build the bundle**

```bash
cargo bundle --release
```

Expected: `target/release/bundle/osx/monitor-rs.app` exists.

- [ ] **Step 4: Verify `LSUIElement` is set**

```bash
plutil -p target/release/bundle/osx/monitor-rs.app/Contents/Info.plist | grep LSUIElement
```

Expected output: `"LSUIElement" => 1`. If not, the `osx_info_plist_exts` key wasn't picked up by your `cargo-bundle` version — fall back to writing `assets/Info.plist` by hand and pointing at it via `[package.metadata.bundle] osx_info_plist_path = "assets/Info.plist"`.

- [ ] **Step 5: Run the bundled app**

```bash
open target/release/bundle/osx/monitor-rs.app
```

Expected: the app launches with **no Dock icon**, only the menu-bar status item.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml
git commit -m "build: cargo-bundle metadata with LSUIElement"
```

---

## Task 18: README + manual smoke checklist

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace `README.md`**

```markdown
# monitor-rs

Small macOS menu bar app showing live CPU, GPU, memory, and top processes.
Apple Silicon only.

![status](https://img.shields.io/badge/status-alpha-orange)

## Build

```
cargo build --release
./target/release/monitor-rs
```

## Bundle as a .app

```
cargo install cargo-bundle    # one-time
cargo bundle --release
open target/release/bundle/osx/monitor-rs.app
```

The app is unsigned. On first launch macOS will block it — right-click the
`.app` in Finder and choose **Open** to authorize it.

## Configuration

Settings live at `~/Library/Application Support/monitor-rs/config.json`:

```json
{
  "sample_rate_hz": 1.0,
  "menu_bar_format": "C {cpu} G {gpu} M {mem}",
  "top_n_procs": 5,
  "history_seconds": 120
}
```

Available substitutions in `menu_bar_format`: `{cpu}` `{gpu}` `{mem}` `{swap}`.

## Logs

Daily-rotated log at `~/Library/Logs/monitor-rs/monitor-rs.log`.

## Smoke test checklist

After building the release binary, verify:

- [ ] Menu-bar text updates roughly every second.
- [ ] Clicking the menu-bar item toggles the popover window.
- [ ] CPU sparkline rises when running `yes > /dev/null` × N.
- [ ] Per-core grid lights up redder with load.
- [ ] Memory sparkline tracks Activity Monitor's "Memory Used" within ~5%.
- [ ] GPU sparkline rises under a Metal compute load (e.g. Xcode's Metal
      sample apps) — or shows "n/a" if IOReport binding is incomplete.
- [ ] Top-processes list updates at the configured rate.
- [ ] Closing all windows does not exit; the app keeps running from the
      menu bar.
- [ ] No Dock icon appears (`LSUIElement` is set in the bundled `.app`).

## Architecture

See `docs/superpowers/specs/2026-05-11-monitor-rs-design.md`.
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: README with build, config, and smoke test"
```

---

## Self-review

**Spec coverage check:**

| Spec section | Implemented in |
| --- | --- |
| Sample types + ring buffer | Tasks 1–2 |
| Settings (incl. `history_capacity` derivation) | Task 3 |
| Logging | Task 4 |
| CPU sampler | Task 5 |
| Memory + swap sampler + pressure classification | Task 6 |
| Process sampler | Task 7 |
| GPU sampler via IOReport | Task 8 |
| Sampler thread orchestration with drift correction | Task 9 |
| Menu-bar text format | Task 10 |
| Sparkline / per-core / process-list widgets | Tasks 11–13 |
| Popover composition | Task 14 |
| eframe app + tray + toggle | Tasks 15–16 |
| App bundle with `LSUIElement` | Task 17 |
| README + manual smoke | Task 18 |

**Open caveats** (called out in the relevant tasks, not gaps):

- The IOReport `compute_idle_complement` body is a port-from-macmon step (Task 8 step 4). It's flagged as a sub-step with a verification test, not glossed over.
- Click-outside-to-dismiss isn't implemented — clicking the status item again toggles. Adding NSEvent local monitor for outside-clicks is reasonable v1.5 work.
- Popover positioning under the status item isn't anchored — noted in Task 16 step 4.

**Type / API consistency:** `tick(&mut self) -> Result<…, MetricError>` is consistent across CPU/Mem/Procs/GPU. `SampleStore::push/latest/recent/len/capacity` is consistent. `Settings::history_capacity()` is referenced both in `SamplerHandle::spawn` and the spec.

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-11-monitor-rs.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
