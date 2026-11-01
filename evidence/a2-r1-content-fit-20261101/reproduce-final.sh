#!/bin/sh
# Run from the exact tested implementation worktree. Creates only bounded temp
# direct-rustc output and reuses the existing Cargo target. No native launch.
set -eu
scripts=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
expected=91c008bdc1054f42f67dcd5d45729290ddb4ffa4
# The evidence-only child commit is allowed; relevant source must be identical.
git diff --exit-code "$expected" -- crates Cargo.toml Cargo.lock
python3 - <<'PY'
import shutil
assert shutil.disk_usage('.').free > 2*1024**3, 'Need >2 GiB for safe bounded rerun; stop, no cleanup.'
PY
out=$(mktemp -d /tmp/rndr-a2-content-fit-rerun-XXXXXX)
python3 "$scripts/validate-reuse.py" "$out"
sh "$scripts/verify-ui.sh" /tmp/rndr-a2-independent-SQwoYo/exact "$out/exact-ui-final"
sh "$scripts/verify-rest-v2.sh" /tmp/rndr-a2-independent-SQwoYo/exact "$out/exact-ui-final" "$out/rest-final"
sh "$scripts/run-fresh-probes.sh" /tmp/rndr-a2-independent-SQwoYo/exact /tmp/rndr-a2-fresh-review-4TfleE/exact-ui "$out/fresh-baseline-final" /tmp/rndr-a2-fresh-review-4TfleE
sh "$scripts/run-fresh-probes.sh" /tmp/rndr-a2-independent-SQwoYo/exact "$out/exact-ui-final" "$out/fresh-final-final" /tmp/rndr-a2-fresh-review-4TfleE
printf 'Evidence output: %s\n' "$out"
