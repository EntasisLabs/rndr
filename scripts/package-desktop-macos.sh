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
[ ! -e "$OUTPUT" ] && [ ! -L "$OUTPUT" ] || { echo "Refusing to replace existing bundle: $OUTPUT" >&2; exit 1; }
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
python3 scripts/package-app-icon.py macos \
    --id org.rndr.desktop-workshop --name 'Desktop Workshop' \
    --binary "$TARGET/$HOST/$PROFILE_DIR/desktop" --executable-name DesktopWorkshop \
    --icon examples/desktop/icons/app.png --version 0.1 --build-number 1 --output "$OUTPUT"
# The Rust toolchain may already ad-hoc sign the linked arm64 executable.
printf 'Local unsigned bundle: %s\nLaunch: open "%s"\n' "$OUTPUT" "$OUTPUT"
