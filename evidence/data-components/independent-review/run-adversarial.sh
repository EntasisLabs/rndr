#!/usr/bin/env bash
# Expected to FAIL against reviewed HEAD 2b2a42a; preserves a failing regression
# without permanently adding a failing test target to the shipped suite.
set -euo pipefail
cd "$(dirname "$0")/../../.."
source scripts/data-gallery-env.sh
fixture=crates/rndr_ui/tests/independent_review_adversarial.rs
if [ -e "$fixture" ]; then echo "Refusing to overwrite $fixture" >&2; exit 2; fi
cp evidence/data-components/independent-review/adversarial.rs "$fixture"
trap 'rm -f "$fixture"' EXIT
cargo test --offline --locked -p rndr_ui --test independent_review_adversarial -- --nocapture
