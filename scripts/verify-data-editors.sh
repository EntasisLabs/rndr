#!/usr/bin/env bash
# Combined candidate gate. Fresh evidence only; bounded builds, no shared cleanup.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/data-gallery-env.sh
out="${1:?Supply a NEW verification output directory}"
[ ! -e "$out" ] || { echo "Refusing existing output: $out" >&2; exit 2; }
[ "$(df -Pk . | awk 'NR==2 {print $4}')" -ge 2097152 ] || { echo 'Less than 2 GiB free; stop without cleanup.' >&2; exit 2; }
mkdir -p "$out"
run() {
    local name="$1"; shift
    { printf '$ '; printf '%q ' "$@"; printf '\n'; } > "$out/$name.log"
    local result=0
    "$@" >> "$out/$name.log" 2>&1 || result=$?
    printf '\nEXIT=%s\n' "$result" >> "$out/$name.log"
    if [ "$result" -ne 0 ]; then tail -65 "$out/$name.log"; return "$result"; fi
    printf 'PASS %s\n' "$name"
}
{ date -u; git rev-parse HEAD; rustc -V; cargo -V; printf 'cache=%s\njobs=%s incremental=%s dev_debug=%s test_debug=%s\n' "$CARGO_TARGET_DIR" "$CARGO_BUILD_JOBS" "$CARGO_INCREMENTAL" "$CARGO_PROFILE_DEV_DEBUG" "$CARGO_PROFILE_TEST_DEBUG"; df -h .; } > "$out/environment.txt"
run formatting rustfmt --edition 2021 --config skip_children=true --check \
 crates/rndr_core/src/access.rs crates/rndr_core/src/field.rs crates/rndr_core/src/interaction.rs crates/rndr_core/src/lib.rs \
 crates/rndr_render/src/a11y.rs crates/rndr_ui/src/collections.rs crates/rndr_ui/src/controls.rs crates/rndr_ui/src/data.rs crates/rndr_ui/src/lib.rs \
 crates/rndr_ui/tests/data_editors.rs crates/rndr_ui/tests/data_editors_gallery.rs crates/rndr_ui/tests/field_transactions.rs \
 crates/rndr_ui/examples/support/data_editors_gallery.rs crates/rndr/examples/data_editors_gallery.rs
run core-paint-ui cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
# These historical fixtures are copied byte-for-byte, never edited in place.
f1=crates/rndr_ui/tests/combined_original_six.rs
f2=crates/rndr_ui/tests/combined_activation_policy.rs
[ ! -e "$f1" ] && [ ! -e "$f2" ] || { echo 'Refusing existing temporary fixture' >&2; exit 2; }
trap 'rm -f "$f1" "$f2"' EXIT
cp evidence/data-components/independent-review/adversarial.rs "$f1"
cp evidence/data-components/independent-rereview/activation-policy.rs "$f2"
run historical-six-and-activation cargo test --offline --locked -p rndr_ui --test combined_original_six --test combined_activation_policy
rm -f "$f1" "$f2"
trap - EXIT
run native-observe cargo test --offline --locked -p rndr_render --lib --features observe
run fallback-renderer cargo test --offline --locked -p rndr_render --lib --no-default-features
run facade cargo test --offline --locked -p rndr --lib --features observe
run facade-fallback cargo test --offline --locked -p rndr --lib --no-default-features
run gallery-build cargo build --offline --locked -p rndr --example data_editors_gallery --features observe
run gallery-fallback cargo check --offline --locked -p rndr --example data_editors_gallery --no-default-features
run whitespace git diff --check
{ date -u; df -h .; } > "$out/final-disk.txt"
echo 'Verification complete. PNG capture and pixel inspection remain a separate gate.'
