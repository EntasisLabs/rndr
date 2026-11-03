#!/usr/bin/env bash
# Bounded synthetic capture matrix; does not certify native input.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
: "${CARGO_TARGET_DIR:?Export the existing shared CARGO_TARGET_DIR; no fresh target is created}"
[ "$#" -eq 1 ] || { echo "usage: $0 NEW_OUTPUT_DIRECTORY" >&2; exit 2; }
out="$1"
mkdir -p "$out"
manifest="${RNDR_POLISH_MANIFEST:-$PWD/Cargo.toml}"
source_root="$(cd "$(dirname "$manifest")" && pwd)"
available="$(df -Pk . | awk 'END {print $4}')"
[ "$available" -ge 1048576 ] || { echo "Need at least 1 GiB free before this bounded build; no cleanup attempted" >&2; exit 1; }
# Archived Git sources carry old timestamps. In the shared cache explicitly
# invalidate ONLY these local package entry points, not dependency caches.
for crate in rndr_core rndr_paint rndr_ui rndr_render rndr_std rndr; do
  touch "$source_root/crates/$crate/src/lib.rs"
done
cargo build --offline --locked --manifest-path "$manifest" -p rndr --example ui_polish --features observe
binary="$CARGO_TARGET_DIR/debug/examples/ui_polish"
shasum -a 256 "$binary" > "$out/binary.sha256"
(cd "$source_root" && shasum -a 256 crates/rndr_core/src/{lib.rs,style.rs,layout.rs,focus.rs} crates/rndr_paint/src/lib.rs crates/rndr_ui/src/{collections.rs,controls.rs,data.rs,selection.rs,style.rs} crates/rndr/examples/ui_polish.rs) > "$out/source.sha256"
for scene in selection data editors; do
  for mode in light-normal-1x dark-normal-2x light-narrow-2x dark-narrow-1x; do
    args=(--scene "$scene")
    case "$mode" in
      dark-normal-2x) args+=(--dark --2x);;
      light-narrow-2x) args+=(--narrow --2x);;
      dark-narrow-1x) args+=(--dark --narrow);;
    esac
    "$binary" "${args[@]}" --focus-view --capture-prefix "$out/$scene-$mode"
  done
done
"$binary" --scene editors --2x --focus-label Disable --capture-prefix "$out/focus-light-2x"
echo '13 PNG/JSON pairs: headless viewport-composite pixels, real text shaping; not native input certification.'
