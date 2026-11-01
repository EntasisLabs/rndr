#!/bin/sh
# Run from the reviewed worktree, offline with existing dependency rlibs.
# Refuses any existing output. No implementation/source or Cargo target edits.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=${1:?supply nonexistent output directory}
test ! -e "$out" || exit 2
mkdir -p "$out"
exec > "$out/transcript.log" 2>&1
set -x
test "$(git rev-parse HEAD)" = 04642fe03f7603f228eaf9b83611641a95676e8d
test -z "$(git status --porcelain)"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
rustc -Vv
shasum -a 256 target/debug/deps/librndr_core-99fc9f1d1448f44b.rlib target/debug/deps/libpulldown_cmark-effc4bf9c02010fd.rlib target/debug/deps/libunicode_segmentation-1422367974f5ae97.rlib > "$out/explicit-input-libraries.txt"
git ls-files -z crates/rndr_core/src | xargs -0 shasum -a 256 > "$out/current-core-sources.txt"
cmp "$out/current-core-sources.txt" evidence/a1-ownership-20261101-2t23Vj/candidate-all-source-hashes.txt
rustc --edition=2021 --crate-name rndr_core --crate-type rlib --cfg 'feature="default"' crates/rndr_core/src/lib.rs -L dependency=target/debug/deps --extern pulldown_cmark=target/debug/deps/libpulldown_cmark-effc4bf9c02010fd.rlib --extern unicode_segmentation=target/debug/deps/libunicode_segmentation-1422367974f5ae97.rlib -C debuginfo=0 -o "$out/librndr_core.rlib"
printf 'direct-library exit=0\n' > "$out/status.txt"
for name in ownership prior14 original4 independent; do
 case "$name" in
 ownership) fixture=crates/rndr_core/tests/a1_hotkey_ownership.rs;;
 prior14) fixture=crates/rndr_core/tests/a1_review_corrections.rs;;
 original4) fixture=evidence/a1-ownership-20261101-2t23Vj/reviewer/adversarial.rs;;
 independent) fixture=$root/independent.rs;;
 esac
 cp "$fixture" "$out/$name.rs"
 rustc --edition=2021 --test "$out/$name.rs" -L dependency=target/debug/deps --extern "rndr_core=$out/librndr_core.rlib" -C debuginfo=0 -o "$out/$name" > "$out/$name-build.log" 2>&1
 printf '%s-build exit=0\n' "$name" >> "$out/status.txt"
 "$out/$name" --nocapture > "$out/$name.log" 2>&1
 printf '%s-test exit=0\n' "$name" >> "$out/status.txt"
done
shasum -a 256 "$out/librndr_core.rlib" > "$out/direct-library-hash.txt"
test -z "$(git status --porcelain)"
git rev-parse HEAD
