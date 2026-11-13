#!/bin/sh
set -eu
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
out=docs/reports/rndr-flow-f4-r3-independent-evidence
free=$(df -k . | awk 'NR==2 {print $4}')
test "$free" -ge 4456448 || exit 74
test ! -e crates/rndr_flow/tests/independent_r3.rs
cp "$out/independent_r3.rs" crates/rndr_flow/tests/independent_r3.rs
trap 'rm -f crates/rndr_flow/tests/independent_r3.rs' EXIT HUP INT TERM
cargo test --offline --locked -p rndr-flow --no-default-features --test independent_r3 -- --nocapture --test-threads=1
