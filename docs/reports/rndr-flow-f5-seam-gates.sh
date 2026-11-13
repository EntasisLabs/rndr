#!/bin/sh
# F5 authorized renderer seam: exact-source qualification, inherited gates preserved.
set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
source=$(git rev-parse HEAD)
out=${1:-docs/reports/rndr-flow-f5-seam-evidence/qualification}
mkdir -p "$out"
test ! -e "$out/summary.tsv" || { echo 'Refusing to overwrite evidence'; exit 73; }
printf 'gate\texit\tfree_kib_before\tfree_kib_after\n' > "$out/summary.tsv"
run() {
    label=$1; shift
    free=$(df -k . | awk 'NR==2 {print $4}')
    if [ "$free" -lt 4456448 ]; then echo "DISK BLOCKER before $label: $free KiB" >&2; exit 74; fi
    printf '$' > "$out/$label.txt"; printf ' %s' "$@" >> "$out/$label.txt"; printf '\n' >> "$out/$label.txt"
    "$@" >> "$out/$label.txt" 2>&1 & pid=$!
    (
        while kill -0 "$pid" 2>/dev/null; do
            now=$(df -k . | awk 'NR==2 {print $4}')
            if [ "$now" -lt 4456448 ]; then echo "DISK BLOCKER: $now KiB" > "$out/disk-blocker.txt"; kill -INT "$pid" 2>/dev/null || true; exit; fi
            sleep 1
        done
    ) & monitor=$!
    code=0; wait "$pid" || code=$?
    kill "$monitor" 2>/dev/null || true; wait "$monitor" 2>/dev/null || true
    after=$(df -k . | awk 'NR==2 {print $4}')
    printf '\nexit=%s\n' "$code" >> "$out/$label.txt"
    printf '%s\t%s\t%s\t%s\n' "$label" "$code" "$free" "$after" >> "$out/summary.tsv"
    echo "$label exit=$code free_kib=$after"
    test ! -e "$out/disk-blocker.txt" || exit 74
    if [ "$code" -ne 0 ]; then tail -70 "$out/$label.txt"; exit "$code"; fi
}
run environment sh -c 'git rev-parse HEAD HEAD^; git status --short; rustc -Vv; cargo -V; env | grep -E "CARGO_(INCREMENTAL|BUILD_JOBS|PROFILE|TARGET_DIR)"; df -k .; du -sk target'
run fmt cargo fmt --all --check
run whitespace git diff --check
run flow cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture
run host-adapter cargo test --offline --locked -p rndr-flow --test f5_native --test f5_admission --test f5_quit_save -- --nocapture
run reviewer-probes sh docs/reports/rndr-flow-f5-probes.sh
run preserved-f5-probes sh docs/reports/rndr-flow-f5-seam-evidence/run-preserved-probes.sh "$source"
run renderer cargo test --offline --locked -p rndr_render --no-default-features --lib -- --nocapture
run renderer-api cargo test --offline --locked -p rndr_render --no-default-features --test close_validator_api -- --nocapture
run renderer-default cargo check --offline --locked -p rndr_render --lib
run lint-renderer cargo clippy --offline --locked -p rndr_render --no-default-features --all-targets --no-deps -- -D warnings
run core cargo test --offline --locked -p rndr_core --no-default-features
run liquid cargo test --offline --locked -p rndr_liquid
run consumer cargo test --offline --locked -p rndr_liquid_ui -- --nocapture
run lint-flow cargo clippy --offline --locked -p rndr-flow --no-default-features --all-targets -- -D warnings
run lint-core cargo clippy --offline --locked -p rndr_core --no-default-features --lib --no-deps -- -D warnings
run lint-consumer cargo clippy --offline --locked -p rndr_liquid_ui --all-targets --no-deps -- -D warnings
run feature-off cargo check --offline --locked -p rndr-flow --no-default-features --lib
run facade-off cargo check --offline --locked -p rndr --no-default-features --lib
run facade-liquid cargo check --offline --locked -p rndr --no-default-features --features liquid --lib
run compatibility-off cargo test --offline --locked -p rndr --no-default-features --test editing_compatibility
run compatibility-liquid cargo test --offline --locked -p rndr --no-default-features --features liquid --test editing_compatibility
run flow-dependencies cargo tree --offline --locked -p rndr-flow --no-default-features -e normal,build
run reverse-dependencies cargo tree --offline --locked -i rndr-flow -e normal,build
run source-clean git diff --exit-code -- crates Cargo.toml Cargo.lock
run source-fingerprints sh -c 'git ls-files crates Cargo.toml Cargo.lock | grep -E "\.(rs|toml)$|^Cargo.lock$" | LC_ALL=C sort | xargs shasum -a 256'
# A standalone receipt, not just the shell command's redirected log.
sed '1d;$d;/^$/d' "$out/source-fingerprints.txt" > "$out/source-digests.txt"
git ls-files docs/reports/rndr-flow-f5-seam-audit.py docs/reports/rndr-flow-f5-seam-gates.sh docs/reports/rndr-flow-f5-seam-evidence/run-preserved-probes.sh docs/design/rndr-flow-f5-session-close-validation.md | LC_ALL=C sort | xargs shasum -a 256 > "$out/harness-digests.txt"
run audit python3 docs/reports/rndr-flow-f5-seam-audit.py "$out"
