#!/usr/bin/env bash
# Serial, capacity-guarded verification. Requires an existing shared build cache.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
: "${CARGO_TARGET_DIR:?Set CARGO_TARGET_DIR to your established shared target first}"
[[ -d "$CARGO_TARGET_DIR" ]] || { echo 'Refusing a new target directory' >&2; exit 2; }
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
out=${1:?Usage: scripts/verify-markdown-slots.sh NEW-output-directory}
[[ ! -e "$out" ]] || { echo 'Output directory must not exist' >&2; exit 2; }
mkdir -p "$(dirname "$out")"
mkdir "$out"
run() {
    local label=$1; shift
    local available
    available=$(df -Pk . | awk 'NR == 2 {print $4}')
    printf '%s available KiB=%s command:' "$label" "$available" | tee -a "$out/commands.log"
    printf ' %q' "$@" | tee -a "$out/commands.log"
    printf '\n' | tee -a "$out/commands.log"
    # Never clean user/cache data to make a gate pass. Refuse below 750 MiB.
    (( available >= 768000 )) || { echo 'Capacity blocker; work preserved' >&2; exit 3; }
    if "$@" > "$out/$label.log" 2>&1; then
        echo "$label PASS" | tee -a "$out/commands.log"
    else
        local status=$?
        tail -40 "$out/$label.log"
        echo "$label FAIL=$status" | tee -a "$out/commands.log"
        exit "$status"
    fi
}
run ui cargo test --offline --locked -p rndr_ui --tests
run core cargo test --offline --locked -p rndr_core --test markdown --test layout --test text_engine --test editor --test editor_navigation --test editor_drag --test a1_interaction --test pane
run parser cargo test --offline --locked -p rndr_core --lib markdown
run highlight cargo test --offline --locked -p rndr_core --features highlight --test markdown
run paint cargo test --offline --locked -p rndr_paint --tests
run capture cargo test --offline --locked -p rndr_render --features observe --lib capture::tests
run gallery cargo build --offline --locked -p rndr --example markdown_gallery --features observe
run fallback cargo check --offline --locked -p rndr --example markdown_gallery --no-default-features
run docs cargo doc --offline --locked -p rndr_ui --no-deps
if [[ ${RNDR_CAPTURE:-0} == 1 ]]; then
    run light "$CARGO_TARGET_DIR/debug/examples/markdown_gallery" --complete --capture-prefix "$out/light"
    run dark-narrow "$CARGO_TARGET_DIR/debug/examples/markdown_gallery" --complete --dark --narrow --capture-prefix "$out/dark-narrow"
fi
# Passing tests/capture receipts do not imply pixels were viewed or native input
# was exercised. The caller must inspect PNGs separately before visual claims.
df -h . | tee "$out/disk-final.log"
