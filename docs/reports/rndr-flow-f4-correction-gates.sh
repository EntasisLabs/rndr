#!/bin/sh
# New correction evidence only. Optional destination enables exact-HEAD reruns.
set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
out=${1:-docs/reports/rndr-flow-f4-correction-evidence}
export out
mkdir -p "$out"
test ! -e "$out/summary.tsv" || { echo 'Refusing to overwrite evidence'; exit 73; }
printf 'gate\texit\tfree_kib_before\tfree_kib_after\n' > "$out/summary.tsv"
run() {
    label=$1; shift
    free=$(df -k . | awk 'NR==2 {print $4}')
    # A 256 MiB safety margin above the requested 4 GiB hard floor.
    if [ "$free" -lt 4456448 ]; then
        echo "DISK BLOCKER before $label: $free KiB; nothing deleted" >&2
        exit 74
    fi
    printf '$' > "$out/$label.txt"
    printf ' %s' "$@" >> "$out/$label.txt"
    printf '\n' >> "$out/$label.txt"
    "$@" >> "$out/$label.txt" 2>&1 &
    pid=$!
    (
        while kill -0 "$pid" 2>/dev/null; do
            now=$(df -k . | awk 'NR==2 {print $4}')
            if [ "$now" -lt 4456448 ]; then
                echo "DISK BLOCKER: $now KiB" > "$out/disk-blocker.txt"
                kill -INT "$pid" 2>/dev/null || true
                exit
            fi
            sleep 1
        done
    ) &
    monitor=$!
    code=0
    wait "$pid" || code=$?
    kill "$monitor" 2>/dev/null || true
    wait "$monitor" 2>/dev/null || true
    after=$(df -k . | awk 'NR==2 {print $4}')
    printf '\nexit=%s\n' "$code" >> "$out/$label.txt"
    printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$free" "$after" >> "$out/summary.tsv"
    echo "$label exit=$code free_kib=$after"
    test ! -e "$out/disk-blocker.txt" || exit 74
    if [ "$code" -ne 0 ]; then tail -60 "$out/$label.txt"; exit "$code"; fi
}
run environment sh -c 'git rev-parse HEAD HEAD^; git status --short; rustc --version; cargo --version; df -k .'
run fmt cargo fmt --all --check
run diff git diff --check
run flow cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture
run core cargo test --offline --locked -p rndr_core --no-default-features
run liquid cargo test --offline --locked -p rndr_liquid
run consumer cargo test --offline --locked -p rndr_liquid_ui -- --nocapture
run matrix-audit python3 docs/reports/rndr-flow-f4-correction-audit.py "$out"
run lint-flow cargo clippy --offline --locked -p rndr-flow --no-default-features --all-targets -- -D warnings
run lint-consumer cargo clippy --offline --locked -p rndr_liquid_ui --all-targets --no-deps -- -D warnings
run feature-off cargo check --offline --locked -p rndr-flow --no-default-features --lib
run facade-off cargo check --offline --locked -p rndr --no-default-features --lib
run facade-liquid cargo check --offline --locked -p rndr --no-default-features --features liquid --lib
run compatibility-off cargo test --offline --locked -p rndr --no-default-features --test editing_compatibility
run compatibility-liquid cargo test --offline --locked -p rndr --no-default-features --features liquid --test editing_compatibility
run flow-dependencies cargo tree --offline --locked -p rndr-flow --no-default-features -e normal,build
run reverse-dependencies cargo tree --offline --locked -i rndr-flow -e normal,build
run boundary sh -c 'git diff --exit-code f16e5572d35450d586dd4f10e00591fa36fb2b04 -- crates/rndr_core crates/rndr/src crates/rndr/Cargo.toml crates/rndr_liquid_ui/src; ! grep -E "tokio|rndr_ui|rndr_liquid|rndr_render|rndr_paint" "$out/flow-dependencies.txt"; ! grep -E "rndr_ui|rndr_liquid|rndr_render|rndr_core" "$out/reverse-dependencies.txt"; git diff --exit-code acaa7ede97f01791f19fe4629ad3e3157011aee4 -- docs/reports/rndr-flow-f4-evidence docs/reports/rndr-flow-f4-independent-evidence docs/reports/rndr-flow-f4-independent-review.md'
run whitespace git diff --check
run source-digests sh -c 'find crates/rndr_flow -type f \( -name "*.rs" -o -name Cargo.toml \) -print | sort | while IFS= read -r p; do shasum -a 256 "$p"; done; shasum -a 256 Cargo.lock crates/rndr_liquid_ui/Cargo.toml crates/rndr_liquid_ui/tests/flow_owned.rs'
run exact-source-audit python3 docs/reports/rndr-flow-f4-correction-audit.py "$out"
echo 'PASS: 21 bounded correction gates; worker evidence, not acceptance.'
