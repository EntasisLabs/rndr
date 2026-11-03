#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
fixture=crates/rndr_ui/tests/independent_rereview_policy.rs
if [ -e "$fixture" ]; then echo "Refusing to overwrite $fixture" >&2; exit 2; fi
cp evidence/data-components/independent-rereview/activation-policy.rs "$fixture"
trap 'rm -f "$fixture"' EXIT
cargo test --offline --locked -p rndr_ui --test independent_rereview_policy -- --nocapture
