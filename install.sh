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
