#!/usr/bin/env bash
# Fresh output only. Does not approve/integrate or alter implementation source.
set -euo pipefail
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
export CARGO_BUILD_JOBS=2
source scripts/data-gallery-env.sh
out="${1:?Supply a new output directory}"
if [ -e "$out" ]; then echo "Refusing existing output: $out" >&2; exit 2; fi
mkdir -p "$out"
bash scripts/verify-data-components.sh "$out/verification"
bash evidence/data-components/independent-review/run-adversarial.sh > "$out/adversarial-six.log" 2>&1
bash evidence/data-components/independent-rereview/run-activation-policy.sh > "$out/activation-policy.log" 2>&1
bash evidence/data-components/corrections/capture-native.sh "$out"
echo 'Fresh captures require actual image inspection; capture success alone is not review.'
