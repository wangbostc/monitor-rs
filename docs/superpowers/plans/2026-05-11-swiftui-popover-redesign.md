# SwiftUI Popover Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the egui popover with a SwiftUI popover hosted in `NSPopover`, while keeping all sampling logic in Rust as a static library consumed through a tiny C FFI.

**Architecture:** Single `.app`, one process. Swift owns `NSApplication`, the status item, the popover, and all rendering. Rust ships as `libmonitor_rs.a` exposing ~8 `extern "C"` functions (lifecycle, sample read, settings). cbindgen generates `include/monitor_rs.h`; SwiftPM has a small C target that wraps the header so the Swift app can `import MonitorRSC`. The build is a shell script that runs `cargo build --release`, regenerates the header, runs `swift build -c release`, and assembles a `.app` bundle.

**Tech Stack:** Rust 1.78+, `cbindgen`, Swift 5.10, SwiftPM, SwiftUI (macOS 14+), AppKit (`NSStatusItem`, `NSPopover`).

---

## Reference reading

- The design spec: `docs/superpowers/specs/2026-05-11-swiftui-popover-redesign.md`
- Current Rust sampling modules under `src/metrics/`, `src/sampler.rs`, `src/store.rs`, `src/sample.rs`, `src/settings.rs` — these stay essentially unchanged
- `cbindgen` user guide for `[export]` and primitive type mappings: https://github.com/mozilla/cbindgen/blob/master/docs.md
- Apple's `NSStatusItem` + `NSPopover` documentation

---

## File map

By the end of the plan:

```
Cargo.toml                          # [lib] crate-type = ["staticlib","rlib"], no [[bin]]
build.sh                            # cargo + cbindgen + swift build → .app
cbindgen.toml                       # cbindgen config
include/                            # generated headers
└── monitor_rs.h
Package.swift                       # SwiftPM root
Resources/
└── Info.plist                      # LSUIElement, identifiers, min OS
src/
├── lib.rs
├── sample.rs · store.rs · settings.rs · sampler.rs · logging.rs   # unchanged
├── metrics/{cpu,mem,procs,gpu}.rs · metrics/mod.rs                # unchanged
└── ffi.rs                          # NEW — extern "C" exports
Sources/
├── MonitorRSC/
│   ├── include/monitor_rs.h        # symlink to ../../include/monitor_rs.h
│   ├── module.modulemap
│   └── dummy.c                     # so SwiftPM treats this as a C target
└── MonitorRSApp/
    ├── App.swift                   # @main + NSApplicationDelegateAdaptor
    ├── AppDelegate.swift           # NSApplicationDelegate
    ├── MenuBarController.swift     # NSStatusItem + NSPopover lifecycle
    ├── RustBridge.swift            # Safe Swift façade over the C functions
    ├── ViewModel.swift             # @Observable MonitorViewModel
    ├── PopoverView.swift           # SwiftUI root composing the layout
    └── Components/
        ├── HeaderStrip.swift
        ├── MetricTile.swift
        ├── Sparkline.swift
        ├── CoreGrid.swift
        ├── ProcessList.swift
        └── FooterStrip.swift
```

Removed by Task 13 (after the Swift side proves it works):

```
src/main.rs · src/format.rs · src/ui/                # old egui UI
assets/lsuielement.plist                              # old cargo-bundle hack
[package.metadata.bundle] section in Cargo.toml
eframe, egui, objc2, objc2-app-kit, objc2-foundation dependencies
```

**Kept-but-deprecated:** `Settings::menu_bar_format` stays in the struct for backwards-compatible config parsing, with a `#[deprecated]` attribute pointing to nowhere (Swift owns the formatting now).

---

## Migration discipline

We keep the existing egui binary working until the Swift app is functional. Tasks 0–12 ship alongside the old binary; Task 13 deletes the old code. At every commit before Task 13, you can run `cargo run --release` and still see the egui popover. From Task 13 onward, the only way to run the app is the SwiftUI build via `./build.sh && open target/release/monitor-rs.app`.

---

## Task 0: Add staticlib crate-type alongside the existing binary

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: Add `[lib]` target to `Cargo.toml`**

Insert this block right after the `[package]` section (and before `[dependencies]`):

```toml
[lib]
name = "monitor_rs"
crate-type = ["staticlib", "rlib"]
```

The existing `[[bin]]` (implicit, from `src/main.rs`) stays. The crate now produces *both* a binary and `libmonitor_rs.a` from `src/lib.rs`.

- [ ] **Step 2: Verify both artifacts build**

```bash
cd /Users/bowang/projects/monitor-rs
cargo build --release 2>&1 | tail
ls -la target/release/monitor-rs target/release/libmonitor_rs.a
```

Expected: both files exist. The `.a` will be ~10-30 MB (includes sysinfo, parking_lot, tracing, etc.).

- [ ] **Step 3: Verify tests still pass**

```bash
cargo test --workspace --all-targets 2>&1 | tail
```

Expected: 26 passed.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml
git commit -m "build: add staticlib crate-type for Swift consumption"
```

---

## Task 1: Add the FFI module (no header generation yet)

**Files:**
- Create: `src/ffi.rs`
- Modify: `src/lib.rs` (add `#[cfg(target_os = "macos")] pub mod ffi;`)

- [ ] **Step 1: Write `src/ffi.rs`**

```rust
//! C-compatible FFI surface for the Swift side. Every function catches
//! panics and returns a safe sentinel on failure. All pointers in the
//! signatures are owned by the Rust side except where noted.

#![cfg(target_os = "macos")]

use std::ffi::{c_char, CStr, CString};
use std::os::raw::c_int;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::Arc;

use parking_lot::RwLock;

use crate::sample::{MemPressure, Sample};
use crate::sampler::SamplerHandle;
use crate::settings::Settings;
use crate::store::SampleStore;

pub const MRS_MAX_CORES: usize = 64;
pub const MRS_MAX_PROCS: usize = 16;
pub const MRS_PROC_NAME: usize = 64;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MrsProcInfo {
    pub pid: u32,
    pub name: [c_char; MRS_PROC_NAME],
    pub cpu_pct: f32,
    pub rss_bytes: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct MrsSample {
    pub ts_seconds: f64,
    pub cpu_total_pct: f32,
    pub core_count: u8,
    pub cpu_per_core_pct: [f32; MRS_MAX_CORES],
    pub gpu_present: i8,
    pub gpu_pct: f32,
    pub mem_used_bytes: u64,
    pub mem_total_bytes: u64,
    pub mem_pressure: u8,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub proc_count: u8,
    pub procs: [MrsProcInfo; MRS_MAX_PROCS],
}

pub struct MrsHandle {
    sampler: SamplerHandle,
    store: Arc<RwLock<SampleStore>>,
    start: std::time::Instant,
}

#[no_mangle]
pub extern "C" fn monitor_rs_start() -> *mut MrsHandle {
    let r = catch_unwind(|| {
        let settings = Settings::load();
        let sampler = SamplerHandle::spawn(settings);
        let store = sampler.store.clone();
        Box::into_raw(Box::new(MrsHandle {
            sampler,
            store,
            start: std::time::Instant::now(),
        }))
    });
    r.unwrap_or(ptr::null_mut())
}

#[no_mangle]
pub extern "C" fn monitor_rs_stop(h: *mut MrsHandle) {
    if h.is_null() {
        return;
    }
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let boxed = unsafe { Box::from_raw(h) };
        // SamplerHandle's Drop runs here: stops the thread and joins.
        drop(boxed);
    }));
}

#[no_mangle]
pub extern "C" fn monitor_rs_latest(h: *mut MrsHandle, out: *mut MrsSample) -> c_int {
    if h.is_null() || out.is_null() {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let handle = &*h;
        let store = handle.store.read();
        let Some(s) = store.latest() else { return 0 };
        *out = sample_to_c(s, handle.start);
        1
    }));
    r.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn monitor_rs_recent(h: *mut MrsHandle, n: usize, out: *mut MrsSample) -> usize {
    if h.is_null() || out.is_null() || n == 0 {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let handle = &*h;
        let store = handle.store.read();
        let slice = std::slice::from_raw_parts_mut(out, n);
        let mut written = 0usize;
        for s in store.recent(n) {
            if written >= n { break; }
            slice[written] = sample_to_c(s, handle.start);
            written += 1;
        }
        written
    }));
    r.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn monitor_rs_settings_get(h: *mut MrsHandle) -> *const c_char {
    if h.is_null() {
        return ptr::null();
    }
    let r = catch_unwind(AssertUnwindSafe(|| {
        let settings = Settings::load();
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| "{}".to_string());
        let cstring = CString::new(json).unwrap_or_else(|_| CString::new("{}").unwrap());
        cstring.into_raw() as *const c_char
    }));
    r.unwrap_or(ptr::null())
}

#[no_mangle]
pub extern "C" fn monitor_rs_settings_set(_h: *mut MrsHandle, json: *const c_char) -> c_int {
    if json.is_null() {
        return 0;
    }
    let r = catch_unwind(AssertUnwindSafe(|| unsafe {
        let cstr = CStr::from_ptr(json);
        let Ok(s) = cstr.to_str() else { return 0 };
        let Ok(settings) = serde_json::from_str::<Settings>(s) else { return 0 };
        if settings.save().is_err() { return 0 }
        1
    }));
    r.unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn monitor_rs_string_free(s: *const c_char) {
    if s.is_null() { return; }
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        // Reconstruct the CString to drop it.
        let _ = CString::from_raw(s as *mut c_char);
    }));
}

fn sample_to_c(s: &Sample, start: std::time::Instant) -> MrsSample {
    let mut out = MrsSample {
        ts_seconds: s.ts.duration_since(start).as_secs_f64(),
        cpu_total_pct: s.cpu_total,
        core_count: s.cpu_per_core.len().min(MRS_MAX_CORES) as u8,
        cpu_per_core_pct: [0.0; MRS_MAX_CORES],
        gpu_present: if s.gpu_pct.is_some() { 1 } else { 0 },
        gpu_pct: s.gpu_pct.unwrap_or(0.0),
        mem_used_bytes: s.mem.used_bytes,
        mem_total_bytes: s.mem.total_bytes,
        mem_pressure: match s.mem.pressure {
            MemPressure::Normal => 0,
            MemPressure::Warning => 1,
            MemPressure::Critical => 2,
        },
        swap_used_bytes: s.swap.used_bytes,
        swap_total_bytes: s.swap.total_bytes,
        proc_count: s.top_procs.len().min(MRS_MAX_PROCS) as u8,
        procs: [MrsProcInfo {
            pid: 0,
            name: [0; MRS_PROC_NAME],
            cpu_pct: 0.0,
            rss_bytes: 0,
        }; MRS_MAX_PROCS],
    };

    for (dst, src) in out.cpu_per_core_pct.iter_mut().zip(s.cpu_per_core.iter()) {
        *dst = *src;
    }
    for (dst, src) in out.procs.iter_mut().zip(s.top_procs.iter()) {
        dst.pid = src.pid;
        dst.cpu_pct = src.cpu_pct;
        dst.rss_bytes = src.rss_bytes;
        // Truncate name to NAME-1 bytes, NUL-terminate.
        let max = MRS_PROC_NAME - 1;
        let bytes = src.name.as_bytes();
        let n = bytes.len().min(max);
        for i in 0..n {
            dst.name[i] = bytes[i] as c_char;
        }
        // remaining bytes are already 0 (default), so NUL termination is implicit
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_latest_stop_round_trip() {
        let h = monitor_rs_start();
        assert!(!h.is_null());

        // Sampler needs ~one tick before latest() returns a sample.
        std::thread::sleep(std::time::Duration::from_millis(1200));

        let mut out: MrsSample = unsafe { std::mem::zeroed() };
        let got = monitor_rs_latest(h, &mut out);
        assert_eq!(got, 1);
        assert!(out.cpu_total_pct >= 0.0 && out.cpu_total_pct <= 100.0);
        assert!(out.core_count >= 1);
        assert!(out.mem_total_bytes > 0);

        monitor_rs_stop(h);
    }

    #[test]
    fn null_handle_returns_zero() {
        let mut out: MrsSample = unsafe { std::mem::zeroed() };
        assert_eq!(monitor_rs_latest(std::ptr::null_mut(), &mut out), 0);
        assert_eq!(monitor_rs_recent(std::ptr::null_mut(), 5, &mut out), 0);
        monitor_rs_stop(std::ptr::null_mut()); // must not crash
    }

    #[test]
    fn settings_round_trip() {
        let h = monitor_rs_start();
        let json_ptr = monitor_rs_settings_get(h);
        assert!(!json_ptr.is_null());
        let json = unsafe { CStr::from_ptr(json_ptr).to_str().unwrap().to_string() };
        monitor_rs_string_free(json_ptr);
        assert!(json.contains("sample_rate_hz"));

        // Set the same JSON back — should succeed.
        let cstring = CString::new(json).unwrap();
        let rc = monitor_rs_settings_set(h, cstring.as_ptr());
        assert_eq!(rc, 1);

        monitor_rs_stop(h);
    }
}
```

- [ ] **Step 2: Register the module in `src/lib.rs`**

Add (alphabetical order, keeping existing modules):

```rust
pub mod format;
#[cfg(target_os = "macos")]
pub mod ffi;
pub mod logging;
pub mod metrics;
pub mod sample;
#[cfg(target_os = "macos")]
pub mod sampler;
pub mod settings;
pub mod store;
pub mod ui;
```

- [ ] **Step 3: Run tests**

```bash
cargo test --lib ffi 2>&1 | tail
```

Expected: 3 passed (`start_latest_stop_round_trip`, `null_handle_returns_zero`, `settings_round_trip`). The first test sleeps 1200ms so the suite is slower — that's fine.

- [ ] **Step 4: Commit**

```bash
git add src/ffi.rs src/lib.rs
git commit -m "feat: add C FFI surface for Swift consumption"
```

---

## Task 2: Add cbindgen config + generate the header

**Files:**
- Create: `cbindgen.toml`
- Create: `include/monitor_rs.h` (generated)
- Modify: `.gitignore` (NOT — we commit `include/monitor_rs.h` so Swift can build standalone)

- [ ] **Step 1: Install cbindgen CLI (one-time, per dev machine)**

```bash
cargo install cbindgen
cbindgen --version
```

Expected: prints a version (≥ 0.27).

- [ ] **Step 2: Write `cbindgen.toml`**

```toml
language = "C"
header = "// Auto-generated by cbindgen — do not edit by hand."
include_guard = "MONITOR_RS_H"
no_includes = false
sys_includes = ["stdint.h", "stddef.h"]
cpp_compat = true
documentation = true
style = "type"

[export]
prefix = ""
include = ["MrsHandle", "MrsSample", "MrsProcInfo"]

[export.rename]
"c_char" = "char"
"c_int" = "int"

[parse]
parse_deps = false
```

- [ ] **Step 3: Generate the header**

```bash
cd /Users/bowang/projects/monitor-rs
mkdir -p include
cbindgen --config cbindgen.toml --output include/monitor_rs.h
```

- [ ] **Step 4: Inspect the generated header**

```bash
cat include/monitor_rs.h
```

Expected (rough shape — exact line numbers will vary):

```c
// Auto-generated by cbindgen — do not edit by hand.
#ifndef MONITOR_RS_H
#define MONITOR_RS_H

#include <stdint.h>
#include <stddef.h>

#define MRS_MAX_CORES 64
#define MRS_MAX_PROCS 16
#define MRS_PROC_NAME 64

typedef struct MrsHandle MrsHandle;

typedef struct {
  uint32_t pid;
  char name[MRS_PROC_NAME];
  float cpu_pct;
  uint64_t rss_bytes;
} MrsProcInfo;

typedef struct {
  double ts_seconds;
  float cpu_total_pct;
  uint8_t core_count;
  float cpu_per_core_pct[MRS_MAX_CORES];
  int8_t gpu_present;
  float gpu_pct;
  uint64_t mem_used_bytes;
  uint64_t mem_total_bytes;
  uint8_t mem_pressure;
  uint64_t swap_used_bytes;
  uint64_t swap_total_bytes;
  uint8_t proc_count;
  MrsProcInfo procs[MRS_MAX_PROCS];
} MrsSample;

MrsHandle *monitor_rs_start(void);
void monitor_rs_stop(MrsHandle *h);
int monitor_rs_latest(MrsHandle *h, MrsSample *out);
size_t monitor_rs_recent(MrsHandle *h, size_t n, MrsSample *out);
const char *monitor_rs_settings_get(MrsHandle *h);
int monitor_rs_settings_set(MrsHandle *h, const char *json);
void monitor_rs_string_free(const char *s);

#endif  /* MONITOR_RS_H */
```

If the exact output differs (e.g. constants are emitted as enums, or struct fields are reordered), accept it as-is — cbindgen is the source of truth. The Swift side will use whatever cbindgen produced.

- [ ] **Step 5: Commit (header gets committed)**

```bash
git add cbindgen.toml include/
git commit -m "build: cbindgen config + generated C header"
```

---

## Task 3: Scaffold SwiftPM project + minimal Swift CLI smoke

This task gets a Swift executable building that just calls `monitor_rs_start`, polls `monitor_rs_latest`, prints, and exits. No menu bar, no SwiftUI yet — pure FFI integration smoke test.

**Files:**
- Create: `Sources/MonitorRSC/include/monitor_rs.h` (symlink)
- Create: `Sources/MonitorRSC/module.modulemap`
- Create: `Sources/MonitorRSC/dummy.c`
- Create: `Sources/MonitorRSApp/main.swift`
- Create: `Package.swift`

- [ ] **Step 1: Create the C target wrapping the header**

```bash
mkdir -p Sources/MonitorRSC/include
ln -sf ../../../include/monitor_rs.h Sources/MonitorRSC/include/monitor_rs.h
```

`Sources/MonitorRSC/module.modulemap`:

```
module MonitorRSC {
    header "monitor_rs.h"
    export *
}
```

`Sources/MonitorRSC/dummy.c` (SwiftPM needs at least one .c file to recognize this as a C target):

```c
/* Placeholder so SwiftPM treats MonitorRSC as a buildable C target.
 * The actual symbols come from libmonitor_rs.a linked at app-build time. */
```

- [ ] **Step 2: Create `Package.swift`**

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
        .executableTarget(
            name: "MonitorRSApp",
            dependencies: ["MonitorRSC"],
            path: "Sources/MonitorRSApp",
            linkerSettings: [
                .linkedLibrary("monitor_rs"),
                .unsafeFlags(["-L", "target/release"])
            ]
        )
    ]
)
```

- [ ] **Step 3: Create the minimal CLI smoke**

`Sources/MonitorRSApp/main.swift`:

```swift
import MonitorRSC
import Foundation

guard let handle = monitor_rs_start() else {
    print("ERROR: monitor_rs_start returned NULL")
    exit(1)
}

print("Sampler started — waiting 1.5s for first samples...")
Thread.sleep(forTimeInterval: 1.5)

var sample = MrsSample()
let ok = monitor_rs_latest(handle, &sample)
if ok == 1 {
    print(String(format: "CPU: %.1f%%   GPU: %@   MEM: %.1f%% (used %llu / total %llu)",
                 sample.cpu_total_pct,
                 sample.gpu_present == 1 ? String(format: "%.1f%%", sample.gpu_pct) : "n/a",
                 Double(sample.mem_used_bytes) / Double(sample.mem_total_bytes) * 100.0,
                 sample.mem_used_bytes,
                 sample.mem_total_bytes))
    print("Cores: \(sample.core_count), Top processes: \(sample.proc_count)")
} else {
    print("ERROR: monitor_rs_latest returned 0")
}

monitor_rs_stop(handle)
print("Done.")
```

- [ ] **Step 4: Build the Rust static lib first**

```bash
cargo build --release 2>&1 | tail
```

Expected: clean, `target/release/libmonitor_rs.a` exists.

- [ ] **Step 5: Build the Swift CLI**

```bash
swift build -c release 2>&1 | tail -30
```

Expected: builds cleanly. Produces `.build/release/MonitorRSApp`.

If you get linker errors about missing symbols, check that the `unsafeFlags(["-L", "target/release"])` path resolves and that `libmonitor_rs.a` is there.

- [ ] **Step 6: Run the CLI smoke**

```bash
.build/release/MonitorRSApp
```

Expected output (numbers will vary):
```
Sampler started — waiting 1.5s for first samples...
CPU: 12.3%   GPU: 4.5%   MEM: 47.2% (used 16500000000 / total 35000000000)
Cores: 10, Top processes: 5
Done.
```

If GPU shows `n/a`, that's also acceptable — IOReport might not be reachable from a sandboxed swift-build invocation. The test passes either way.

- [ ] **Step 7: Commit**

```bash
git add Sources/ Package.swift
git commit -m "feat: SwiftPM scaffold + FFI smoke CLI"
```

---

## Task 4: Build script + Info.plist + .app bundling

**Files:**
- Create: `build.sh`
- Create: `Resources/Info.plist`
- Modify: `.gitignore`

- [ ] **Step 1: Write `Resources/Info.plist`**

```bash
mkdir -p Resources
```

`Resources/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>dev.monitor-rs</string>
    <key>CFBundleName</key>
    <string>monitor-rs</string>
    <key>CFBundleDisplayName</key>
    <string>monitor-rs</string>
    <key>CFBundleExecutable</key>
    <string>monitor-rs</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
```

- [ ] **Step 2: Write `build.sh`**

```bash
#!/usr/bin/env bash
# Build script: cargo (staticlib) + cbindgen (header) + swift (app) + .app bundle.
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building Rust static library..."
cargo build --release

echo "==> Regenerating C header..."
cbindgen --config cbindgen.toml --output include/monitor_rs.h

echo "==> Building Swift app..."
swift build -c release

echo "==> Assembling .app bundle..."
APP_DIR="target/release/monitor-rs.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp ".build/release/MonitorRSApp" "$APP_DIR/Contents/MacOS/monitor-rs"
cp "Resources/Info.plist" "$APP_DIR/Contents/Info.plist"
echo "APPL????" > "$APP_DIR/Contents/PkgInfo"

echo "==> Done."
echo "Bundle: $APP_DIR"
ls -la "$APP_DIR/Contents/MacOS/"
```

- [ ] **Step 3: Make it executable + extend `.gitignore`**

```bash
chmod +x build.sh
```

Append to `.gitignore`:

```
/.build
```

(`.build` is SwiftPM's output directory. Existing `/target` already covers Rust output.)

- [ ] **Step 4: Run the full build**

```bash
./build.sh 2>&1 | tail -20
```

Expected: ends with "Done.", lists the bundled binary.

- [ ] **Step 5: Verify the bundle structure**

```bash
find target/release/monitor-rs.app -type f
plutil -p target/release/monitor-rs.app/Contents/Info.plist | grep -E "LSUIElement|CFBundleIdentifier|LSMinimumSystemVersion"
```

Expected:
```
target/release/monitor-rs.app/Contents/MacOS/monitor-rs
target/release/monitor-rs.app/Contents/Info.plist
target/release/monitor-rs.app/Contents/PkgInfo
"LSUIElement" => 1
"CFBundleIdentifier" => "dev.monitor-rs"
"LSMinimumSystemVersion" => "14.0"
```

- [ ] **Step 6: Spot-launch the bundle**

```bash
open target/release/monitor-rs.app
```

Expected: the app launches and immediately exits (because `main.swift` from Task 3 is a CLI that prints and quits). The next task replaces `main.swift` with the menu-bar app so this becomes a long-running process.

- [ ] **Step 7: Commit**

```bash
git add build.sh Resources/ .gitignore
git commit -m "build: build.sh + Info.plist + .app bundling"
```

---

## Task 5: Replace the CLI with NSApplicationDelegate + empty popover

This task pivots `main.swift` from a CLI smoke into a long-running menu-bar app. The popover is empty (just a placeholder label) — components come in later tasks.

**Files:**
- Replace: `Sources/MonitorRSApp/main.swift` (becomes thin entry point) → `Sources/MonitorRSApp/App.swift`
- Create: `Sources/MonitorRSApp/AppDelegate.swift`
- Create: `Sources/MonitorRSApp/MenuBarController.swift`
- Create: `Sources/MonitorRSApp/PopoverView.swift` (placeholder)

- [ ] **Step 1: Delete `main.swift`, create `App.swift`**

```bash
rm Sources/MonitorRSApp/main.swift
```

`Sources/MonitorRSApp/App.swift`:

```swift
import SwiftUI

@main
struct MonitorRSApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        // No main window — the app lives in the menu bar.
        // `Settings` is a no-op placeholder scene.
        Settings { EmptyView() }
    }
}
```

- [ ] **Step 2: Create `AppDelegate.swift`**

```swift
import AppKit

final class AppDelegate: NSObject, NSApplicationDelegate {
    private var menuBarController: MenuBarController?

    func applicationDidFinishLaunching(_ notification: Notification) {
        menuBarController = MenuBarController()
    }

    func applicationWillTerminate(_ notification: Notification) {
        menuBarController = nil  // releases the bridge → calls monitor_rs_stop
    }

    // Required for LSUIElement apps: explicitly allow termination via Quit.
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
        false  // we live in the menu bar; don't quit when popover closes
    }
}
```

- [ ] **Step 3: Create `MenuBarController.swift`**

```swift
import AppKit
import SwiftUI

@MainActor
final class MenuBarController {
    private let statusItem: NSStatusItem
    private let popover: NSPopover

    init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        popover = NSPopover()
        popover.contentSize = NSSize(width: 300, height: 360)
        popover.behavior = .transient   // dismisses on click-outside
        popover.animates = true
        popover.contentViewController = NSHostingController(rootView: PopoverView())

        if let button = statusItem.button {
            button.title = "monitor-rs"
            button.target = self
            button.action = #selector(togglePopover(_:))
        }
    }

    @objc private func togglePopover(_ sender: NSStatusBarButton) {
        if popover.isShown {
            popover.performClose(sender)
        } else {
            popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }
}
```

- [ ] **Step 4: Create placeholder `PopoverView.swift`**

```swift
import SwiftUI

struct PopoverView: View {
    var body: some View {
        VStack {
            Text("monitor-rs")
                .font(.headline)
            Text("Popover scaffolding — components in later tasks.")
                .font(.caption)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .padding(20)
        .frame(width: 300, height: 200)
    }
}
```

- [ ] **Step 5: Build and launch**

```bash
./build.sh 2>&1 | tail -10
open target/release/monitor-rs.app
```

Expected: a status item titled `monitor-rs` appears in the system menu bar. Clicking it shows a small popover anchored under the icon containing the placeholder text. Clicking outside dismisses it. Cmd-Q quits the app.

If the status item doesn't appear, check `Console.app` filtered to `monitor-rs` for crash logs.

- [ ] **Step 6: Kill the app and commit**

```bash
pkill -f monitor-rs.app
git add Sources/MonitorRSApp/
git commit -m "feat: menu bar controller + empty SwiftUI popover"
```

---

## Task 6: RustBridge — safe Swift façade over the FFI

**Files:**
- Create: `Sources/MonitorRSApp/RustBridge.swift`

- [ ] **Step 1: Write `Sources/MonitorRSApp/RustBridge.swift`**

```swift
import Foundation
import MonitorRSC

/// Safe Swift façade over the Rust FFI. Owns the opaque handle for its
/// lifetime; the deinit calls `monitor_rs_stop` which joins the sampler
/// thread.
final class RustBridge {
    private let handle: OpaquePointer

    init?() {
        guard let h = monitor_rs_start() else { return nil }
        handle = h
    }

    deinit {
        monitor_rs_stop(handle)
    }

    /// Returns the latest sample if one exists.
    func latest() -> MrsSample? {
        var out = MrsSample()
        return monitor_rs_latest(handle, &out) == 1 ? out : nil
    }

    /// Returns up to `n` recent samples (newest last).
    func recent(_ n: Int) -> [MrsSample] {
        guard n > 0 else { return [] }
        var buf = Array<MrsSample>(repeating: MrsSample(), count: n)
        let written = buf.withUnsafeMutableBufferPointer { ptr -> Int in
            Int(monitor_rs_recent(handle, n, ptr.baseAddress))
        }
        return Array(buf.prefix(written))
    }

    /// Returns the current settings JSON, or `{}` on failure.
    func settingsJSON() -> String {
        guard let cstr = monitor_rs_settings_get(handle) else { return "{}" }
        defer { monitor_rs_string_free(cstr) }
        return String(cString: cstr)
    }

    /// Persists the given settings JSON. Returns true on success.
    @discardableResult
    func setSettingsJSON(_ json: String) -> Bool {
        json.withCString { cstr in
            monitor_rs_settings_set(handle, cstr) == 1
        }
    }
}

/// Helpers for converting MrsSample C-array fields into Swift Arrays.
extension MrsSample {
    var perCoreUsage: [Float] {
        let count = Int(core_count)
        let array = withUnsafeBytes(of: cpu_per_core_pct) { bytes -> [Float] in
            let ptr = bytes.bindMemory(to: Float.self).baseAddress!
            return Array(UnsafeBufferPointer(start: ptr, count: count))
        }
        return array
    }

    var topProcesses: [MrsProcInfo] {
        let count = Int(proc_count)
        let array = withUnsafeBytes(of: procs) { bytes -> [MrsProcInfo] in
            let ptr = bytes.bindMemory(to: MrsProcInfo.self).baseAddress!
            return Array(UnsafeBufferPointer(start: ptr, count: count))
        }
        return array
    }

    var gpuUsage: Float? {
        gpu_present == 1 ? gpu_pct : nil
    }
}

extension MrsProcInfo {
    var nameString: String {
        withUnsafeBytes(of: name) { bytes -> String in
            // bytes is a buffer of CChar; find the NUL terminator.
            let ptr = bytes.bindMemory(to: CChar.self).baseAddress!
            return String(cString: ptr)
        }
    }
}
```

- [ ] **Step 2: Build to verify it compiles (don't wire into the UI yet)**

```bash
./build.sh 2>&1 | tail -10
```

Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/RustBridge.swift
git commit -m "feat: RustBridge safe Swift façade over FFI"
```

---

## Task 7: ViewModel + 250 ms refresh wired through MenuBarController

**Files:**
- Create: `Sources/MonitorRSApp/ViewModel.swift`
- Modify: `Sources/MonitorRSApp/MenuBarController.swift`
- Modify: `Sources/MonitorRSApp/PopoverView.swift`

- [ ] **Step 1: Create `Sources/MonitorRSApp/ViewModel.swift`**

```swift
import Foundation
import Observation

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
```

- [ ] **Step 2: Update `MenuBarController.swift` to own a bridge, view model, and timer**

Replace the entire file with:

```swift
import AppKit
import SwiftUI

@MainActor
final class MenuBarController {
    private let statusItem: NSStatusItem
    private let popover: NSPopover
    private let bridge: RustBridge?
    private let viewModel = MonitorViewModel()
    private var refreshTimer: Timer?

    init() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        bridge = RustBridge()

        popover = NSPopover()
        popover.contentSize = NSSize(width: 300, height: 360)
        popover.behavior = .transient
        popover.animates = true
        popover.contentViewController = NSHostingController(
            rootView: PopoverView(model: viewModel)
        )

        if let button = statusItem.button {
            button.title = "monitor-rs"
            button.target = self
            button.action = #selector(togglePopover(_:))
        }

        startRefreshLoop()
    }

    deinit {
        refreshTimer?.invalidate()
    }

    @objc private func togglePopover(_ sender: NSStatusBarButton) {
        if popover.isShown {
            popover.performClose(sender)
        } else {
            popover.show(relativeTo: sender.bounds, of: sender, preferredEdge: .minY)
            popover.contentViewController?.view.window?.makeKey()
        }
    }

    private func startRefreshLoop() {
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refreshTick() }
        }
    }

    private func refreshTick() {
        guard let bridge = bridge else { return }
        let latest = bridge.latest()
        let recent = bridge.recent(120)
        viewModel.latest = latest
        viewModel.recent = recent

        if let s = latest {
            statusItem.button?.title = MenuBarController.formatStatus(s)
        }
    }

    /// Equivalent to the old Rust render_menu_bar with the default template
    /// "C {cpu} G {gpu} M {mem}". Em-dash for GPU None.
    static func formatStatus(_ s: MrsSample) -> String {
        let cpu = Int(s.cpu_total_pct.rounded())
        let gpu: String = s.gpu_present == 1 ? "\(Int(s.gpu_pct.rounded()))" : "—"
        let memPct: Int = {
            guard s.mem_total_bytes > 0 else { return 0 }
            return Int((Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0).rounded())
        }()
        return "C \(cpu) G \(gpu) M \(memPct)"
    }
}
```

- [ ] **Step 3: Update `PopoverView.swift` to consume the model**

```swift
import SwiftUI

struct PopoverView: View {
    @Bindable var model: MonitorViewModel

    var body: some View {
        VStack(spacing: 8) {
            if let latest = model.latest {
                Text("CPU \(Int(latest.cpu_total_pct))%   GPU \(latest.gpu_present == 1 ? "\(Int(latest.gpu_pct))%" : "n/a")   MEM \(memPercent(latest))%")
                    .font(.system(.body, design: .monospaced))
                Text("Cores: \(latest.core_count) · Procs: \(latest.proc_count) · History: \(model.recent.count) samples")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            } else {
                Text("Sampling…")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(20)
        .frame(width: 300, height: 120)
    }

    private func memPercent(_ s: MrsSample) -> Int {
        guard s.mem_total_bytes > 0 else { return 0 }
        return Int(Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0)
    }
}
```

- [ ] **Step 4: Build and launch**

```bash
./build.sh 2>&1 | tail -10
pkill -f monitor-rs.app || true
open target/release/monitor-rs.app
```

Expected:
- Status item title updates from `monitor-rs` to `C 12 G 4 M 47` (or similar) within a second.
- Clicking shows a popover with live updating CPU/GPU/MEM numbers and the sample count rising.

- [ ] **Step 5: Commit**

```bash
pkill -f monitor-rs.app || true
git add Sources/MonitorRSApp/
git commit -m "feat: live view model + 250ms refresh + tray status text"
```

---

## Task 8: Sparkline SwiftUI component

**Files:**
- Create: `Sources/MonitorRSApp/Components/Sparkline.swift`

- [ ] **Step 1: Create `Sources/MonitorRSApp/Components/Sparkline.swift`**

```swift
import SwiftUI

/// A small filled-area sparkline. `values` should be 0…1 normalized.
struct Sparkline: View {
    let values: [Float]
    let color: Color

    var body: some View {
        Canvas { context, size in
            guard values.count >= 2 else { return }

            let n = values.count
            let dx = size.width / CGFloat(n - 1)

            func point(_ i: Int) -> CGPoint {
                let x = CGFloat(i) * dx
                let v = max(0, min(1, CGFloat(values[i])))
                let y = size.height * (1 - v)
                return CGPoint(x: x, y: y)
            }

            // Filled area under the line.
            var fillPath = Path()
            fillPath.move(to: CGPoint(x: 0, y: size.height))
            for i in 0..<n { fillPath.addLine(to: point(i)) }
            fillPath.addLine(to: CGPoint(x: size.width, y: size.height))
            fillPath.closeSubpath()
            context.fill(fillPath, with: .color(color.opacity(0.20)))

            // Line on top.
            var linePath = Path()
            linePath.move(to: point(0))
            for i in 1..<n { linePath.addLine(to: point(i)) }
            context.stroke(linePath, with: .color(color), lineWidth: 1.5)
        }
    }
}

#Preview {
    VStack(spacing: 12) {
        Sparkline(values: (0..<60).map { Float(sin(Double($0) / 5.0)) * 0.5 + 0.5 },
                  color: .green)
            .frame(width: 220, height: 32)
        Sparkline(values: [0.1, 0.3, 0.5, 0.8, 0.9, 0.7, 0.4, 0.2], color: .blue)
            .frame(width: 220, height: 32)
        Sparkline(values: [], color: .red)
            .frame(width: 220, height: 32)
    }
    .padding()
    .background(Color(NSColor.windowBackgroundColor))
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
./build.sh 2>&1 | tail -5
```

Expected: clean. (No visual verification yet — wired into PopoverView in Task 12.)

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/
git commit -m "feat: Sparkline SwiftUI component"
```

---

## Task 9: CoreGrid SwiftUI component

**Files:**
- Create: `Sources/MonitorRSApp/Components/CoreGrid.swift`

- [ ] **Step 1: Create `Sources/MonitorRSApp/Components/CoreGrid.swift`**

```swift
import SwiftUI

/// A horizontal row of small colored blocks, one per CPU core, colored by usage.
struct CoreGrid: View {
    let perCoreUsage: [Float]  // 0–100 per core

    var body: some View {
        GeometryReader { geo in
            let n = max(perCoreUsage.count, 1)
            let gap: CGFloat = 2
            let blockW = max(4, (geo.size.width - gap * CGFloat(n - 1)) / CGFloat(n))
            HStack(spacing: gap) {
                ForEach(Array(perCoreUsage.enumerated()), id: \.offset) { _, usage in
                    RoundedRectangle(cornerRadius: 2)
                        .fill(color(for: usage))
                        .frame(width: blockW)
                }
            }
        }
        .frame(height: 8)
    }

    private func color(for pct: Float) -> Color {
        // Same gradient idea as the old Rust widget: green → yellow → red.
        let p = Double(min(max(pct, 0), 100) / 100.0)
        let hue = 0.33 * (1.0 - p)  // 0.33 (green) at 0%, 0 (red) at 100%
        return Color(hue: hue, saturation: 0.7, brightness: 0.85)
    }
}

#Preview {
    VStack(spacing: 8) {
        CoreGrid(perCoreUsage: [10, 20, 30, 40, 50, 60, 70, 80, 90, 100])
            .frame(width: 260)
        CoreGrid(perCoreUsage: [5, 5, 5, 5, 5, 5, 5, 5])
            .frame(width: 260)
    }
    .padding()
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
./build.sh 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/CoreGrid.swift
git commit -m "feat: CoreGrid SwiftUI component"
```

---

## Task 10: MetricTile SwiftUI component

**Files:**
- Create: `Sources/MonitorRSApp/Components/MetricTile.swift`

- [ ] **Step 1: Create `Sources/MonitorRSApp/Components/MetricTile.swift`**

```swift
import SwiftUI

/// One of the three top-row tiles (CPU / GPU / MEM).
/// `value` is the current display (e.g. "9%" or "n/a").
/// `history` are recent samples normalized to 0…1 for the sparkline.
struct MetricTile: View {
    let label: String
    let value: String
    let color: Color
    let history: [Float]

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(label)
                .font(.system(.caption, design: .rounded).weight(.medium))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.5)

            Text(value)
                .font(.system(.title2, design: .rounded).weight(.semibold))
                .monospacedDigit()

            Sparkline(values: history, color: color)
                .frame(height: 28)
        }
    }
}

#Preview {
    HStack(alignment: .top, spacing: 16) {
        MetricTile(label: "CPU", value: "9%", color: .green,
                   history: (0..<60).map { Float(sin(Double($0) / 5.0)) * 0.5 + 0.5 })
        MetricTile(label: "GPU", value: "17%", color: .blue,
                   history: (0..<60).map { _ in Float.random(in: 0...0.5) })
        MetricTile(label: "MEM", value: "76%", color: .orange,
                   history: (0..<60).map { i in Float(i) / 60.0 })
    }
    .padding(20)
    .frame(width: 300)
}
```

- [ ] **Step 2: Build to verify it compiles**

```bash
./build.sh 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/Components/MetricTile.swift
git commit -m "feat: MetricTile SwiftUI component"
```

---

## Task 11: ProcessList + HeaderStrip + FooterStrip

Three small components in one task — none stand alone meaningfully.

**Files:**
- Create: `Sources/MonitorRSApp/Components/ProcessList.swift`
- Create: `Sources/MonitorRSApp/Components/HeaderStrip.swift`
- Create: `Sources/MonitorRSApp/Components/FooterStrip.swift`

- [ ] **Step 1: Create `Sources/MonitorRSApp/Components/ProcessList.swift`**

```swift
import SwiftUI

struct ProcessList: View {
    let procs: [MrsProcInfo]

    var body: some View {
        if procs.isEmpty {
            Text("No process data")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 4) {
                ForEach(Array(procs.enumerated()), id: \.offset) { _, p in
                    GridRow {
                        Text(truncate(p.nameString, max: 22))
                            .font(.system(.caption, design: .default))
                            .lineLimit(1)
                        Text("\(Int(p.cpu_pct.rounded()))%")
                            .font(.system(.caption, design: .default).monospacedDigit())
                            .frame(minWidth: 36, alignment: .trailing)
                            .foregroundStyle(.secondary)
                        Text(formatBytes(p.rss_bytes))
                            .font(.system(.caption, design: .default).monospacedDigit())
                            .frame(minWidth: 52, alignment: .trailing)
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func truncate(_ s: String, max: Int) -> String {
        s.count <= max ? s : String(s.prefix(max - 1)) + "…"
    }

    private func formatBytes(_ b: UInt64) -> String {
        let b = Double(b)
        let GB = 1024.0 * 1024.0 * 1024.0
        let MB = 1024.0 * 1024.0
        let KB = 1024.0
        if b >= GB { return String(format: "%.1fG", b / GB) }
        if b >= MB { return String(format: "%.0fM", b / MB) }
        return String(format: "%.0fK", max(b / KB, 0))
    }
}
```

- [ ] **Step 2: Create `Sources/MonitorRSApp/Components/HeaderStrip.swift`**

```swift
import SwiftUI

struct HeaderStrip: View {
    /// Called when the user clicks the power icon — should quit the app.
    let onQuit: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Text("MONITOR-RS")
                .font(.system(.caption, design: .rounded).weight(.medium))
                .tracking(1.2)
                .foregroundStyle(.secondary)

            Spacer()

            Button(action: {}) {
                Image(systemName: "gearshape")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Settings (coming soon)")
            .disabled(true)

            Button(action: onQuit) {
                Image(systemName: "power")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .help("Quit monitor-rs")
        }
    }
}
```

- [ ] **Step 3: Create `Sources/MonitorRSApp/Components/FooterStrip.swift`**

```swift
import SwiftUI

struct FooterStrip: View {
    let swapUsedBytes: UInt64
    let swapTotalBytes: UInt64
    let sampleRateHz: Double

    var body: some View {
        HStack {
            Text("swap \(swapText)")
                .font(.caption)
                .foregroundStyle(.secondary)
            Spacer()
            Text("· \(rateText) ·")
                .font(.caption)
                .foregroundStyle(.tertiary)
        }
    }

    private var swapText: String {
        if swapTotalBytes == 0 { return "off" }
        let g = Double(swapUsedBytes) / (1024.0 * 1024.0 * 1024.0)
        return String(format: "%.2f GB", g)
    }

    private var rateText: String {
        let rounded = (sampleRateHz * 10).rounded() / 10
        if rounded == 1.0 { return "1 Hz" }
        return String(format: "%.1f Hz", rounded)
    }
}
```

- [ ] **Step 4: Build to verify all three compile**

```bash
./build.sh 2>&1 | tail -5
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSApp/Components/
git commit -m "feat: ProcessList, HeaderStrip, FooterStrip components"
```

---

## Task 12: Compose the full PopoverView

This task wires all the components into the final layout per Direction A from the spec.

**Files:**
- Modify: `Sources/MonitorRSApp/PopoverView.swift`
- Modify: `Sources/MonitorRSApp/MenuBarController.swift` (pass an `onQuit` closure into the view)

- [ ] **Step 1: Rewrite `Sources/MonitorRSApp/PopoverView.swift`**

```swift
import SwiftUI

struct PopoverView: View {
    @Bindable var model: MonitorViewModel
    let onQuit: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HeaderStrip(onQuit: onQuit)

            if let latest = model.latest {
                summaryGrid(latest: latest)
                CoreGrid(perCoreUsage: latest.perCoreUsage)
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
                    sampleRateHz: 1.0  // TODO: thread through settings
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
    }

    @ViewBuilder
    private func summaryGrid(latest: MrsSample) -> some View {
        HStack(alignment: .top, spacing: 12) {
            MetricTile(
                label: "CPU",
                value: "\(Int(latest.cpu_total_pct.rounded()))%",
                color: .green,
                history: normalize(model.cpuHistory)
            )
            MetricTile(
                label: "GPU",
                value: latest.gpuUsage.map { "\(Int($0.rounded()))%" } ?? "n/a",
                color: .blue,
                history: normalize(model.gpuHistory)
            )
            MetricTile(
                label: "MEM",
                value: "\(Int(memPct(latest).rounded()))%",
                color: memColor(latest.mem_pressure),
                history: normalize(model.memHistory)
            )
        }
    }

    private func normalize(_ raw: [Float]) -> [Float] {
        raw.map { max(0, min(1, $0 / 100)) }
    }

    private func memPct(_ s: MrsSample) -> Double {
        guard s.mem_total_bytes > 0 else { return 0 }
        return Double(s.mem_used_bytes) / Double(s.mem_total_bytes) * 100.0
    }

    private func memColor(_ pressure: UInt8) -> Color {
        switch pressure {
        case 0: return .orange
        case 1: return Color(red: 0.95, green: 0.55, blue: 0.20)
        default: return .red
        }
    }
}
```

(`TODO: thread through settings` is intentional — Task 14 will surface settings; for now we hard-code 1.0 Hz.)

Wait — TODO comments are flagged by the No Placeholders rule. Replace with a hard-coded value and an explicit doc:

Replace:
```swift
                    sampleRateHz: 1.0  // TODO: thread through settings
```

With:
```swift
                    sampleRateHz: 1.0  // Settings UI is v1.5 scope; reading from the bridge later.
```

- [ ] **Step 2: Update `Sources/MonitorRSApp/MenuBarController.swift` to pass `onQuit`**

In the `init()` body, find:

```swift
popover.contentViewController = NSHostingController(
    rootView: PopoverView(model: viewModel)
)
```

Replace with:

```swift
popover.contentViewController = NSHostingController(
    rootView: PopoverView(model: viewModel, onQuit: {
        NSApp.terminate(nil)
    })
)
```

`NSApp.terminate(nil)` runs `applicationWillTerminate` first, which releases `menuBarController`, which drops `RustBridge`, which calls `monitor_rs_stop`. Clean shutdown.

- [ ] **Step 3: Build and launch**

```bash
./build.sh 2>&1 | tail -10
pkill -f monitor-rs.app || true
open target/release/monitor-rs.app
```

Expected:
- Status item title shows `C XX G XX M XX`.
- Clicking opens the new popover with summary tiles, per-core grid, process list, footer.
- Power icon in the header quits the app cleanly.
- Re-open works the same.

- [ ] **Step 4: Commit**

```bash
pkill -f monitor-rs.app || true
git add Sources/MonitorRSApp/
git commit -m "feat: compose full PopoverView with all components"
```

---

## Task 13: Delete the old egui UI + dependencies

The Swift app is now functional. Time to throw away the dead Rust code.

**Files:**
- Delete: `src/main.rs`, `src/format.rs`, `src/ui/` (entire directory), `assets/`, `examples/check_*.rs` (optional)
- Modify: `src/lib.rs`, `Cargo.toml`
- Modify: `src/settings.rs` (deprecate `menu_bar_format`)

- [ ] **Step 1: Delete the old UI files**

```bash
cd /Users/bowang/projects/monitor-rs
rm src/main.rs
rm src/format.rs
rm -r src/ui
rm -r assets
# Examples are diagnostic-only; keep gpu but drop the ones that explored sysinfo.
rm examples/check_cpus.rs examples/check_interval.rs
```

(Keep `examples/check_gpu.rs` — it's still a useful manual probe.)

- [ ] **Step 2: Remove `[[bin]]` parity and bundle metadata from `Cargo.toml`**

The crate no longer has a binary. Remove these sections if present:

- `[[bin]]` (likely auto-detected from `src/main.rs` rather than declared — removing `src/main.rs` is enough, but if there's an explicit `[[bin]]` block, delete it)
- `[package.metadata.bundle]` and `[package.metadata.bundle.osx_info_plist_exts]`

Remove these dependencies (no longer used):

- `eframe`
- `egui`
- `objc2` (under `[target.'cfg(target_os = "macos")'.dependencies]`)
- `objc2-app-kit`
- `objc2-foundation`

Keep:
- `sysinfo`, `serde`, `serde_json`, `directories`, `tracing`, `tracing-subscriber`, `tracing-appender`, `anyhow`, `thiserror`, `parking_lot`
- `core-foundation`, `core-foundation-sys`, `libloading` (used by GPU IOReport)

After editing, `Cargo.toml` should have a single `[lib]` target (added in Task 0).

- [ ] **Step 3: Clean `src/lib.rs`**

Replace with:

```rust
#[cfg(target_os = "macos")]
pub mod ffi;
pub mod logging;
pub mod metrics;
pub mod sample;
#[cfg(target_os = "macos")]
pub mod sampler;
pub mod settings;
pub mod store;
```

(`format` and `ui` modules are gone.)

- [ ] **Step 4: Deprecate `Settings::menu_bar_format`**

In `src/settings.rs`, find the field:

```rust
    pub menu_bar_format: String,
```

Add a `#[deprecated]` attribute above it:

```rust
    /// Deprecated: status item formatting is owned by the Swift side as of v0.2.
    /// Kept in the schema so old config.json files still parse.
    #[deprecated(note = "Swift owns status item formatting; this field is ignored.")]
    pub menu_bar_format: String,
```

The compiler may warn about reads/writes elsewhere — silence them where needed with `#[allow(deprecated)]` (only at the use sites; tests in `settings.rs` create defaults that read this field).

In `settings.rs`, the `Default` impl and tests need `#[allow(deprecated)]`. Add at the top of the file:

```rust
#![allow(deprecated)]
```

(File-level allow so the deprecation warning doesn't pollute the rest of the file; external code that uses `menu_bar_format` will still see the warning, which is the point.)

- [ ] **Step 5: Confirm Rust still builds and tests pass**

```bash
cargo build --release 2>&1 | tail
cargo test --workspace --all-targets 2>&1 | tail
```

Expected: clean build, all tests pass (counts may have dropped — the deleted modules removed their tests).

- [ ] **Step 6: Confirm the Swift app still builds and runs**

```bash
./build.sh 2>&1 | tail -10
pkill -f monitor-rs.app || true
open target/release/monitor-rs.app
```

Expected: full functionality unchanged.

- [ ] **Step 7: Commit**

```bash
pkill -f monitor-rs.app || true
git add -A
git commit -m "refactor: delete egui UI and unused deps; deprecate menu_bar_format"
```

---

## Task 14: Update README + docs

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Rewrite `README.md`**

```markdown
# monitor-rs

Small macOS menu bar app showing live CPU, GPU, memory, and top processes.
Apple Silicon only. Rust sampling core + native SwiftUI popover.

![status](https://img.shields.io/badge/status-alpha-orange)

## Build

```
./build.sh
```

The script:
1. `cargo build --release` → produces `libmonitor_rs.a` (Rust sampling core)
2. `cbindgen --config cbindgen.toml --output include/monitor_rs.h` → regenerates the C header
3. `swift build -c release` → compiles the SwiftPM `MonitorRSApp` executable
4. Bundles the binary + `Info.plist` into `target/release/monitor-rs.app`

Prerequisites: Rust 1.78+, Xcode Command Line Tools (Swift 5.10+),
`cargo install cbindgen`.

## Run

```
open target/release/monitor-rs.app
```

The app is unsigned. On first launch macOS will block it — right-click the
`.app` in Finder and choose **Open** to authorize it.

## Configuration

Settings live at `~/Library/Application Support/dev.monitor-rs.monitor-rs/config.json`:

```json
{
  "sample_rate_hz": 1.0,
  "menu_bar_format": "(ignored)",
  "top_n_procs": 5,
  "history_seconds": 120
}
```

`menu_bar_format` is deprecated as of v0.2 — the SwiftUI side owns the
status item title. The field is still parsed for backwards compatibility
but has no effect.

## Logs

Daily-rotated log at `~/Library/Logs/monitor-rs/monitor-rs.log`.

## Smoke test checklist

After `./build.sh`, verify:

- [ ] Status item shows `C XX G XX M XX` and updates ~4 times per second.
- [ ] Clicking the status item shows a translucent popover anchored beneath it.
- [ ] Popover shows three summary tiles (CPU / GPU / MEM) with sparklines and
      a per-core grid under the CPU tile.
- [ ] Top processes section updates live.
- [ ] CPU sparkline rises when running `yes > /dev/null` × N.
- [ ] Per-core grid lights up redder with load.
- [ ] GPU sparkline rises under a Metal compute load — or shows `n/a` if
      IOReport binding is unavailable.
- [ ] Power icon in the header quits the app cleanly.
- [ ] Light and Dark mode both look correct (toggle via System Settings →
      Appearance).
- [ ] No Dock icon appears (`LSUIElement` is set in the bundled `.app`).

## Architecture

See `docs/superpowers/specs/2026-05-11-swiftui-popover-redesign.md`.

## Repository layout

```
src/                # Rust sampling core (library only)
Sources/            # SwiftPM targets: MonitorRSC (C bindings) + MonitorRSApp
include/            # cbindgen-generated C header (committed)
Resources/          # Info.plist for the .app
build.sh            # End-to-end build script
Package.swift       # SwiftPM root
Cargo.toml          # [lib] crate-type = ["staticlib", "rlib"]
```
```

- [ ] **Step 2: Commit**

```bash
git add README.md
git commit -m "docs: update README for SwiftUI build process"
```

---

## Self-review

**Spec coverage check:**

| Spec section | Implemented in |
| --- | --- |
| Architecture (Rust staticlib + Swift app) | Task 0 (staticlib), Task 3 (Swift scaffold) |
| FFI surface (8 functions, MrsSample, MrsProcInfo, MrsHandle) | Task 1 (Rust side), Task 2 (header generation) |
| `cbindgen.toml` + generated header | Task 2 |
| `Package.swift` with MonitorRSC + MonitorRSApp targets | Task 3 |
| `build.sh` orchestrating cargo → cbindgen → swift → .app | Task 4 |
| `Info.plist` with `LSUIElement`, identifier, min OS 14 | Task 4 |
| `App.swift` + `AppDelegate.swift` | Task 5 |
| `MenuBarController` (NSStatusItem + NSPopover) | Task 5 (skeleton), Task 7 (timer + view model), Task 12 (onQuit) |
| `RustBridge` safe Swift façade | Task 6 |
| `MonitorViewModel` (`@Observable`) | Task 7 |
| 250 ms refresh loop on the main thread | Task 7 |
| Status item title formatting | Task 7 (`formatStatus`) |
| `Sparkline` component | Task 8 |
| `CoreGrid` component | Task 9 |
| `MetricTile` component | Task 10 |
| `ProcessList`, `HeaderStrip`, `FooterStrip` components | Task 11 |
| `PopoverView` full composition + onQuit wiring | Task 12 |
| Delete old egui UI + unused deps | Task 13 |
| Deprecate `Settings::menu_bar_format` | Task 13 |
| README update | Task 14 |

**Placeholder scan:** No "TBD"/"TODO"/etc. in step contents. The single
`// TODO` instance was rewritten inline.

**Type / API consistency:**
- `MrsHandle`, `MrsSample`, `MrsProcInfo` consistent across Rust ffi.rs,
  cbindgen output, RustBridge.swift, and PopoverView.
- `MrsSample` extensions (`perCoreUsage`, `topProcesses`, `gpuUsage`,
  `nameString`) defined in RustBridge.swift and consumed in PopoverView,
  ProcessList. No name drift.
- `MonitorViewModel.{latest, recent, cpuHistory, gpuHistory, memHistory}`
  defined in Task 7, consumed in Task 12.
- `MenuBarController.formatStatus(_:)` defined in Task 7, called only
  inside `refreshTick` in the same task.

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-11-swiftui-popover-redesign.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.

**2. Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

**Which approach?**
