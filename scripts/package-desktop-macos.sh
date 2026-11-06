#!/bin/sh
# Local unsigned macOS bundle. Developers need Cargo; end users do not.
set -eu
[ "$(uname -s)" = Darwin ] || { echo 'macOS packaging requires macOS' >&2; exit 1; }
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
PROFILE=${1:-release}
case "$PROFILE" in dev) PROFILE_DIR=debug ;; release) PROFILE_DIR=release ;; *) echo 'Usage: scripts/package-desktop-macos.sh [release|dev]' >&2; exit 2 ;; esac
# Refuse to consume the last 2 GiB. This is headroom, not a build-size promise.
FREE=$(df -k . | tail -1 | awk '{print $4}')
[ "$FREE" -ge 2097152 ] || { echo 'Need at least 2 GiB free before packaging' >&2; exit 1; }
TARGET=${CARGO_TARGET_DIR:-target}
case "$TARGET" in /*) ;; *) TARGET="$ROOT/$TARGET" ;; esac
OUTPUT="$TARGET/bundle/Desktop Workshop.app"
[ ! -e "$OUTPUT" ] || { echo "Refusing to replace existing bundle: $OUTPUT" >&2; exit 1; }
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_RELEASE_DEBUG=0
# Explicit native target and output root override Cargo build.target/target-dir
# configuration. Otherwise a configured cross target can leave a stale host
# binary at the old path and silently bundle it instead of this build's output.
HOST=$(rustc -vV | sed -n 's/^host: //p')
case "$HOST" in *-apple-darwin) ;; *) echo "Not a native macOS Rust toolchain: $HOST" >&2; exit 1 ;; esac
cargo build -p desktop --profile "$PROFILE" --target "$HOST" --target-dir "$TARGET" --locked --offline
mkdir -p "$TARGET/bundle"
STAGE=$(mktemp -d "$TARGET/bundle/.desktop-workshop.XXXXXX")
trap 'rm -rf "$STAGE"' EXIT HUP INT TERM
APP="$STAGE/Desktop Workshop.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$TARGET/$HOST/$PROFILE_DIR/desktop" "$APP/Contents/MacOS/DesktopWorkshop"
chmod 755 "$APP/Contents/MacOS/DesktopWorkshop"
python3 scripts/desktop-icon.py "$STAGE/DesktopWorkshop.iconset"
iconutil -c icns "$STAGE/DesktopWorkshop.iconset" -o "$APP/Contents/Resources/DesktopWorkshop.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleExecutable</key><string>DesktopWorkshop</string>
<key>CFBundleIconFile</key><string>DesktopWorkshop</string>
<key>CFBundleIdentifier</key><string>org.rndr.desktop-workshop</string>
<key>CFBundleName</key><string>Desktop Workshop</string>
<key>CFBundleDisplayName</key><string>Desktop Workshop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1</string>
<key>CFBundleVersion</key><string>1</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
plutil -lint "$APP/Contents/Info.plist"
# No codesign identity, network service, credentials or quarantine changes.
# The Rust toolchain may already ad-hoc sign the linked arm64 executable.
mv "$APP" "$OUTPUT"
printf 'Local unsigned bundle: %s\nLaunch: open "%s"\n' "$OUTPUT" "$OUTPUT"
