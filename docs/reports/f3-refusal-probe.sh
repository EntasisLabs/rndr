#!/bin/sh
# Isolate the unchanged reviewer regression; never overwrite candidate source.
# Run from the repository root after the scoped headless build has populated deps.
set -eu
export CARGO_INCREMENTAL=0
out=docs/reports/f3-refusal-evidence
private=target/f3-refusal/private
parent=b3bf25bb1095ada20694dbc28816934479b4f8a2
mkdir -p "$private"
core=$(ls -t target/debug/deps/librndr_core-*.rlib | head -1)
{
    printf 'parent=%s\n' "$parent"
    printf 'rustc --edition=2021 --test %s/lib.rs --extern rndr_core=%s -L dependency=target/debug/deps -o target/f3-refusal/private_probe\n' "$private" "$core"
    printf 'target/f3-refusal/private_probe extra_review_tests --nocapture\n'
    printf 'before expected=101; after expected=0\n'
} > "$out/private-command.log"
for name in transition.rs tests.rs budget_tests.rs; do
    git show "$parent:crates/rndr_flow/src/$name" > "$private/$name"
done
git show "$parent:crates/rndr_flow/src/lib.rs" > "$private/original.rs"
cat "$private/original.rs" "$out/reviewer-regression.rs" > "$private/lib.rs"
python3 - "$private" <<'PY'
import pathlib,sys
p=pathlib.Path(sys.argv[1])
original=(p/'original.rs').read_bytes()
regression=pathlib.Path('docs/reports/f3-refusal-evidence/reviewer-regression.rs').read_bytes()
assert (p/'lib.rs').read_bytes() == original + regression
print('Isolated source = exact b3bf25bb lib.rs + unchanged appended reviewer test')
PY
rustc --edition=2021 --test "$private/lib.rs" --extern rndr_core="$core" -L dependency=target/debug/deps -o target/f3-refusal/private_probe
set +e
target/f3-refusal/private_probe extra_review_tests --nocapture > "$out/private-before.log" 2>&1
before=$?
set -e
[ "$before" = 101 ]
cat crates/rndr_flow/src/lib.rs "$out/reviewer-regression.rs" > "$private/lib.rs"
cp crates/rndr_flow/src/lease_admission_tests.rs "$private/"
rustc --edition=2021 --test "$private/lib.rs" --extern rndr_core="$core" -L dependency=target/debug/deps -o target/f3-refusal/private_probe
target/f3-refusal/private_probe extra_review_tests --nocapture > "$out/private-after.log" 2>&1
printf 'Isolated regression: before=%s (expected 101), after=0\n' "$before"
