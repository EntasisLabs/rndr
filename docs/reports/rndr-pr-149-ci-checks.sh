#!/bin/sh
# PR #149 CI repair: append-only receipts; reuse an explicitly selected target.
set -eu
: "${CARGO_TARGET_DIR:?select the existing shared Cargo target}"
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
out=${1:-docs/reports/rndr-pr-149-ci-evidence/checks}
mkdir -p "$out"
test ! -e "$out/summary.tsv" || exit 73
printf 'gate\texit\tfree_kib_before\tfree_kib_after\n' > "$out/summary.tsv"
failed=0
run() {
    label=$1; shift
    free=$(df -k . | awk 'NR==2 {print $4}')
    # Leave at least 3 GiB free. Do not clean shared or historical artifacts.
    test "$free" -ge 3145728 || { echo "disk blocker before $label"; exit 74; }
    { printf '$'; printf ' %s' "$@"; printf '\n'; } > "$out/$label.txt"
    "$@" >> "$out/$label.txt" 2>&1 & pid=$!
    (
        while kill -0 "$pid" 2>/dev/null; do
            now=$(df -k . | awk 'NR==2 {print $4}')
            if [ "$now" -lt 3145728 ]; then
                echo "disk blocker: $now KiB" > "$out/disk-blocker.txt"
                kill -INT "$pid" 2>/dev/null || true
                exit
            fi
            sleep 2
        done
    ) & monitor=$!
    code=0; wait "$pid" || code=$?
    kill "$monitor" 2>/dev/null || true; wait "$monitor" 2>/dev/null || true
    after=$(df -k . | awk 'NR==2 {print $4}')
    printf '\nexit=%s\n' "$code" >> "$out/$label.txt"
    printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$free" "$after" >> "$out/summary.tsv"
    echo "$label exit=$code free_kib=$after"
    test ! -e "$out/disk-blocker.txt" || exit 74
    # Keep exploring subsequent configurations, but fail the overall driver.
    if [ "$code" -ne 0 ]; then failed=1; fi
}
run environment sh -ec 'git diff --exit-code HEAD -- crates Cargo.toml Cargo.lock; git rev-parse HEAD; rustc -Vv; cargo -V; env | grep -E "CARGO_(INCREMENTAL|BUILD_JOBS|PROFILE|TARGET_DIR)"; df -k .'
run fmt cargo fmt --check
run clippy-workspace cargo clippy --offline --locked --workspace --all-targets -- -D warnings
run clippy-no-default cargo clippy --offline --locked -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings
run clippy-dialog cargo clippy --offline --locked -p rndr_render -p rndr --features dialog --all-targets -- -D warnings
run clippy-wasm cargo clippy --locked -p rndr_render -p rndr -p web-hello --target wasm32-unknown-unknown -- -D warnings
run clippy-ui cargo clippy --offline --locked -p rndr --no-default-features --features ui --all-targets -- -D warnings
run clippy-liquid-canvas cargo clippy --offline --locked -p rndr --no-default-features --features liquid,canvas --all-targets -- -D warnings
run clippy-tokio cargo clippy --offline --locked -p rndr_core --features tokio --all-targets -- -D warnings
run core cargo test --offline --locked -p rndr_core --no-default-features
run flow cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture
run consumer cargo test --offline --locked -p rndr_liquid_ui -- --nocapture
run renderer-close cargo test --offline --locked -p rndr_render --no-default-features --lib
run renderer-api cargo test --offline --locked -p rndr_render --no-default-features --test close_validator_api
run reviewer-probes sh docs/reports/rndr-flow-f5-probes.sh
run preserved-probes sh docs/reports/rndr-flow-f5-seam-evidence/run-preserved-probes.sh "$(git rev-parse HEAD)"
run independent-seam sh docs/reports/rndr-pr-149-ci-seam-probes.sh "$(git rev-parse HEAD)"
test "$failed" -eq 0
