#!/bin/sh
# Same saved repro bytes/assertions; only the source fence and temporary name differ.
set -eu
source=eed1878119f11ee66e73cdb0a6fe24a1027205eb
original=docs/reports/rndr-flow-f5-independent-evidence
p=crates/rndr_flow/tests/f5_independent_corrected_head.rs
git diff --exit-code "$source" -- crates Cargo.toml Cargo.lock
printf 'corrected_source=%s\nactual_HEAD=' "$source"; git rev-parse HEAD
shasum -a 256 "$original/adversarial.rs"
free=$(df -k . | awk 'NR==2 {print $4}')
test "$free" -ge 4456448 || { echo "DISK BLOCKER $free KiB"; exit 74; }
test ! -e "$p"
trap 'rm -f "$p"' EXIT HUP INT TERM
cp "$original/adversarial.rs" "$p"
cmp "$original/adversarial.rs" "$p"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --offline --locked -p rndr-flow --test f5_independent_corrected_head -- --nocapture --test-threads=1
