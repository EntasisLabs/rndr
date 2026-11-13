#!/bin/sh
# Bounded correction qualification; reuse the original driver's monitored run().
set -eu
: "${CARGO_TARGET_DIR:?select the existing shared target}"
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
out=docs/reports/rndr-pr-149-ci-correction/checks
mkdir -p "$out"
test ! -e "$out/summary.tsv" || exit 73
# Only load definitions, never execute the original broad gate list.
functions="$out/run-functions.sh"
sed -n '/^failed=0$/,/^run environment/{ /^run environment/!p; }' docs/reports/rndr-pr-149-ci-checks.sh > "$functions"
. "./$functions"
printf 'gate\texit\tfree_kib_before\tfree_kib_after\n' > "$out/summary.tsv"
run environment sh -ec 'git diff --exit-code HEAD -- crates Cargo.toml Cargo.lock; git rev-parse HEAD; rustc -Vv; cargo -V; cargo clippy -V; env | grep -E "CARGO_(INCREMENTAL|BUILD_JOBS|PROFILE|TARGET_DIR)"; df -k .'
run fmt cargo fmt --check
run clippy-workspace cargo clippy --offline --locked --workspace --all-targets -- -D warnings
run clippy-no-default cargo clippy --offline --locked -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings
run clippy-dialog cargo clippy --offline --locked -p rndr_render -p rndr --features dialog --all-targets -- -D warnings
run clippy-wasm cargo clippy --offline --locked -p rndr_render -p rndr -p web-hello --target wasm32-unknown-unknown -- -D warnings
run clippy-ui cargo clippy --offline --locked -p rndr --no-default-features --features ui --all-targets -- -D warnings
run clippy-liquid-canvas cargo clippy --offline --locked -p rndr --no-default-features --features liquid,canvas --all-targets -- -D warnings
run clippy-tokio cargo clippy --offline --locked -p rndr_core --features tokio --all-targets -- -D warnings
run flow cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture
run consumer cargo test --offline --locked -p rndr_liquid_ui -- --nocapture
run core-freeze cargo test --offline --locked -p rndr_core --no-default-features --lib freeze::tests
run composer cargo test --offline --locked -p rndr_core --no-default-features --test structured_composer
run historical-audit python3 docs/reports/rndr-pr-149-ci-audit.py
test "$failed" -eq 0
