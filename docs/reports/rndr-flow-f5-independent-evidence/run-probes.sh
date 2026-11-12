#!/bin/sh
set -eu
out=docs/reports/rndr-flow-f5-independent-evidence
p=crates/rndr_flow/tests/f5_independent_review.rs
git diff --exit-code 286818f9bcac048b725a650594ea88fa9b1d1624 -- crates Cargo.toml Cargo.lock
printf "review_HEAD="; git rev-parse HEAD
shasum -a 256 "$out/adversarial.rs"
free=$(df -k . | awk 'NR==2 {print $4}')
test "$free" -ge 4456448 || { echo "DISK BLOCKER $free KiB"; exit 74; }
test ! -e "$p"
trap 'rm -f "$p"' EXIT HUP INT TERM
cp "$out/adversarial.rs" "$p"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --offline --locked -p rndr-flow --test f5_independent_review -- --nocapture --test-threads=1
