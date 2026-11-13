#!/bin/sh
# Headless/focused F4 qualification. Run from repository root; no new target tree.
set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
out=docs/reports/rndr-flow-f4-independent-evidence
mkdir -p "$out"
printf 'gate\texit\tfree_kib_before\tfree_kib_after\n' > "$out/summary.tsv"
run() {
    label=$1; shift
    free=$(df -k . | awk 'NR==2 {print $4}')
    if [ "$free" -lt 4194304 ]; then
        echo "DISK BLOCKER: $free KiB free before $label; require 4 GiB. Nothing deleted." >&2
        exit 74
    fi
    printf '$' > "$out/$label.txt"
    printf ' %s' "$@" >> "$out/$label.txt"
    printf '\n' >> "$out/$label.txt"
    code=0
    "$@" >> "$out/$label.txt" 2>&1 || code=$?
    after=$(df -k . | awk 'NR==2 {print $4}')
    printf '\nexit=%s\n' "$code" >> "$out/$label.txt"
    printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$free" "$after" >> "$out/summary.tsv"
    echo "$label exit=$code free_kib=$after"
    if [ "$code" -ne 0 ]; then tail -60 "$out/$label.txt"; exit "$code"; fi
}
run environment sh -c 'git rev-parse HEAD; rustc --version; cargo --version; df -h .'
run fmt cargo fmt --all --check
run diff git diff --check
run flow cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture
run core cargo test --offline --locked -p rndr_core --no-default-features
run liquid cargo test --offline --locked -p rndr_liquid
run consumer cargo test --offline --locked -p rndr_liquid_ui -- --nocapture
run matrix-audit python3 docs/reports/rndr-flow-f4-independent-evidence/audit.py
run lint-flow cargo clippy --offline --locked -p rndr-flow --no-default-features --all-targets -- -D warnings
run lint-consumer cargo clippy --offline --locked -p rndr_liquid_ui --all-targets --no-deps -- -D warnings
run feature-off cargo check --offline --locked -p rndr-flow --no-default-features --lib
run facade-off cargo check --offline --locked -p rndr --no-default-features --lib
run facade-liquid cargo check --offline --locked -p rndr --no-default-features --features liquid --lib
run compatibility-off cargo check --offline --locked -p rndr --no-default-features --test editing_compatibility
run compatibility-liquid cargo check --offline --locked -p rndr --no-default-features --features liquid --test editing_compatibility
run flow-dependencies cargo tree --offline --locked -p rndr-flow --no-default-features -e normal,build
run reverse-dependencies cargo tree --offline --locked -i rndr-flow -e normal,build
run boundary sh -c 'git diff --exit-code f16e5572d35450d586dd4f10e00591fa36fb2b04 -- crates/rndr_core crates/rndr/src crates/rndr/Cargo.toml crates/rndr_liquid_ui/src; ! grep -E "tokio|rndr_ui|rndr_liquid|rndr_render|rndr_paint" docs/reports/rndr-flow-f4-independent-evidence/flow-dependencies.txt; ! grep -E "rndr_ui|rndr_liquid|rndr_render|rndr_core" docs/reports/rndr-flow-f4-independent-evidence/reverse-dependencies.txt'
run whitespace git diff --check
run source-digests sh -c 'find crates/rndr_flow -type f \( -name "*.rs" -o -name Cargo.toml \) -print | sort | while IFS= read -r p; do shasum -a 256 "$p"; done; shasum -a 256 Cargo.lock crates/rndr_liquid_ui/Cargo.toml crates/rndr_liquid_ui/tests/flow_owned.rs'
echo 'PASS: all bounded F4 gates completed. This is worker evidence, not acceptance.'
