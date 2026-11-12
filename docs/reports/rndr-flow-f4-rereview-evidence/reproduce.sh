#!/bin/sh
set -eu
candidate=218e06c9998296a87f6393e182f03f9e40e97b92
probe=crates/rndr_flow/tests/independent_rereview.rs
here=docs/reports/rndr-flow-f4-rereview-evidence
test ! -e "$probe"
test -z "$(git diff "$candidate" -- crates Cargo.toml Cargo.lock)"
test "$(df -k . | awk 'NR==2 {print $4}')" -ge 4456448
trap 'rm -f "$probe"' EXIT HUP INT TERM
cp "$here/independent_rereview.rs" "$probe"
# Uses existing target, no source fix. Bounded test; expected exit 101, 2 pass/1 fail.
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked -p rndr-flow --test independent_rereview -- --nocapture
