#!/usr/bin/env bash
# Reproduce tests and native build without destructive cache cleanup.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/data-gallery-env.sh
out="${1:-evidence/data-components}"
mkdir -p "$out"
run() {
    local name="$1"; shift
    { printf '$ '; printf '%q ' "$@"; printf '\n'; } > "$out/$name.log"
    if "$@" >> "$out/$name.log" 2>&1; then
        printf '\nEXIT=0\n' >> "$out/$name.log"
        printf 'PASS %s\n' "$name"
    else
        local result=$?
        printf '\nEXIT=%s\n' "$result" >> "$out/$name.log"
        tail -60 "$out/$name.log"
        return "$result"
    fi
}
{
    date -u
    rustc -V; cargo -V
    printf 'worktree=%s\ncache=%s\n' "$PWD" "$CARGO_TARGET_DIR"
    printf 'jobs=%s incremental=%s dev_debug=%s test_debug=%s\n' "$CARGO_BUILD_JOBS" "$CARGO_INCREMENTAL" "$CARGO_PROFILE_DEV_DEBUG" "$CARGO_PROFILE_TEST_DEBUG"
    df -h .
    du -sh "$CARGO_TARGET_DIR" 2>/dev/null || true
} > "$out/verification-environment.txt"
free_k=$(df -Pk . | awk 'NR == 2 { print $4 }')
if [ "$free_k" -lt 2097152 ]; then
    echo 'Less than 2 GiB free; stop instead of cleaning shared caches.' >&2; exit 2
fi
run formatting rustfmt --edition 2021 --config skip_children=true --check \
    crates/rndr_core/src/access.rs crates/rndr_core/src/focus.rs \
    crates/rndr_core/src/interaction.rs crates/rndr_core/src/lib.rs \
    crates/rndr_render/src/a11y.rs crates/rndr_ui/src/controls.rs \
    crates/rndr_ui/src/lib.rs crates/rndr_ui/src/collections.rs \
    crates/rndr_ui/tests/collections.rs crates/rndr_ui/examples/support/data_gallery.rs \
    crates/rndr/examples/data_gallery.rs
run core-paint-ui cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
run collection-metrics cargo test --offline --locked -p rndr_ui --test collections -- --nocapture
run native-observe cargo test --offline --locked -p rndr_render --lib --features observe
run fallback-renderer cargo test --offline --locked -p rndr_render --lib --no-default-features
run facade cargo test --offline --locked -p rndr --lib --features observe
run facade-fallback cargo test --offline --locked -p rndr --lib --no-default-features
run python python3 -m unittest discover -s tools -p 'test_*.py'
run gallery-build cargo build --offline --locked -p rndr --example data_gallery --features observe
run gallery-fallback-check cargo check --offline --locked -p rndr --example data_gallery --no-default-features
{ df -h .; du -sh "$CARGO_TARGET_DIR"; } > "$out/final-disk.txt"
printf 'Verification complete. Native capture/PNG inspection is a separate gate.\n'
