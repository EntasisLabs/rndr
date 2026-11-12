#!/bin/sh
# Run both byte-unchanged saved reviewer suites against current source.
set -eu
test "$(df -k . | awk 'NR==2 {print $4}')" -ge 4456448
one=crates/rndr_flow/tests/independent_review.rs
two=crates/rndr_flow/tests/independent_rereview.rs
test ! -e "$one" && test ! -e "$two"
trap 'rm -f "$one" "$two"' EXIT HUP INT TERM
cp docs/reports/rndr-flow-f4-independent-evidence/independent_review.rs "$one"
cp docs/reports/rndr-flow-f4-rereview-evidence/independent_rereview.rs "$two"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
cargo test --offline --locked -p rndr-flow --test independent_review --test independent_rereview -- --nocapture
