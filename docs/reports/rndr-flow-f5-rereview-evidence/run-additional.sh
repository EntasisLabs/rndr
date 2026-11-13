#!/bin/sh
set -eu
source=3c59d5ba35ab78d9760f1cb2016d995af77d1823
p=crates/rndr_flow/tests/f5_independent_rereview.rs
git diff --exit-code "$source" -- crates Cargo.toml Cargo.lock
free=$(df -k . | awk 'NR==2 {print $4}'); test "$free" -ge 4456448
test ! -e "$p"; trap 'rm -f "$p"' EXIT HUP INT TERM
cp docs/reports/rndr-flow-f5-rereview-evidence/additional.rs "$p"
cmp docs/reports/rndr-flow-f5-rereview-evidence/additional.rs "$p"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --offline --locked -p rndr-flow --test f5_independent_rereview -- --nocapture --test-threads=1
