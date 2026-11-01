#!/bin/sh
# Run from worktree root. Output must not exist. Never cleans Cargo targets.
set -eu
out=${1:?usage: sh evidence/a1-ownership-20261101-2t23Vj/run.sh NEW_OUTPUT}
test ! -e "$out" || { echo 'Refusing existing output' >&2; exit 2; }
mkdir -p "$out"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
base=01da8acdc57240cd536234d062933e232b033eec
here=evidence/a1-ownership-20261101-2t23Vj
build=$(mktemp -d /tmp/rndr-a1-ownership-verify-XXXXXX)
printf 'base=%s\nworktree=%s\nbuild=%s\njobs=2 dev/test debug=0 incremental=0 offline locked existing target\n' "$base" "$PWD" "$build" > "$out/identity.txt"
git rev-parse HEAD >> "$out/identity.txt"
git status --short >> "$out/identity.txt"
rustc -Vv >> "$out/identity.txt"
df -h . >> "$out/identity.txt"
du -sh target >> "$out/identity.txt"
record() {
    label=$1; shift
    printf '%s:' "$label" >> "$out/status.txt"
    printf ' %s' "$@" >> "$out/status.txt"
    if "$@" > "$out/$label.log" 2>&1; then result=0; else result=$?; fi
    printf ' exit=%s\n' "$result" >> "$out/status.txt"
    return "$result"
}
# initial-focused.log records actual candidate rustc invocation; validate its
# source/artifact hashes below, even if this repeat suite sees Fresh artifacts.
# For any source change, establish a new actual build identity independently.
record core cargo test --offline --locked -p rndr_core -v
# initial-focused.log records the source-changing rustc invocation on this run;
# later repeat runs must independently establish build identity themselves.
rlib=target/debug/deps/librndr_core-99fc9f1d1448f44b.rlib
shasum -a 256 "$rlib" > "$out/candidate-rlib.txt"
git ls-files -z crates/rndr_core/src | xargs -0 shasum -a 256 > "$out/candidate-sources.txt"
cmp "$here/candidate-all-source-hashes.txt" "$out/candidate-sources.txt"
cmp "$here/candidate-rlib-hash.txt" "$out/candidate-rlib.txt"
grep -q 'Running.*rustc --crate-name rndr_core ' "$here/initial-focused.log"
cp crates/rndr_core/tests/a1_hotkey_ownership.rs "$out/fixture.rs"
cmp "$out/fixture.rs" "$here/fixture.rs"
git archive "$base" crates/rndr_core/src | tar -x -C "$build"
find "$build/crates/rndr_core/src" -type f -exec shasum -a 256 {} \; | sort > "$out/starting-sources.txt"
# Direct rustc: no Cargo workspace/snapshot fingerprint involved, no second target.
record original-library rustc --edition=2021 --crate-name rndr_core --crate-type rlib --cfg 'feature="default"' "$build/crates/rndr_core/src/lib.rs" -L dependency=target/debug/deps --extern pulldown_cmark=target/debug/deps/libpulldown_cmark-effc4bf9c02010fd.rlib --extern unicode_segmentation=target/debug/deps/libunicode_segmentation-1422367974f5ae97.rlib -C debuginfo=0 -o "$build/librndr_core.rlib"
shasum -a 256 "$build/librndr_core.rlib" >> "$out/identity.txt"
for variant in starting candidate; do
    lib=$rlib
    if test "$variant" = starting; then lib=$build/librndr_core.rlib; fi
    record "$variant-fixture-build" rustc --edition=2021 --test "$out/fixture.rs" -L dependency=target/debug/deps --extern "rndr_core=$lib" -C debuginfo=0 -o "$build/$variant-fixture"
    if record "$variant-fixture" "$build/$variant-fixture" --nocapture; then
        test "$variant" = candidate
    else
        test "$variant" = starting
        grep -q '6 passed; 5 failed' "$out/starting-fixture.log"
    fi
done
# Unchanged independent repro (two failures on starting, all four pass candidate).
record adversarial-build rustc --edition=2021 --test "$here/reviewer/adversarial.rs" -L dependency=target/debug/deps --extern "rndr_core=$rlib" -C debuginfo=0 -o "$build/adversarial"
record adversarial "$build/adversarial" --nocapture
for source in probe mixed; do
    record "$source-build" rustc --edition=2021 "$here/$source.rs" -L dependency=target/debug/deps --extern "rndr_core=$rlib" -C debuginfo=0 -o "$build/$source"
    record "$source" "$build/$source"
done
record paint cargo test --offline --locked -p rndr_paint
record ui cargo test --offline --locked -p rndr_ui
record accesskit cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests
record umbrella cargo check --offline --locked -p rndr --all-targets --no-default-features
record tree cargo tree --offline --locked -p rndr_ui --edges normal
if grep -E 'wgpu|winit|rndr_render|rndr_paint|rndr_app' "$out/tree.log"; then exit 1; fi
record gallery cargo run --offline --locked -p rndr_ui --example passive_gallery -- "$out/gallery"
record gallery-diff diff -rq evidence/ui-gallery "$out/gallery"
record recovery-hashes shasum -a 256 -c evidence/recovery/logs-sha256.txt
record gallery-hashes shasum -a 256 -c evidence/ui-gallery-sha256.txt
record correction-hashes shasum -a 256 -c evidence/a1-corrections-20261101/files-sha256.txt
record archived-diff git diff --exit-code "$base" -- evidence/recovery evidence/a1-corrections-20261101 'evidence/ui-*'
record format rustfmt --edition 2021 --check crates/rndr_core/src/hotkey/registry.rs crates/rndr_core/src/interaction.rs crates/rndr_core/tests/a1_review_corrections.rs crates/rndr_core/tests/a1_hotkey_ownership.rs
shasum -a 256 "$rlib" > "$out/candidate-rlib-after.txt"
cmp "$out/candidate-rlib.txt" "$out/candidate-rlib-after.txt"
df -h . >> "$out/identity.txt"
du -sh target >> "$out/identity.txt"
cat "$out/status.txt"
