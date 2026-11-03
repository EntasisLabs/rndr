#!/usr/bin/env bash
set -u
cd "$(dirname "$0")/../../.."
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
out=evidence/selection-controls/corrections
run() {
  [ "$(df -Pk . | awk 'NR==2 {print $4}')" -ge 2097152 ] || return 2
  local name="$1"; shift
  printf '$ ' > "$out/$name.log"; printf '%q ' "$@" >> "$out/$name.log"; printf '\n' >> "$out/$name.log"
  "$@" >> "$out/$name.log" 2>&1; local rc=$?
  printf '\nEXIT=%s\n' "$rc" >> "$out/$name.log"
  echo "$name EXIT=$rc"; tail -4 "$out/$name.log"
  return "$rc"
}
run probes cargo test --offline --locked -p rndr_ui --test selection_independent_review --test selection_corrections || exit $?
run core-paint cargo test --offline --locked -p rndr_core -p rndr_paint || exit $?
run ui-focused cargo test --offline --locked -p rndr_ui --lib --test selection --test selection_scope --test selection_gallery --test data_editors --test data_editors_gallery --test field_transactions --test collections --test collection_chronology --test foundations --test malformed_lifecycle || exit $?
run native-observe cargo test --offline --locked -p rndr_render --lib --features observe || exit $?
{ git rev-parse HEAD; git status --short; df -h .; } > "$out/gate-completion.txt"
