#!/usr/bin/env bash
# Build script: cargo (staticlib) + cbindgen (header) + icon + swift (app) + .app bundle.
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
