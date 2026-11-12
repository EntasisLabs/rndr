#!/bin/sh
set -eu
candidate=9e09c575f222384d5e3c843511552f64d0cbbf4d
probe=crates/rndr_flow/tests/independent_review.rs
here=docs/reports/rndr-flow-f4-independent-evidence
# Run at candidate or its documentation-only review descendant; preserve source.
test ! -e "$probe"
test -z "$(git diff "$candidate" -- crates/rndr_flow crates/rndr_core Cargo.lock)"
free=$(df -k . | awk 'END {print $4}')
test "$free" -ge 4194304
trap 'rm -f "$probe"' EXIT HUP INT TERM
cp "$here/independent_review.rs" "$probe"
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 cargo test --offline --locked -p rndr-flow --test independent_review -- --nocapture
