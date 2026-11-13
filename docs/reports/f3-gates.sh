#!/bin/sh
# Run from the undertaking root. Logs stay in the admitted checkout.
set -u
out=docs/reports/f3-evidence
mkdir -p "$out"
: > "$out/gates.tsv"
run() {
    name=$1
    shift
    printf '\nRUN %s: %s\n' "$name" "$*"
    "$@" > "$out/$name.log" 2>&1
    result=$?
    printf '%s\t%s\t%s\n' "$name" "$result" "$*" >> "$out/gates.tsv"
    printf 'RESULT %s: %s\n' "$name" "$result"
}
run flow-test cargo test --locked -p rndr-flow --no-default-features
run cycles cargo test --locked -p rndr-flow --no-default-features one_hundred_lifecycle_cycles -- --nocapture
run headless cargo build --locked -p rndr-flow --no-default-features
run dependency-tree cargo tree --locked -p rndr-flow --no-default-features -e normal,build
run dependency-inverse cargo tree --locked -i rndr-flow
run fmt cargo fmt --all --check
run flow-clippy cargo clippy --locked -p rndr-flow --no-default-features --all-targets -- -D warnings
run liquid cargo test --locked -p rndr_liquid -p rndr_liquid_ui -- --nocapture
run core-ui-paint-render cargo test --locked -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render
run source-session cargo test --locked -p source-session
run contributor-clippy cargo clippy --locked -p rndr_core -p rndr_ui -p rndr_paint -p rndr_render -p source-session --lib --bins -- -D warnings
run workspace-check cargo check --locked --workspace
run feature-off cargo check --locked -p rndr_core -p rndr_ui -p rndr_paint --no-default-features
run editing-off cargo test --locked -p rndr --no-default-features --test editing_compatibility
run editing-ui cargo test --locked -p rndr --no-default-features --features ui --test editing_compatibility
run editing-liquid cargo test --locked -p rndr --no-default-features --features liquid --test editing_compatibility
run docs env RUSTDOCFLAGS=-Dwarnings cargo doc --locked -p rndr-flow -p rndr_core -p rndr_ui --no-deps
run ci-clippy cargo clippy --locked --workspace --all-targets -- -D warnings
run ci-feature-off-clippy cargo clippy --locked -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings
run facade-off cargo test --locked -p rndr --no-default-features --lib
run facade-ui cargo test --locked -p rndr --no-default-features --features ui --lib
run facade-ui-doc cargo test --locked -p rndr --no-default-features --features ui --doc
run facade-liquid-canvas cargo test --locked -p rndr --no-default-features --features liquid,canvas --lib
run diff-check git diff --check
cat "$out/gates.tsv"
