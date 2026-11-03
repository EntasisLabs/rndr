#!/usr/bin/env bash
# Bounded selection increment gate, reusing the accepted data verification gate.
set -euo pipefail
cd "$(dirname "$0")/.."
[ -n "${CARGO_TARGET_DIR:-}" ] || { echo 'Set retained compatible CARGO_TARGET_DIR first.' >&2; exit 2; }
source scripts/data-gallery-env.sh
out="${1:?Supply a NEW output directory}"
[ ! -e "$out" ] || { echo "Refusing existing output: $out" >&2; exit 2; }
check_disk() { [ "$(df -Pk . | awk 'NR==2 {print $4}')" -ge 2097152 ] || { echo 'Less than 2 GiB free; stop without cleanup.' >&2; exit 2; }; }
check_disk
mkdir -p "$out"
run() { local name="$1"; shift; check_disk; { printf '$ '; printf '%q ' "$@"; printf '\n'; } > "$out/$name.log"; local rc=0; "$@" >> "$out/$name.log" 2>&1 || rc=$?; printf '\nEXIT=%s\n' "$rc" >> "$out/$name.log"; if [ "$rc" -ne 0 ]; then tail -65 "$out/$name.log"; return "$rc"; fi; printf 'PASS %s\n' "$name"; }
# Includes core/paint/UI (all maintained selection and inspector tests), original
# collection review fixtures, native/fallback renderer, facade and data gallery.
bash scripts/verify-data-editors.sh "$out/data-verification"
run new-formatting rustfmt --edition 2021 --config skip_children=true --check crates/rndr_core/src/popup.rs crates/rndr_core/src/layout.rs crates/rndr_core/src/overlay.rs crates/rndr_ui/src/selection.rs crates/rndr_ui/tests/selection.rs crates/rndr_ui/tests/selection_scope.rs crates/rndr_ui/tests/selection_gallery.rs crates/rndr_ui/examples/support/selection_gallery.rs crates/rndr/examples/selection_gallery.rs
run python python3 -m unittest discover -s tools -p 'test_*.py'
run selection-native-build cargo build --offline --locked -p rndr --example selection_gallery --features observe
run selection-fallback-check cargo check --offline --locked -p rndr --example selection_gallery --no-default-features
run whitespace git diff --check
{ date -u; git rev-parse HEAD; df -h .; } > "$out/completion.txt"
echo 'PASS bounded selection gate (test execution totals overlap other runs; native capture is separate)'
