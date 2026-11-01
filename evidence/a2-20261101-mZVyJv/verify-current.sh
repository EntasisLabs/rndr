#!/bin/sh
# Run from undertaking checkout. Refuses an existing output, never cleans a target.
# Reuses only explicit third-party rlibs; every rndr rlib below is current-source.
set -eu
out=${1:?supply NEW absolute output directory}
case "$out" in /*) ;; *) echo 'absolute output required' >&2; exit 2;; esac
test ! -e "$out" || { echo 'refuse overwrite' >&2; exit 2; }
free=$(df -Pk . | tail -1 | awk '{print $4}')
test "$free" -gt 524288 || { echo 'BLOCKER: need >512MiB free for bounded direct rebuild; preserve work' >&2; exit 3; }
mkdir -p "$out/current" "$out/baseline" "$out/bin" "$out/logs"
exec > "$out/transcript.log" 2>&1
set -x
root=$(pwd)
evidence="$root/evidence/a2-20261101-mZVyJv"
deps="$root/target/debug/deps"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_PKG_VERSION=0.3.1 INSTA_UPDATE=no INSTA_WORKSPACE_ROOT="$root"
git diff --exit-code HEAD -- crates/rndr_core crates/rndr_paint crates/rndr_ui crates/rndr_render crates/rndr Cargo.lock
rustc -Vv
git rev-parse HEAD > "$out/implementation-head.txt"
shasum -a 256 "$0" "$evidence/tab-release-repro.rs" > "$out/runner-inputs-sha256.txt"
# Explicit inspected third-party artifact identities; no guessing a rndr artifact.
cmark="$deps/libpulldown_cmark-effc4bf9c02010fd.rlib"
unicode="$deps/libunicode_segmentation-1422367974f5ae97.rlib"
lyon="$deps/liblyon-37185680a94e4faa.rlib"
insta="$deps/libinsta-ceeb3b5188bdb88b.rlib"
accesskit="$deps/libaccesskit-46f958fdc5b7a499.rlib"
shasum -a 256 "$cmark" "$unicode" "$lyon" "$insta" "$accesskit" > "$out/dependency-rlibs-sha256.txt"
hash_sources() { git ls-files -z crates/rndr_core crates/rndr_paint crates/rndr_ui crates/rndr_render/src/a11y.rs crates/rndr/examples/ui_gallery.rs crates/rndr/Cargo.toml Cargo.lock | xargs -0 shasum -a 256; }
hash_sources > "$out/source-before-sha256.txt"
core() { CARGO_MANIFEST_DIR="$root/crates/rndr_core" CARGO_PKG_NAME=rndr_core rustc --edition=2021 --cfg 'feature="default"' -C debuginfo=0 -C metadata=a2_current_exact -L "dependency=$out/current" -L "dependency=$deps" --extern "pulldown_cmark=$cmark" --extern "unicode_segmentation=$unicode" "$@"; }
paint() { CARGO_MANIFEST_DIR="$root/crates/rndr_paint" CARGO_PKG_NAME=rndr_paint rustc --edition=2021 -C debuginfo=0 -C metadata=a2_current_exact -L "dependency=$out/current" -L "dependency=$deps" --extern "rndr_core=$out/current/librndr_core.rlib" --extern "lyon=$lyon" --extern "insta=$insta" "$@"; }
ui() { CARGO_MANIFEST_DIR="$root/crates/rndr_ui" CARGO_PKG_NAME=rndr_ui rustc --edition=2021 -C debuginfo=0 -C metadata=a2_current_exact -L "dependency=$out/current" -L "dependency=$deps" --extern "rndr_core=$out/current/librndr_core.rlib" --extern "rndr_paint=$out/current/librndr_paint.rlib" --extern "insta=$insta" "$@"; }
core --crate-name rndr_core --crate-type rlib crates/rndr_core/src/lib.rs -o "$out/current/librndr_core.rlib"
paint --crate-name rndr_paint --crate-type rlib crates/rndr_paint/src/lib.rs -o "$out/current/librndr_paint.rlib"
ui --crate-name rndr_ui --crate-type rlib crates/rndr_ui/src/lib.rs -o "$out/current/librndr_ui.rlib"
shasum -a 256 "$out/current/"*.rlib > "$out/current-rlibs-sha256.txt"
core --test --crate-name rndr_core crates/rndr_core/src/lib.rs -o "$out/bin/core-unit"
"$out/bin/core-unit" > "$out/logs/core-unit.log" 2>&1
for f in crates/rndr_core/tests/*.rs; do
 name=$(basename "$f" .rs)
 core --test --crate-name "$name" "$f" --extern "rndr_core=$out/current/librndr_core.rlib" -o "$out/bin/core-$name"
 "$out/bin/core-$name" > "$out/logs/core-$name.log" 2>&1
done
paint --test --crate-name rndr_paint crates/rndr_paint/src/lib.rs -o "$out/bin/paint-unit"
"$out/bin/paint-unit" > "$out/logs/paint-unit.log" 2>&1
for f in crates/rndr_paint/tests/*.rs; do
 name=$(basename "$f" .rs)
 paint --test --crate-name "$name" "$f" --extern "rndr_paint=$out/current/librndr_paint.rlib" -o "$out/bin/paint-$name"
 "$out/bin/paint-$name" > "$out/logs/paint-$name.log" 2>&1
done
ui --test --crate-name rndr_ui crates/rndr_ui/src/lib.rs -o "$out/bin/ui-unit"
"$out/bin/ui-unit" > "$out/logs/ui-unit.log" 2>&1
for f in crates/rndr_ui/tests/*.rs; do
 name=$(basename "$f" .rs)
 ui --test --crate-name "$name" "$f" --extern "rndr_ui=$out/current/librndr_ui.rlib" -o "$out/bin/ui-$name"
 "$out/bin/ui-$name" > "$out/logs/ui-$name.log" 2>&1
done
# Compile the ACTUAL adapter module, not a reimplementation or World-only test.
printf '#[path = "%s/crates/rndr_render/src/a11y.rs"] mod actual_adapter;\n' "$root" > "$out/adapter-wrapper.rs"
rustc --edition=2021 --test --crate-name actual_adapter "$out/adapter-wrapper.rs" -C debuginfo=0 -L "dependency=$out/current" -L "dependency=$deps" --extern "rndr_core=$out/current/librndr_core.rlib" --extern "accesskit=$accesskit" -o "$out/bin/actual-adapter"
"$out/bin/actual-adapter" > "$out/logs/actual-adapter.log" 2>&1
for name in independent original4 tab_release; do
 case "$name" in
 independent) f="$evidence/accepted-a1-review/independent.rs";;
 original4) f=evidence/a1-ownership-20261101-2t23Vj/reviewer/adversarial.rs;;
 tab_release) f="$evidence/tab-release-repro.rs";;
 esac
 core --test --crate-name "$name" "$f" --extern "rndr_core=$out/current/librndr_core.rlib" -o "$out/bin/probe-$name"
 "$out/bin/probe-$name" > "$out/logs/probe-$name.log" 2>&1
done
ui --crate-name operating_gallery crates/rndr_ui/examples/operating_gallery.rs --extern "rndr_ui=$out/current/librndr_ui.rlib" -o "$out/bin/operating-gallery"
"$out/bin/operating-gallery" "$out/direct-gallery"
# Reconstruct exact accepted A1 source, verify archive bytes against Git objects.
git archive 04642fe03f7603f228eaf9b83611641a95676e8d crates/rndr_core | tar -xf - -C "$out/baseline"
python3 - "$out" <<'PY'
import sys,subprocess,pathlib,hashlib
out=pathlib.Path(sys.argv[1]);base=out/'baseline';rows=[]
for name in subprocess.check_output(['git','ls-tree','-rz','--name-only','04642fe03f7603f228eaf9b83611641a95676e8d','crates/rndr_core']).decode().split('\0'):
 if not name:continue
 expected=subprocess.check_output(['git','show','04642fe03f7603f228eaf9b83611641a95676e8d:'+name]);actual=(base/name).read_bytes();assert actual==expected,name;rows.append(hashlib.sha256(actual).hexdigest()+' '+name)
(out/'baseline-source-sha256.txt').write_text('\n'.join(rows)+'\n')
PY
CARGO_MANIFEST_DIR="$out/baseline/crates/rndr_core" CARGO_PKG_NAME=rndr_core rustc --edition=2021 --cfg 'feature="default"' --crate-name rndr_core --crate-type rlib "$out/baseline/crates/rndr_core/src/lib.rs" -C debuginfo=0 -C metadata=a1_baseline_exact -L "dependency=$deps" --extern "pulldown_cmark=$cmark" --extern "unicode_segmentation=$unicode" -o "$out/baseline/librndr_core.rlib"
for name in tab_release nested_focus; do
 case "$name" in tab_release) f="$evidence/tab-release-repro.rs";; nested_focus) f=crates/rndr_core/tests/a2_nested_focus.rs;; esac
 rustc --edition=2021 --test --crate-name "$name" "$f" -C debuginfo=0 -L "dependency=$out/baseline" -L "dependency=$deps" --extern "rndr_core=$out/baseline/librndr_core.rlib" -o "$out/bin/baseline-$name"
 if "$out/bin/baseline-$name" > "$out/logs/baseline-$name.expected-failure.log" 2>&1; then echo 'UNEXPECTED BASELINE PASS';exit 1;else code=$?;test "$code" = 101;fi
done
grep -q 'must select that tab' "$out/logs/baseline-tab_release.expected-failure.log"
grep -q 'outer ancestor must also reveal target' "$out/logs/baseline-nested_focus.expected-failure.log"
shasum -a 256 "$out/baseline/librndr_core.rlib" "$out/bin/"* > "$out/binary-identities-sha256.txt"
hash_sources > "$out/source-after-sha256.txt"
cmp "$out/source-before-sha256.txt" "$out/source-after-sha256.txt"
python3 - "$out" <<'PY'
import pathlib,re,sys
out=pathlib.Path(sys.argv[1]);counts={}
for f in sorted((out/'logs').glob('*.log')):
 if 'expected-failure' in f.name:continue
 text=f.read_text();m=re.findall(r'test result: ok\. (\d+) passed;',text);assert m,f;group=f.name.split('-')[0];counts[group]=counts.get(group,0)+sum(map(int,m))
(out/'direct-test-counts.txt').write_text('\n'.join(f'{k}: {v}' for k,v in counts.items())+'\nCargo separately verifies9core+1paint doctests. Two exact A1 baseline reproductions fail as expected.\n')
PY
printf 'ALL DIRECT CURRENT-SOURCE SUITES AND EXPECTED BASELINE REPRODUCTIONS PASS\n' > "$out/status.txt"
