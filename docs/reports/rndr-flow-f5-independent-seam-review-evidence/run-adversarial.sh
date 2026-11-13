#!/bin/sh
set -eu
base=8c337328ea733cda18a0f7215bfe379b986ea646
out=docs/reports/rndr-flow-f5-independent-seam-review-evidence
p=crates/rndr_flow/tests/f5_independent_seam_probe.rs
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
free=$(df -k . | awk 'NR==2 {print $4}'); test "$free" -ge 4456448
printf 'free_kib_before=%s\n' "$free"
git diff --exit-code "$base" -- crates Cargo.toml Cargo.lock
test ! -e "$p"; trap 'rm -f "$p"' EXIT HUP INT TERM
cp "$out/adversarial.rs" "$p"; cmp "$out/adversarial.rs" "$p"
shasum -a 256 "$out/adversarial.rs" "$p"
cargo test --offline --locked -p rndr-flow --test f5_independent_seam_probe -- --nocapture --test-threads=1 & pid=$!
(while kill -0 "$pid" 2>/dev/null; do
 now=$(df -k . | awk 'NR==2 {print $4}')
 if [ "$now" -lt 4456448 ]; then echo "DISK BLOCKER $now" > "$out/adversarial-disk-blocker.txt"; kill -INT "$pid" 2>/dev/null || true; exit; fi
 sleep 1
done) & monitor=$!
code=0; wait "$pid" || code=$?
kill "$monitor" 2>/dev/null || true; wait "$monitor" 2>/dev/null || true
printf 'exit=%s\n' "$code"; df -k .
test ! -e "$out/adversarial-disk-blocker.txt"
exit "$code"
