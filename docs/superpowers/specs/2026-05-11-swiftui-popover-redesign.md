# monitor-rs — SwiftUI Popover Redesign

Replace the egui popover with a truly native SwiftUI popover hosted in an
`NSPopover`, while keeping all sampling logic in Rust. The Rust crate becomes
a static library (`libmonitor_rs.a`) consumed via a tiny C FFI from a Swift
executable that owns the menu bar and the UI.

## Goals

- **Apple-native look and feel.** Real `NSVisualEffectView` vibrancy (free
  with `NSPopover`), real SF Pro rendering via CoreText, real Mac controls,
  automatic Light/Dark theming via SwiftUI `ColorScheme`.
- **Reuse the Rust sampling work.** All metric modules, the sampler thread,
  the ring buffer, settings — all stay. Only the UI layer changes.
- **Single `.app`, one process.** Swift drives `NSApplication`; Rust is
  linked in as a static library. No IPC.
- **Layout per Direction A** (summary tiles + macOS HUD) — three-column
  CPU/GPU/MEM grid at top, per-core grid under the CPU tile, process list,
  footer with swap + sample rate. Icon-only gear and power controls in the
  header.

## Non-goals (v1.5)

- Functional Settings panel (gear is a stub).
- Sample-rate adjustment from the UI.
- Touch Bar / VoiceOver accessibility (later).
- Custom font bundling — rely on system SF Pro.
- Popover anchoring under the status item is now **free** via `NSPopover`;
  no manual frame math required.

## Architecture

```
┌─────────────────────────────────────────────┐
│  Swift binary (monitor-rs.app)              │
│   • NSApplication, NSStatusItem             │
│   • NSPopover hosting SwiftUI PopoverView   │
│   • Light/Dark via SwiftUI ColorScheme      │
│   • Periodic main-thread poll of the store  │
└──────────────────────▲──────────────────────┘
                       │  extern "C"
┌──────────────────────┴──────────────────────┐
│  libmonitor_rs.a (Rust static library)      │
│   • Sampler thread (unchanged)              │
│   • SampleStore ring buffer (unchanged)     │
│   • cpu, mem, procs, gpu modules            │
│   • Settings (unchanged)                    │
│   • ffi.rs — C-compatible exports           │
└─────────────────────────────────────────────┘
```

Both halves link into a single Swift-driven executable. No subprocesses,
no sockets, no shared memory. The status item, the popover, and the
periodic UI poll all run on the Swift main thread; the sampler runs in a
dedicated Rust thread spawned via the FFI bootstrap call.

## FFI surface

Hand-written, intentionally small. Generated as `include/monitor_rs.h` by
`cbindgen` and committed to the repo so the Swift side has a stable header
without needing `cargo` at Swift-build time.

```c
// monitor_rs.h
#include <stdint.h>
#include <stddef.h>

#define MRS_MAX_CORES 64
#define MRS_MAX_PROCS 16
#define MRS_PROC_NAME 64

typedef struct MrsHandle MrsHandle;

typedef struct {
    uint32_t pid;
    char     name[MRS_PROC_NAME];
    float    cpu_pct;
    uint64_t rss_bytes;
} MrsProcInfo;

typedef struct {
    double   ts_seconds;          // monotonic seconds since handle start
    float    cpu_total_pct;
    uint8_t  core_count;
    float    cpu_per_core_pct[MRS_MAX_CORES];
    int8_t   gpu_present;         // 1 if gpu_pct is meaningful
    float    gpu_pct;
    uint64_t mem_used_bytes;
    uint64_t mem_total_bytes;
    uint8_t  mem_pressure;        // 0=Normal, 1=Warning, 2=Critical
    uint64_t swap_used_bytes;
    uint64_t swap_total_bytes;
    uint8_t  proc_count;
    MrsProcInfo procs[MRS_MAX_PROCS];
} MrsSample;

// Lifecycle
MrsHandle* monitor_rs_start(void);
void       monitor_rs_stop(MrsHandle*);

// Sampling
// Returns 1 if *out was populated, 0 if the store has no samples yet.
int    monitor_rs_latest(MrsHandle*, MrsSample* out);
// Writes up to `n` recent samples into `out`. Returns the count written.
size_t monitor_rs_recent(MrsHandle*, size_t n, MrsSample* out);

// Settings round-trip. JSON strings are allocated by the callee; free with
// monitor_rs_string_free.
const char* monitor_rs_settings_get(MrsHandle*);
int         monitor_rs_settings_set(MrsHandle*, const char* json);
void        monitor_rs_string_free(const char*);
```

**Why fixed-size arrays** instead of out-pointers for variable-length data?
A `MrsSample` is ~1 KB. The whole point is a single memcpy across the FFI
boundary on each poll — no allocation, no callbacks, no error paths. The
caps (`64` cores, `16` procs, `64`-byte names) are generous for the
foreseeable hardware and the v1 settings (`top_n_procs <= 16`).

## Project layout

```
monitor-rs/
├── Cargo.toml                      # [lib] crate-type = ["staticlib", "rlib"]
├── cbindgen.toml                   # generates include/monitor_rs.h
├── Package.swift                   # SwiftPM root, .executableTarget
├── build.sh                        # cargo build → cbindgen → swift build → .app
├── src/                            # Rust (mostly unchanged)
│   ├── lib.rs
│   ├── sample.rs · store.rs · settings.rs · sampler.rs
│   ├── metrics/{cpu,mem,procs,gpu}.rs · metrics/mod.rs
│   └── ffi.rs                      # NEW — extern "C" wrappers
├── include/
│   └── monitor_rs.h                # cbindgen output, committed
├── Sources/
│   └── MonitorRSApp/
│       ├── App.swift               # @main + NSApplicationDelegateAdaptor
│       ├── MenuBarController.swift # NSStatusItem + NSPopover wiring
│       ├── RustBridge.swift        # Safe Swift façade over the C functions
│       ├── PopoverView.swift       # SwiftUI root
│       └── Components/
│           ├── MetricTile.swift
│           ├── Sparkline.swift     # Path-based, .canvas
│           ├── CoreGrid.swift
│           └── ProcessList.swift
├── Resources/
│   └── Info.plist                  # LSUIElement, bundle identifiers
└── docs/                           # existing
```

Files removed: `src/main.rs`, `src/format.rs`, `src/ui/` (entire directory).
Cargo dependencies removed: `eframe`, `egui`, `objc2`, `objc2-app-kit`,
`objc2-foundation`. The Rust crate slims down to its sampling core.

## Rust changes

### `Cargo.toml`

```toml
[lib]
name = "monitor_rs"
crate-type = ["staticlib", "rlib"]   # staticlib for Swift, rlib for tests
```

The `[[bin]]` target goes away (Swift owns the executable now). Existing
Rust tests keep working because `rlib` is still there.

### `src/lib.rs`

Add `pub mod ffi;` (gated `#[cfg(target_os = "macos")]`). Existing module
graph (sample, store, settings, sampler, metrics) is unchanged.

### `src/ffi.rs` (new)

A thin layer that:
- Owns the `SamplerHandle` through an opaque heap pointer (`Box<...>` →
  `*mut MrsHandle`).
- Copies `Sample` → `MrsSample` on every read (truncating per-core and
  per-process arrays to the C caps).
- Catches panics at every entry point via `catch_unwind` and returns a
  safe value (`null`/`0`/`-1`) on panic.
- Serializes `Settings` to/from JSON for the get/set pair.

Every `pub extern "C"` function asserts non-null inputs and is `unsafe` in
spirit but `extern "C"` in declaration. The C header documents the
preconditions.

### `cbindgen.toml`

```toml
language = "C"
header = "// Auto-generated by cbindgen — do not edit."
include_guard = "MONITOR_RS_H"
no_includes = false
sys_includes = ["stdint.h", "stddef.h"]
cpp_compat = true

[export]
prefix = "Mrs"
```

`cbindgen --config cbindgen.toml --output include/monitor_rs.h` is wired
into `build.sh` and runs on every build. The header is also committed so
Swift can build standalone.

## Swift side

### `Package.swift`

```swift
// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "MonitorRSApp",
    platforms: [.macOS(.v14)],
    targets: [
        // C interop target wrapping the cbindgen-generated header.
        .target(
            name: "MonitorRSC",
            path: "Sources/MonitorRSC",
            publicHeadersPath: "include"
        ),
        // Swift app target depending on the C bindings + the Rust staticlib.
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

`Sources/MonitorRSC/` contains:
- `include/monitor_rs.h` (the cbindgen-generated header, copied/symlinked
  from the repo-root `include/` so SwiftPM treats it as the C target's
  public header)
- `module.modulemap` declaring `module MonitorRSC { header "monitor_rs.h"
  export * }`
- An empty `dummy.c` so SwiftPM recognizes this as a C target

The Swift target imports it idiomatically: `import MonitorRSC`. The
linker pulls in `libmonitor_rs.a` from `target/release/`. No
`.unsafeFlags(["-import-objc-header", ...])` hack, no bridging header.

**Platform**: macOS 14 (`@Observable`, `Bindable`, `Grid` all need 14+;
bumping from the earlier `.v13` keeps the modern SwiftUI surface clean
and the implementation small).

### `App.swift`

```swift
@main
struct MonitorRSApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate
    var body: some Scene { Settings { EmptyView() } } // no main window
}
```

`Settings { EmptyView() }` registers a scene without showing a window — the
app lives entirely in the status item. The `LSUIElement` Info.plist key
keeps the Dock icon away.

### `MenuBarController` (`AppDelegate`)

- On `applicationDidFinishLaunching`:
  - Call `monitor_rs_start()` → store the `OpaquePointer`.
  - Create `NSStatusItem` with `variableLength`.
  - Set the button title to a placeholder, attach a click handler.
  - Create the `NSPopover` with `contentSize = CGSize(width: 300, height: 360)`,
    `behavior = .transient`, `contentViewController = NSHostingController(rootView: PopoverView(...))`.
  - Start a `Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true)` that:
    - Calls `RustBridge.latest()` → updates an `@Observable` snapshot
      bound into the SwiftUI view tree.
    - Reformats the status item button title (`C 9 G 17 M 76`-style).
- On click: toggle `popover.show(relativeTo:of:preferredEdge:)`.
- On `applicationWillTerminate`: call `monitor_rs_stop()`.

The status item title is formatted in Swift as `"C \(cpu) G \(gpu) M \(mem)"`
(integers, em-dash for GPU None) — equivalent to the old
`format::render_menu_bar` template. We don't carry the template-string
config over to v1.5; if the user wants a custom format later, it goes
into Settings.

### `RustBridge.swift`

A safe Swift façade:

```swift
final class RustBridge {
    private let handle: OpaquePointer

    init?() {
        guard let h = monitor_rs_start() else { return nil }
        handle = h
    }

    deinit { monitor_rs_stop(handle) }

    func latest() -> MrsSample? {
        var out = MrsSample()
        let ok = monitor_rs_latest(handle, &out)
        return ok == 1 ? out : nil
    }

    func recent(_ n: Int) -> [MrsSample] {
        var buf = [MrsSample](repeating: MrsSample(), count: n)
        let written = monitor_rs_recent(handle, n, &buf)
        return Array(buf.prefix(Int(written)))
    }

    func settingsJSON() -> String { ... }
    func setSettings(json: String) -> Bool { ... }
}
```

Swift owns the lifetime; Rust is responsible only for what's behind the
opaque handle.

### `PopoverView` (SwiftUI)

```swift
struct PopoverView: View {
    @Bindable var model: MonitorViewModel

    var body: some View {
        VStack(spacing: 12) {
            HeaderStrip()                              // MONITOR-RS + gear + power
            MetricSummaryGrid(snapshot: model.snapshot,
                              recent: model.recent)    // 3 tiles + per-core
            Divider()
            ProcessList(procs: model.snapshot.procs)
            Divider()
            FooterStrip(swap: model.snapshot.swap,
                        rate: model.settings.sampleRateHz)
        }
        .padding(12)
        .frame(width: 300)
    }
}
```

All subviews use SwiftUI primitives. `Sparkline` draws via `Canvas` with a
`Path` of `min(60, history_seconds)` points normalized to 0…1, plus a
filled area at 25 % alpha — identical visual semantics to the existing egui
widget, just rendered through CoreGraphics. `CoreGrid` is an `HStack` of
`RoundedRectangle(cornerRadius: 2)`s with per-core color via
`Color(hue: 0.33 - 0.33 * pct, saturation: 0.7, brightness: 0.85)`.

`ProcessList` is a `Grid` (SwiftUI's `Grid`, not `LazyVGrid`) with three
columns and `.monospacedDigit()` on the numeric columns for clean alignment.

System colors used (all SwiftUI `Color`):

- CPU: `Color.green`
- GPU: `Color.blue`
- MEM (Normal): `Color.orange`
- MEM (Warning): `Color(red: 0.95, green: 0.55, blue: 0.20)` (deeper orange)
- MEM (Critical): `Color.red`

(Distinct color values per pressure level rather than opacity tricks so
the level is unambiguous in both Light and Dark mode.)

Light/Dark is automatic — SwiftUI inherits the system's `ColorScheme` and
all of the above resolve correctly in both modes.

## Build & distribution

`build.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo build --release
cbindgen --config cbindgen.toml --output include/monitor_rs.h
swift build -c release

# Wrap the SwiftPM binary into a proper .app
APP="target/release/monitor-rs.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp ".build/release/MonitorRSApp" "$APP/Contents/MacOS/monitor-rs"
cp "Resources/Info.plist" "$APP/Contents/Info.plist"

echo "Built $APP"
```

`Info.plist` sets `LSUIElement = true` (no Dock icon),
`CFBundleIdentifier = dev.monitor-rs`, `CFBundleExecutable = monitor-rs`,
`LSMinimumSystemVersion = 13.0` (SwiftUI APIs we use), and
`NSHighResolutionCapable = true`.

The previous `cargo bundle` integration is removed (`[package.metadata.bundle]`
and `assets/lsuielement.plist` deleted).

## Threading model

| Thread | Responsibility |
|---|---|
| Rust sampler thread | Tick every `1 / sample_rate_hz` s, write into `SampleStore`. Same as today. |
| Swift main thread | UI rendering, the 250 ms poll timer, FFI calls. |
| (Cocoa internal threads) | Not touched by us. |

FFI calls are short (single struct memcpy under a read lock) and always
happen on the Swift main thread, so we don't need to make the FFI
re-entrant. The `RwLock` inside `SampleStore` already protects reads.

## Risks

- **Mixed-language build complexity.** `build.sh` is ad-hoc; we don't get
  IDE integration on the Rust side from Xcode or on the Swift side from
  Cargo. Mitigation: keep the script small and well-commented; if it grows
  beyond ~50 lines, revisit Xcode integration as a follow-up.
- **`cbindgen` drift.** The committed `include/monitor_rs.h` could diverge
  from the actual extern surface. Mitigation: `build.sh` regenerates it
  every build and any diff after a build fails CI (added when CI exists).
- **SwiftUI/AppKit version drift.** We target macOS 13. The plan deliberately
  uses only well-established SwiftUI APIs (`Grid`, `Canvas`, `Bindable`,
  `Timer`) to avoid bleeding-edge breakage.
- **GPU FFI still uses private framework via `libloading`.** Unchanged from
  current behavior; degrades to `gpu_present = 0` if IOReport breaks. Swift
  side just shows `n/a` in that case.

## What we throw away

- `src/main.rs`, `src/format.rs`, `src/ui/{popover,sparkline,cores,procs,tray}.rs`
- The eframe / egui / objc2-app-kit / objc2-foundation Cargo dependencies
- `assets/lsuielement.plist`, the `[package.metadata.bundle]` section
- `examples/check_*.rs` may stay; they're diagnostic-only

## What we keep

- All sampling logic: `cpu.rs`, `mem.rs`, `procs.rs`, `gpu.rs`, `sampler.rs`,
  `store.rs`, `sample.rs`, `settings.rs`, `logging.rs`
  - **Note on `Settings::menu_bar_format`**: Swift owns status item formatting
    now, so this field becomes dead weight. Keep it in the struct (so old
    `config.json` files still parse) but ignore it. Document as deprecated
    in a doc-comment.
- The IOReport private-framework binding (re-tested unchanged from Swift)
- Settings persistence path and JSON format
- The log rotation configuration

## Migration order (foreshadows the plan)

1. Restructure Cargo to produce a `staticlib`; keep the existing binary for
   parity until the Swift side ships.
2. Add `src/ffi.rs` and `cbindgen.toml`; generate `include/monitor_rs.h`.
3. Scaffold `Package.swift` and a minimal `MonitorRSApp` that prints
   `monitor_rs_start()`-`monitor_rs_latest()` to stdout. Wire the build script.
4. Build the menu bar + empty popover in Swift. Verify the status item shows.
5. Implement each SwiftUI component (`MetricTile`, `Sparkline`, `CoreGrid`,
   `ProcessList`) bottom-up.
6. Wire `PopoverView` and the 250 ms refresh.
7. Bundle to `.app`, smoke-test the checklist from the original README.
8. Delete the old `eframe` UI and dependencies; update the README.

The full step-by-step plan is the next document.
