#!/bin/sh
# Reuse unchanged independent assertions against the committed CI repair source.
set -eu
source=${1:?source commit required}
p=crates/rndr_flow/tests/f5_independent_seam_probe.rs
original=docs/reports/rndr-flow-f5-independent-seam-review-evidence/adversarial.rs
git diff --exit-code "$source" -- crates Cargo.toml Cargo.lock
git diff --exit-code 43a0e83394ac80a9ac35bcffde479bef4d71cfd1 -- "$original"
test ! -e "$p"; trap 'rm -f "$p"' EXIT HUP INT TERM
cp "$original" "$p"; cmp "$original" "$p"
shasum -a 256 "$original" "$p"
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cargo test --offline --locked -p rndr-flow --test f5_independent_seam_probe -- --nocapture --test-threads=1
