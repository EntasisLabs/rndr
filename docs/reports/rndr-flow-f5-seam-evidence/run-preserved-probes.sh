#!/bin/sh
# Original desired safety assertions are never edited or weakened.
set -eu
source=${1:?exact qualified source commit required}
git diff --exit-code "$source" -- crates Cargo.toml Cargo.lock
p=crates/rndr_flow/tests/f5_seam_preserved_probe.rs
test ! -e "$p"; trap 'rm -f "$p"' EXIT HUP INT TERM
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
for original in \
 docs/reports/rndr-flow-f5-independent-evidence/adversarial.rs \
 docs/reports/rndr-flow-f5-rereview-evidence/additional.rs \
 docs/reports/rndr-flow-f5-rereview-evidence/seam-boundaries.rs
do
 free=$(df -k . | awk 'NR==2 {print $4}'); test "$free" -ge 4456448
 git diff --exit-code 9721657 -- "$original"
 echo "PRESERVED_PROBE=$original SOURCE=$source"
 shasum -a 256 "$original"
 cp "$original" "$p"; cmp "$original" "$p"
 cargo test --offline --locked -p rndr-flow --test f5_seam_preserved_probe -- --nocapture --test-threads=1
 rm "$p"
done
