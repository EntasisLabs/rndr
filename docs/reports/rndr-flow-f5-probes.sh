#!/bin/sh
# Byte-unchanged accepted F4 adversarial suites; serial, shared target only.
set -eu
one=crates/rndr_flow/tests/independent_review.rs
two=crates/rndr_flow/tests/independent_rereview.rs
three=crates/rndr_flow/tests/independent_r3.rs
test ! -e "$one" && test ! -e "$two" && test ! -e "$three"
trap 'rm -f "$one" "$two" "$three"' EXIT HUP INT TERM
cp docs/reports/rndr-flow-f4-independent-evidence/independent_review.rs "$one"
cp docs/reports/rndr-flow-f4-rereview-evidence/independent_rereview.rs "$two"
cp docs/reports/rndr-flow-f4-r3-independent-evidence/independent_r3.rs "$three"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1
cargo test --offline --locked -p rndr-flow --test independent_review --test independent_rereview --test independent_r3 -- --nocapture
cargo test --offline --locked -p rndr-flow --test independent_r3 -- --nocapture
