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
