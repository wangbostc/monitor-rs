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
3. Generates `Resources/icon/AppIcon.icns` from `Resources/icon/gen_icon.swift`
   (SF-Symbol-based placeholder)
4. `swift build -c release` → compiles the SwiftPM `MonitorRSApp` executable
5. Bundles the binary + `Info.plist` + icon into `target/release/monitor-rs.app`
6. Ad-hoc codesigns the bundle (`codesign --sign -`) so Gatekeeper doesn't
   block it on this Mac

Prerequisites: Rust 1.78+, Xcode Command Line Tools (Swift 5.10+),
`cargo install cbindgen`.

> If thermal readouts show `—` on a chip we don't have keys for yet,
> run `cargo run --example list_thermal_sensors` and extend the
> `TABLE` constant in `src/metrics/thermal.rs` with the sensor names
> printed there.

## Install

```
./install.sh
```

Copies the built `.app` to `/Applications/monitor-rs.app`. After installing,
launch from Spotlight, Launchpad, or Finder → Applications by double-clicking.

On first launch the app registers itself with `SMAppService` so it auto-starts
at login. To turn that off, go to **System Settings → General → Login Items**
and toggle "monitor-rs" off.

## Run (from local build, without installing)

```
open target/release/monitor-rs.app
```

The ad-hoc signature is tied to this Mac. If you copy the bundle to another
Mac it will be unsigned on that machine; right-click → **Open** the first
time to authorize.

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

- [ ] Menu-bar status rotates through 7 entries on a ~14 s loop:
      `CPU N%` → `GPU N%` → `MEM N%` → `NET ↓X.X ↑Y.Y` → `DSK ↓X.X ↑Y.Y`
      → `BAT N%[⚡]` → `TMP N°C`.
- [ ] The status item does NOT change width as the rotation cycles — the
      button frame stays fixed for the widest entry's pixel width, and
      neighbouring menu bar items do not visibly shift.
- [ ] Clicking the status item shows a translucent popover anchored beneath it.
      Re-opening the popover at any point in the rotation cycle anchors it at
      the same X position.
- [ ] Popover shows ONE large tinted hero card (CPU by default — green) with
      a big percentage, a meta line (`N-core · hot core M%`), and an area
      sparkline on the right.
- [ ] Below the hero, four pills (GPU / MEM / NET / DSK) show their
      current values.
- [ ] Per-core grid is always visible, directly below the pills row,
      regardless of which metric is hero.
- [ ] Tapping a pill pins that metric as the new hero with a brief
      fade-in. A small filled dot appears next to its label. The popover
      bottom edge does NOT move when the hero swaps.
- [ ] Tapping the pinned hero unpins it; auto-promotion resumes.
- [ ] Top processes section updates live.
- [ ] Running `yes > /dev/null` × N keeps CPU as the hero (already #1).
- [ ] Per-core grid lights up redder with load.
- [ ] GPU hero shows `n/a` / `Metal idle` if IOReport binding is unavailable;
      otherwise pinning GPU shows a live percentage.
- [ ] Downloading swaps the hero to NET within ~5 s of sustained transfer
      (`curl -o /dev/null https://speed.cloudflare.com/__down\?bytes\=200000000`);
      ending the transfer returns the hero to CPU after the matching window.
- [ ] Sustained writes swap the hero to DSK
      (`dd if=/dev/zero of=/tmp/iotest bs=1m count=2000 && rm /tmp/iotest`);
      finishing returns it to CPU.
- [ ] With macOS "Reduce motion" enabled (System Settings → Accessibility
      → Display → Reduce motion), hero swaps happen with no animation.
- [ ] Footer shows `🔋 N% [⚡]` when on a laptop; bolt drops when unplugged;
      battery chip is hidden entirely on a desktop.
- [ ] Footer shows `🌡 CPU N° GPU N°` (M-series only); under sustained CPU
      load the CPU number rises faster than the GPU number.
- [ ] Power icon in the header quits the app cleanly.
- [ ] Light and Dark mode both look correct (toggle via System Settings →
      Appearance).
- [ ] No Dock icon appears (`LSUIElement` is set in the bundled `.app`).

## Architecture

See `docs/superpowers/specs/2026-05-11-popover-hero-redesign-design.md`
(supersedes the original popover spec for the UI layer; the original
`2026-05-11-swiftui-popover-redesign.md` still documents the
Rust/FFI/sampling architecture).

## Repository layout

```
src/                # Rust sampling core (library only)
Sources/            # SwiftPM targets:
                    #   MonitorRSC    - C bindings (cbindgen header)
                    #   MonitorRSLogic - pure-Swift logic (MetricKind, HeroSelector)
                    #   MonitorRSApp  - SwiftUI executable
include/            # cbindgen-generated C header (committed)
Resources/          # Info.plist for the .app
build.sh            # End-to-end build script
Package.swift       # SwiftPM root
Cargo.toml          # [lib] crate-type = ["staticlib", "rlib"]
```
