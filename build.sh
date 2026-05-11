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
