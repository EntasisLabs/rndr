#!/bin/sh
# Expected exit101: two known missing-seam desired safety assertions, unchanged private runner.
set -eu
p=crates/rndr_flow/tests/f5_independent_seam_boundaries.rs
git diff --exit-code 3c59d5ba35ab78d9760f1cb2016d995af77d1823 -- crates Cargo.toml Cargo.lock
free=$(df -k . | awk 'NR==2 {print $4}'); test "$free" -ge 4456448
test ! -e "$p"; trap 'rm -f "$p"' EXIT HUP INT TERM
cp docs/reports/rndr-flow-f5-rereview-evidence/seam-boundaries.rs "$p"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --offline --locked -p rndr-flow --test f5_independent_seam_boundaries -- --nocapture --test-threads=1
