#!/usr/bin/env bash
# Review evidence only. Exits 101 while any confirmed regression remains.
set -euo pipefail
cd "$(dirname "$0")/../../.."
D=evidence/data-components-next/independent-combined-review
out="${1:?Supply a fresh output directory}"
[ ! -e "$out" ] || { echo 'Refusing existing output' >&2; exit 2; }
source scripts/data-gallery-env.sh
for n in data field gallery; do
 [ ! -e "crates/rndr_ui/tests/reviewer_$n.rs" ] || { echo 'Refusing existing test target' >&2; exit 2; }
done
mkdir -p "$out"
# Remove only the three temporary files this invocation creates.
trap 'rm -f crates/rndr_ui/tests/reviewer_data.rs crates/rndr_ui/tests/reviewer_field.rs crates/rndr_ui/tests/reviewer_gallery.rs' EXIT
for n in data field gallery; do cp "$D/reviewer_$n.rs" "crates/rndr_ui/tests/reviewer_$n.rs"; done
failed=0
for n in data field gallery; do
 result=0
 { printf '$ cargo test --offline --locked -p rndr_ui --test reviewer_%s -- --nocapture\n' "$n";
   cargo test --offline --locked -p rndr_ui --test "reviewer_$n" -- --nocapture; } > "$out/$n.log" 2>&1 || result=$?
 printf '\nEXIT=%s\n' "$result" >> "$out/$n.log"
 [ "$result" = 0 ] || failed=101
done
exit "$failed"
