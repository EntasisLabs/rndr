#!/bin/sh
# Exact current UI rebuild/tests using already source-verified, unchanged core/paint.
# No Cargo target or cleanup. Both inputs/output are explicit absolute paths.
set -eu
base=${1:?exact full-source receipt directory};out=${2:?NEW absolute output}
case "$out" in /*) ;; *) exit 2;; esac
test ! -e "$out" || { echo 'refuse overwrite' >&2; exit 2; }
test -f "$base/status.txt";test -f "$base/current/librndr_core.rlib";test -f "$base/current/librndr_paint.rlib"
free=$(df -Pk . | tail -1 | awk '{print $4}');test "$free" -gt 524288 || { echo 'BLOCKER: need >512MiB free for bounded current UI verification' >&2;exit 3; }
mkdir -p "$out/current" "$out/bin" "$out/logs"
exec > "$out/transcript.log" 2>&1
set -x
root=$(pwd);deps="$root/target/debug/deps";evidence="$root/evidence/a2-20261101-mZVyJv"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_PKG_VERSION=0.3.1 CARGO_PKG_NAME=rndr_ui CARGO_MANIFEST_DIR="$root/crates/rndr_ui"
export INSTA_UPDATE=no INSTA_WORKSPACE_ROOT="$root"
git diff --stat -- crates Cargo.lock
git rev-parse HEAD > "$out/implementation-head.txt"
rustc -Vv
shasum -a 256 "$0" > "$out/runner-sha256.txt"
# Reuse exact source rebuilds, NOT cached Cargo rndr libraries. Verify every
# non-UI source/test/Cargo/adapter byte against the completed full-source receipt.
python3 - "$base" "$out" <<'PY'
import pathlib,sys,hashlib
base,out=map(pathlib.Path,sys.argv[1:]);rows=[]
for line in (base/'source-before-sha256.txt').read_text().splitlines():
 expected,name=line.split(maxsplit=1)
 if name.startswith('crates/rndr_ui/'):continue
 actual=hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest();assert actual==expected,name;rows.append(line)
(out/'unchanged-core-paint-adapter-inputs-sha256.txt').write_text('\n'.join(rows)+'\n')
for line in (base/'current-rlibs-sha256.txt').read_text().splitlines():
 expected,name=line.split(maxsplit=1);assert hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest()==expected,name
PY
hash_ui() { find crates/rndr_ui -type f | LC_ALL=C sort | xargs shasum -a 256; }
hash_ui > "$out/ui-source-before-sha256.txt"
ui() { rustc --edition=2021 -C debuginfo=0 -C metadata=a2_handoff_ui_exact -L "dependency=$out/current" -L "dependency=$base/current" -L "dependency=$deps" --extern "rndr_core=$base/current/librndr_core.rlib" --extern "rndr_paint=$base/current/librndr_paint.rlib" --extern "insta=$deps/libinsta-ceeb3b5188bdb88b.rlib" "$@"; }
ui --crate-name rndr_ui --crate-type rlib crates/rndr_ui/src/lib.rs -o "$out/current/librndr_ui.rlib"
shasum -a 256 "$base/current/librndr_core.rlib" "$base/current/librndr_paint.rlib" "$out/current/librndr_ui.rlib" > "$out/handoff-rlibs-sha256.txt"
ui --test --crate-name rndr_ui crates/rndr_ui/src/lib.rs -o "$out/bin/ui-unit"
"$out/bin/ui-unit" > "$out/logs/ui-unit.log" 2>&1
for f in crates/rndr_ui/tests/*.rs; do
 name=$(basename "$f" .rs)
 ui --test --crate-name "$name" "$f" --extern "rndr_ui=$out/current/librndr_ui.rlib" -o "$out/bin/ui-$name"
 "$out/bin/ui-$name" > "$out/logs/ui-$name.log" 2>&1
done
ui --crate-name operating_gallery crates/rndr_ui/examples/operating_gallery.rs --extern "rndr_ui=$out/current/librndr_ui.rlib" -o "$out/bin/operating-gallery"
"$out/bin/operating-gallery" "$out/gallery-one"
"$out/bin/operating-gallery" "$out/gallery-two"
diff -r "$out/gallery-one" "$out/gallery-two" > "$out/gallery-repeat.diff"
# Changes to operating gallery geometry are recorded, never overwrite history.
set +e
diff -r "$evidence/gallery-a2-final" "$out/gallery-one" > "$out/gallery-established.diff"
code=$?
set -e
test "$code" -le 1
printf '%s\n' "$code" > "$out/gallery-established.exit"
test ! -s "$out/gallery-repeat.diff"
hash_ui > "$out/ui-source-after-sha256.txt"
cmp "$out/ui-source-before-sha256.txt" "$out/ui-source-after-sha256.txt"
shasum -a 256 "$out/bin/"* > "$out/binary-identities-sha256.txt"
python3 - "$out" <<'PY'
import pathlib,sys,re
out=pathlib.Path(sys.argv[1]);total=0
for f in (out/'logs').glob('ui-*.log'):
 m=re.findall(r'test result: ok\. (\d+) passed;',f.read_text());assert m,f;total+=sum(map(int,m))
assert total==41,total
(out/'test-counts.txt').write_text('Current corrected-source UI:41 PASS (32 original +5 corrected +4 new content/geometry tests). Repeated Cargo coverage, not additive. Unchanged core/paint/adapter inputs and rlibs verified against independent receipt.\n')
PY
printf 'ALL CORRECTED UI41 TESTS AND TWO COMPLETE GALLERY PROCESSES PASS; CORE/PAINT/ADAPTER IDENTITIES UNCHANGED\n' > "$out/status.txt"
