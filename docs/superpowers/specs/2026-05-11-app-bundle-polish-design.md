# App Bundle Polish — Design

Date: 2026-05-11
Status: Approved (brainstorming)

## Goal

Make `monitor-rs` feel like a normal macOS app you click to open:

1. Installable to `/Applications` (so Spotlight, Launchpad, and Finder → Applications find it).
2. Carries a real icon instead of the generic blank-document glyph.
3. Opens on double-click without the "unidentified developer" Gatekeeper warning.
4. Auto-launches when the user logs in.

Distribution scope is **this Mac only** — no plans to ship the bundle to other Macs, so no Apple Developer Program / notarization.

## Non-goals

- Cross-Mac distribution, notarization, DMG packaging.
- Auto-update mechanism (Sparkle etc.).
- In-app preferences UI for the login-item toggle (deferred — users can toggle from System Settings → General → Login Items).
- Custom-drawn icon. Placeholder generated from an SF Symbol is sufficient for v1; can be replaced by dropping in a real `icon.png` later.

## File layout

```
build.sh                            # role unchanged: produces target/release/monitor-rs.app
                                    # additions: icon step, ad-hoc codesign step
install.sh                          # NEW: deploys built .app to /Applications
Resources/
  Info.plist                        # +CFBundleIconFile = AppIcon
  icon/
    gen_icon.swift                  # NEW: renders 1024x1024 PNG (SF Symbol on tinted square)
    AppIcon.png                     # generated, gitignored
    AppIcon.iconset/                # generated, gitignored
    AppIcon.icns                    # generated, gitignored, copied into the bundle
Sources/MonitorRSApp/
  LoginItem.swift                   # NEW: SMAppService.mainApp wrapper
  <existing entry point>            # adds one call: LoginItem.ensureRegistered()
.gitignore                          # adds Resources/icon/AppIcon.{png,icns} and AppIcon.iconset/
```

Three new units, each with a single responsibility:

| Unit | Input | Output | Depends on |
|---|---|---|---|
| `gen_icon.swift` | SF Symbol name, tint color | `Resources/icon/AppIcon.png` (1024×1024) | AppKit / SwiftUI only |
| `install.sh` | `target/release/monitor-rs.app` | `/Applications/monitor-rs.app` | `ditto`, `xattr`, `pkill` |
| `LoginItem.swift` | n/a | Side effect: app registered with `SMAppService` | `ServiceManagement` |

## Build flow (`build.sh` changes)

Existing steps 1–3 untouched (`cargo build --release`, `cbindgen`, `swift build -c release`). Add the following after the existing bundle-assembly step:

1. **Icon generation** (skipped if `AppIcon.icns` is newer than `gen_icon.swift`):
   - `swift Resources/icon/gen_icon.swift` → writes `Resources/icon/AppIcon.png` at 1024×1024.
   - `sips` produces the 10 sizes required by `iconutil`:
     `icon_16x16.png`, `icon_16x16@2x.png`, `icon_32x32.png`, `icon_32x32@2x.png`,
     `icon_128x128.png`, `icon_128x128@2x.png`, `icon_256x256.png`, `icon_256x256@2x.png`,
     `icon_512x512.png`, `icon_512x512@2x.png`.
   - `iconutil -c icns Resources/icon/AppIcon.iconset` → `Resources/icon/AppIcon.icns`.
2. **Bundle the icon**: copy `Resources/icon/AppIcon.icns` into `$APP_DIR/Contents/Resources/`.
3. **Ad-hoc codesign**: `codesign --force --deep --sign - --options runtime "$APP_DIR"`.
4. **Verify**: `codesign --verify --deep --strict "$APP_DIR"`; fail the script if invalid.

`Info.plist` gains exactly one key:

```xml
<key>CFBundleIconFile</key>
<string>AppIcon</string>
```

### Why ad-hoc sign (not "leave it unsigned")

- A stable code signature gives the app a consistent identity for `SMAppService` and TCC permission grants — without it, every rebuild can re-trigger permission dialogs.
- macOS occasionally tightens Gatekeeper after system updates and starts blocking unsigned apps even from `/Applications`. Ad-hoc signing avoids that surprise.
- Cost: zero. No keychain, no Apple ID, no network round-trip.

## Install flow (`install.sh`)

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"

APP_SRC="target/release/monitor-rs.app"
APP_DST="/Applications/monitor-rs.app"

[[ -d "$APP_SRC" ]] || { echo "build first: ./build.sh"; exit 1; }

pkill -x monitor-rs 2>/dev/null || true

rm -rf "$APP_DST"
ditto "$APP_SRC" "$APP_DST"

xattr -dr com.apple.quarantine "$APP_DST" 2>/dev/null || true

echo "Installed: $APP_DST"
echo "Launch with: open '$APP_DST'   (or via Spotlight / Launchpad)"
```

Behavior:

- **Idempotent.** Re-running cleanly replaces the previous install.
- **No login-item logic here.** The app itself registers via `SMAppService` on launch. Keeps the script to a single job.
- **`pkill` first** so the script doesn't try to overwrite a running binary.
- **`ditto`, not `cp -R`** — `ditto` is the macOS-blessed copy; preserves extended attributes, symlinks, resource forks; `cp -R` has edge-case bugs on bundles.
- **`xattr -dr com.apple.quarantine`** is defense in depth. A locally-built bundle won't have the quarantine bit, but stripping it is a no-op if absent.

## Launch at login (`LoginItem.swift`)

```swift
// Sources/MonitorRSApp/LoginItem.swift
import ServiceManagement
import os

enum LoginItem {
    private static let log = Logger(subsystem: "dev.monitor-rs", category: "login-item")

    static var isRegistered: Bool {
        SMAppService.mainApp.status == .enabled
    }

    /// Register the app to auto-launch at login. Safe to call repeatedly.
    static func ensureRegistered() {
        let svc = SMAppService.mainApp
        guard svc.status != .enabled else { return }
        do {
            try svc.register()
            log.info("registered for launch-at-login")
        } catch {
            log.error("register failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    static func unregister() {
        try? SMAppService.mainApp.unregister()
    }
}
```

Wiring: one call to `LoginItem.ensureRegistered()` from the existing app-startup path (wherever `MonitorRSApp` initializes its `NSStatusItem` / `MenuBarExtra`). Idempotent — `status == .enabled` short-circuits.

User control surfaces:

- **System Settings → General → Login Items** lists "monitor-rs" with a toggle. Standard macOS surface.
- No in-app toggle in v1.

### Edge cases

- First call after install may show a one-time system notification "monitor-rs added to Login Items." That's the OS, not suppressible, not a bug.
- If the `.app` is run from a path other than `/Applications` (e.g., directly from `target/release/`), `SMAppService` notices the path mismatch and the System Settings toggle reads "Not Registered." Re-running `./install.sh` + launching from `/Applications` re-registers.
- Ad-hoc signing is compatible with `SMAppService.mainApp` (the per-app variant). The `SMAppService.daemon` / `SMAppService.agent` variants would require a Developer ID — we don't use those.

## Testing / verification checklist

Manual, run on a single Mac:

- [ ] `./build.sh` runs to completion with no warnings; `codesign --verify --deep --strict target/release/monitor-rs.app` succeeds.
- [ ] `target/release/monitor-rs.app` shows the new icon in Finder.
- [ ] `./install.sh` produces `/Applications/monitor-rs.app`.
- [ ] Spotlight (`Cmd-Space` → "monitor-rs") finds and launches the app.
- [ ] Launchpad shows the app with the new icon.
- [ ] Double-click from Finder → Applications launches the app with no Gatekeeper warning.
- [ ] System Settings → General → Login Items shows "monitor-rs" enabled after first launch.
- [ ] Reboot (or log out / log in) — app launches automatically, status item appears in the menu bar.
- [ ] Toggling Login Items off in System Settings disables auto-launch on next login.
- [ ] All existing smoke-test items from `README.md` still pass.

## Open questions

None. All design choices were made during brainstorming:

- Distribution scope: this Mac only → ad-hoc signing.
- Icon source: generated placeholder from SF Symbol.
- Build/install split: separate scripts.
- Login mechanism: `SMAppService.mainApp` in Swift (not `osascript` from shell).
