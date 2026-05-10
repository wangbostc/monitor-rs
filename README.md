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

Settings live at `~/Library/Application Support/dev.monitor-rs.monitor-rs/config.json`:

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
