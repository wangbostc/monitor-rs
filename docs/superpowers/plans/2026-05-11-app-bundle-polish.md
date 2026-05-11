# App Bundle Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn `target/release/monitor-rs.app` into a real clickable macOS app — installable to `/Applications`, with an icon, no Gatekeeper warning, and auto-launch at login.

**Architecture:** Three additions: (1) a Swift one-shot script that renders a placeholder app icon from an SF Symbol, plumbed through `build.sh` and ad-hoc codesigned into the bundle; (2) a new `install.sh` that `ditto`s the built bundle into `/Applications`; (3) a `LoginItem.swift` wrapper around `SMAppService.mainApp` called once from `AppDelegate.applicationDidFinishLaunching` to register for launch-at-login.

**Tech Stack:** Swift / SwiftUI `ImageRenderer`, AppKit `NSBitmapImageRep`, `sips`, `iconutil`, `codesign --sign -` (ad-hoc), `ditto`, Apple `ServiceManagement.framework` (`SMAppService.mainApp`).

**Design doc:** `docs/superpowers/specs/2026-05-11-app-bundle-polish-design.md`

---

## File map

| Action | Path | Responsibility |
|---|---|---|
| Create | `Resources/icon/gen_icon.swift` | Render 1024×1024 PNG from SF Symbol; writes to `argv[1]` |
| Create | `install.sh` | Idempotent: kill running app → `ditto` to `/Applications` → strip quarantine |
| Create | `Sources/MonitorRSApp/LoginItem.swift` | Thin wrapper around `SMAppService.mainApp` |
| Modify | `build.sh` | Add icon-gen step + bundle-copy step + ad-hoc codesign + verify |
| Modify | `Resources/Info.plist` | Add `CFBundleIconFile` = `AppIcon` |
| Modify | `Sources/MonitorRSApp/AppDelegate.swift` | One-line call to `LoginItem.ensureRegistered()` |
| Modify | `.gitignore` | Ignore `Resources/icon/AppIcon.{png,icns}` + `AppIcon.iconset/` |
| Modify | `README.md` | Update Build / Run sections to mention `install.sh` and login-item behavior |

---

## Task 1: Add icon generator script

**Files:**
- Create: `Resources/icon/gen_icon.swift`

- [ ] **Step 1: Create the icon-generator script**

```swift
// Resources/icon/gen_icon.swift
// Single-file Swift script. Usage: swift gen_icon.swift <out.png>
// Renders a 1024x1024 placeholder app icon: SF Symbol on a tinted rounded square.
import SwiftUI
import AppKit

let size: CGFloat = 1024
let symbolName = "gauge.with.dots.needle.50percent"

guard CommandLine.arguments.count == 2 else {
    FileHandle.standardError.write("usage: gen_icon.swift <out.png>\n".data(using: .utf8)!)
    exit(2)
}
let outURL = URL(fileURLWithPath: CommandLine.arguments[1])

let view = ZStack {
    RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
        .fill(LinearGradient(
            colors: [Color(red: 0.10, green: 0.55, blue: 0.95),
                     Color(red: 0.05, green: 0.30, blue: 0.75)],
            startPoint: .topLeading,
            endPoint: .bottomTrailing))
    Image(systemName: symbolName)
        .font(.system(size: size * 0.55, weight: .regular))
        .foregroundStyle(.white)
}
.frame(width: size, height: size)

let renderer = ImageRenderer(content: view)
renderer.scale = 1

guard let nsImage = renderer.nsImage,
      let tiff = nsImage.tiffRepresentation,
      let rep  = NSBitmapImageRep(data: tiff),
      let png  = rep.representation(using: .png, properties: [:])
else {
    FileHandle.standardError.write("icon render failed\n".data(using: .utf8)!)
    exit(1)
}

try png.write(to: outURL)
print("wrote \(outURL.path) (\(png.count) bytes)")
```

- [ ] **Step 2: Verify it runs and produces a PNG**

Run:
```bash
mkdir -p /tmp/icon-smoke
swift Resources/icon/gen_icon.swift /tmp/icon-smoke/AppIcon.png
file /tmp/icon-smoke/AppIcon.png
```

Expected: prints `wrote /tmp/icon-smoke/AppIcon.png (… bytes)`, and `file` output contains `PNG image data, 1024 x 1024`.

- [ ] **Step 3: Commit**

```bash
git add Resources/icon/gen_icon.swift
git commit -m "feat(icon): add SwiftUI icon-generator script"
```

---

## Task 2: Ignore generated icon artifacts

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Add icon build artifacts to .gitignore**

Append to `.gitignore`:

```
/Resources/icon/AppIcon.png
/Resources/icon/AppIcon.iconset/
/Resources/icon/AppIcon.icns
```

The script `gen_icon.swift` is the committed source of truth; PNG/iconset/icns are reproducible build artifacts.

- [ ] **Step 2: Verify**

Run:
```bash
git check-ignore -v Resources/icon/AppIcon.icns
```

Expected: prints a line showing `.gitignore` matched the pattern. (If `AppIcon.icns` doesn't exist yet, `--no-index` would be needed, but `check-ignore` against an unborn path still resolves the ignore rule.)

- [ ] **Step 3: Commit**

```bash
git add .gitignore
git commit -m "chore: ignore generated icon artifacts"
```

---

## Task 3: Declare the icon in Info.plist

**Files:**
- Modify: `Resources/Info.plist`

- [ ] **Step 1: Add CFBundleIconFile**

In `Resources/Info.plist`, insert the following two lines immediately before the closing `</dict>`:

```xml
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
```

(`CFBundleIconFile` value is the basename without extension. macOS resolves it to `Contents/Resources/AppIcon.icns`.)

- [ ] **Step 2: Verify the plist still parses**

Run:
```bash
plutil -lint Resources/Info.plist
```

Expected: `Resources/Info.plist: OK`.

- [ ] **Step 3: Commit**

```bash
git add Resources/Info.plist
git commit -m "feat(bundle): declare AppIcon in Info.plist"
```

---

## Task 4: Extend `build.sh` with icon generation

**Files:**
- Modify: `build.sh`

- [ ] **Step 1: Add the icon step to `build.sh`**

Replace the existing `build.sh` with the version below. Changes vs. current:
- New "icon" step between cbindgen and swift build.
- Copies `AppIcon.icns` into `$APP_DIR/Contents/Resources/`.

```bash
#!/usr/bin/env bash
# Build script: cargo (staticlib) + cbindgen (header) + icon + swift (app) + .app bundle + ad-hoc sign.
set -euo pipefail

cd "$(dirname "$0")"

echo "==> Building Rust static library..."
cargo build --release

echo "==> Regenerating C header..."
cbindgen --config cbindgen.toml --output include/monitor_rs.h

echo "==> Generating app icon..."
ICON_DIR="Resources/icon"
ICON_PNG="$ICON_DIR/AppIcon.png"
ICON_SET="$ICON_DIR/AppIcon.iconset"
ICON_ICNS="$ICON_DIR/AppIcon.icns"

# Rebuild icon if .icns is missing or older than the generator script.
if [[ ! -f "$ICON_ICNS" || "$ICON_DIR/gen_icon.swift" -nt "$ICON_ICNS" ]]; then
    swift "$ICON_DIR/gen_icon.swift" "$ICON_PNG"
    rm -rf "$ICON_SET"
    mkdir -p "$ICON_SET"
    for spec in "16 16 icon_16x16.png" \
                "32 32 icon_16x16@2x.png" \
                "32 32 icon_32x32.png" \
                "64 64 icon_32x32@2x.png" \
                "128 128 icon_128x128.png" \
                "256 256 icon_128x128@2x.png" \
                "256 256 icon_256x256.png" \
                "512 512 icon_256x256@2x.png" \
                "512 512 icon_512x512.png" \
                "1024 1024 icon_512x512@2x.png"; do
        # shellcheck disable=SC2086
        set -- $spec
        sips -z "$2" "$1" "$ICON_PNG" --out "$ICON_SET/$3" >/dev/null
    done
    iconutil -c icns "$ICON_SET" -o "$ICON_ICNS"
else
    echo "  (up to date)"
fi

echo "==> Building Swift app..."
swift build -c release

echo "==> Assembling .app bundle..."
APP_DIR="target/release/monitor-rs.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp ".build/release/MonitorRSApp" "$APP_DIR/Contents/MacOS/monitor-rs"
cp "Resources/Info.plist" "$APP_DIR/Contents/Info.plist"
cp "$ICON_ICNS" "$APP_DIR/Contents/Resources/AppIcon.icns"
echo "APPL????" > "$APP_DIR/Contents/PkgInfo"

echo "==> Done."
echo "Bundle: $APP_DIR"
ls -la "$APP_DIR/Contents/MacOS/" "$APP_DIR/Contents/Resources/"
```

Note: the `sips` size pairs follow the iconset rule that `@2x` files are double the base size at the *same* logical density. The first column of `$spec` is the pixel size to render; the third column is the iconset filename.

- [ ] **Step 2: Run the build**

```bash
./build.sh
```

Expected: completes without error. Output ends with the bundle path and listings showing both `monitor-rs` binary and `AppIcon.icns`.

- [ ] **Step 3: Verify the icon file is real and the bundle picked it up**

```bash
file Resources/icon/AppIcon.icns
file target/release/monitor-rs.app/Contents/Resources/AppIcon.icns
```

Expected: both report `Mac OS X icon`.

- [ ] **Step 4: Verify the icon renders in Finder**

```bash
open target/release/monitor-rs.app/Contents/Resources/
```

Expected: Finder window opens; `AppIcon.icns` shows a gradient-blue gauge glyph (not the generic icon).

- [ ] **Step 5: Commit**

```bash
git add build.sh
git commit -m "feat(build): generate AppIcon.icns and bundle it"
```

---

## Task 5: Ad-hoc codesign the bundle

**Files:**
- Modify: `build.sh`

- [ ] **Step 1: Append codesign + verify steps to `build.sh`**

In `build.sh`, replace the final two lines:

```bash
echo "Bundle: $APP_DIR"
ls -la "$APP_DIR/Contents/MacOS/" "$APP_DIR/Contents/Resources/"
```

with:

```bash
echo "==> Ad-hoc codesigning..."
codesign --force --deep --sign - --options runtime "$APP_DIR"

echo "==> Verifying signature..."
codesign --verify --deep --strict "$APP_DIR"

echo "==> Done."
echo "Bundle: $APP_DIR"
ls -la "$APP_DIR/Contents/MacOS/" "$APP_DIR/Contents/Resources/"
```

`--sign -` is ad-hoc — no keychain identity required. `--options runtime` enables the Hardened Runtime, which monitor-rs's current dependencies are compatible with (no JIT, no unsigned dylib loading; IOReport bindings via system framework are already system-signed).

- [ ] **Step 2: Run the build and watch the new steps**

```bash
./build.sh
```

Expected: prints `==> Ad-hoc codesigning...`, `==> Verifying signature...`, exits 0.

- [ ] **Step 3: Verify externally**

```bash
codesign -dvv target/release/monitor-rs.app 2>&1 | grep -E '^(Identifier|Signature|TeamIdentifier|Sealed Resources)'
```

Expected: `Signature=adhoc` appears in the output.

- [ ] **Step 4: Commit**

```bash
git add build.sh
git commit -m "feat(build): ad-hoc codesign the .app bundle"
```

---

## Task 6: Add `install.sh`

**Files:**
- Create: `install.sh`

- [ ] **Step 1: Create the install script**

```bash
#!/usr/bin/env bash
# Install the built .app bundle to /Applications. Idempotent.
set -euo pipefail

cd "$(dirname "$0")"

APP_SRC="target/release/monitor-rs.app"
APP_DST="/Applications/monitor-rs.app"

if [[ ! -d "$APP_SRC" ]]; then
    echo "error: $APP_SRC not found. Run ./build.sh first." >&2
    exit 1
fi

echo "==> Stopping any running instance..."
pkill -x monitor-rs 2>/dev/null || true

echo "==> Installing to $APP_DST..."
rm -rf "$APP_DST"
ditto "$APP_SRC" "$APP_DST"

# A locally-built bundle won't carry a quarantine xattr, but strip defensively.
xattr -dr com.apple.quarantine "$APP_DST" 2>/dev/null || true

echo "==> Done."
echo "Installed: $APP_DST"
echo "Launch:    open '$APP_DST'   (or via Spotlight / Launchpad)"
```

- [ ] **Step 2: Make it executable**

```bash
chmod +x install.sh
```

- [ ] **Step 3: Run it (requires you to have run `./build.sh` first)**

```bash
./install.sh
ls -ld /Applications/monitor-rs.app
```

Expected: prints `Installed: …`; `ls` shows `/Applications/monitor-rs.app` directory.

- [ ] **Step 4: Smoke test — launch from /Applications**

```bash
open /Applications/monitor-rs.app
```

Expected: app launches; menu-bar status item appears; no "unidentified developer" dialog. Quit it (power icon in popover header) before continuing.

- [ ] **Step 5: Verify Spotlight finds it**

Press `Cmd-Space`, type `monitor-rs`. Expected: the app appears as a top hit. Press Esc.

- [ ] **Step 6: Commit**

```bash
git add install.sh
git commit -m "feat: add install.sh to deploy bundle to /Applications"
```

---

## Task 7: Add `LoginItem.swift`

**Files:**
- Create: `Sources/MonitorRSApp/LoginItem.swift`

- [ ] **Step 1: Create the wrapper**

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

- [ ] **Step 2: Verify it compiles standalone**

```bash
swift build -c release
```

Expected: builds successfully. `LoginItem` is unused at this point (no call site yet) but Swift permits unused enums.

- [ ] **Step 3: Commit**

```bash
git add Sources/MonitorRSApp/LoginItem.swift
git commit -m "feat(login): add SMAppService.mainApp wrapper"
```

---

## Task 8: Call `LoginItem.ensureRegistered()` at startup

**Files:**
- Modify: `Sources/MonitorRSApp/AppDelegate.swift`

- [ ] **Step 1: Add the call to `applicationDidFinishLaunching`**

In `Sources/MonitorRSApp/AppDelegate.swift`, change:

```swift
    func applicationDidFinishLaunching(_ notification: Notification) {
        menuBarController = MenuBarController()
    }
```

to:

```swift
    func applicationDidFinishLaunching(_ notification: Notification) {
        menuBarController = MenuBarController()
        LoginItem.ensureRegistered()
    }
```

Order matters: bring up the menu-bar UI first so a slow `SMAppService.register()` call (rare) doesn't delay the user-visible status item.

- [ ] **Step 2: Rebuild and reinstall**

```bash
./build.sh && ./install.sh
```

Expected: both succeed.

- [ ] **Step 3: Launch from /Applications and verify login-item registration**

```bash
open /Applications/monitor-rs.app
```

Then open **System Settings → General → Login Items** and look under "Open at Login."

Expected: "monitor-rs" appears in the list, toggle is **on**. (macOS may show a one-time notification "monitor-rs added to Login Items." That's expected — it's an OS notification, not from the app.)

- [ ] **Step 4: Verify persistence by relaunching from cold**

Quit the app (power icon in popover), then reboot or log out / log in. Expected: monitor-rs starts automatically; status item appears in the menu bar without you opening the app.

- [ ] **Step 5: Commit**

```bash
git add Sources/MonitorRSApp/AppDelegate.swift
git commit -m "feat(login): register for launch-at-login on first run"
```

---

## Task 9: Update README

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the "Build" and "Run" sections**

In `README.md`, replace the existing **Build** section through the end of the **Run** section (lines 8–35 in the current file, ending with "right-click the `.app` in Finder and choose **Open** to authorize it.") with:

````markdown
## Build

```
./build.sh
```

The script:
1. `cargo build --release` → produces `libmonitor_rs.a` (Rust sampling core)
2. `cbindgen --config cbindgen.toml --output include/monitor_rs.h` → regenerates the C header
3. Generates `Resources/icon/AppIcon.icns` from `Resources/icon/gen_icon.swift` (SF-Symbol-based placeholder)
4. `swift build -c release` → compiles the SwiftPM `MonitorRSApp` executable
5. Bundles the binary + `Info.plist` + icon into `target/release/monitor-rs.app`
6. Ad-hoc codesigns the bundle (`codesign --sign -`) so Gatekeeper doesn't block it on this Mac

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
Mac it will be unsigned on that machine; right-click → **Open** the first time
to authorize.
````

- [ ] **Step 2: Verify the README still renders**

```bash
head -60 README.md
```

Expected: sections are well-formed; no broken code fences.

- [ ] **Step 3: Commit**

```bash
git add README.md
git commit -m "docs: document install.sh and launch-at-login behavior"
```

---

## Final verification (manual smoke test)

After all tasks are done, run through this checklist once on a clean state:

- [ ] `rm -rf target/release/monitor-rs.app .build Resources/icon/AppIcon.{png,icns} Resources/icon/AppIcon.iconset` → then `./build.sh` → succeeds end-to-end.
- [ ] `codesign --verify --deep --strict target/release/monitor-rs.app` → exits 0.
- [ ] `./install.sh` → `/Applications/monitor-rs.app` exists.
- [ ] Spotlight (`Cmd-Space` → "monitor-rs") finds and launches the app on first try, no Gatekeeper dialog.
- [ ] Launchpad shows the app with the gauge icon.
- [ ] Finder → Applications shows the app with the gauge icon.
- [ ] System Settings → General → Login Items lists "monitor-rs" enabled.
- [ ] Log out, log back in: monitor-rs auto-launches; status item appears.
- [ ] All existing smoke-test items from `README.md` (menu-bar rotation, popover, sparklines, etc.) still pass after install.

---

## Self-review notes

- **Spec coverage:** every requirement from the design doc maps to a task — Applications install (T6), icon (T1–T4), ad-hoc sign (T5), login-at-login (T7–T8), README (T9), gitignore (T2), Info.plist (T3).
- **Placeholders:** none. Every code block is complete and runnable as-is.
- **Type consistency:** `LoginItem.ensureRegistered()` defined in T7 and called with that exact name in T8. `AppIcon` is the basename used consistently across Info.plist (T3), the iconset build (T4), and the bundle-copy step (T4).
- **Ambiguity in the spec ("wherever MonitorRSApp initializes its NSStatusItem / MenuBarExtra"):** pinned in T8 to `AppDelegate.applicationDidFinishLaunching` after `menuBarController = MenuBarController()`.
