#!/bin/sh
# R1 full checks, unchanged source-linked core/paint reruns, independent probes.
# Existing Cargo target only. Never overwrite evidence or remove output.
set -eu
base=${1:?independent exact receipt};ui=${2:?corrected exact UI receipt};out=${3:?NEW absolute output}
case "$out" in /*) ;; *) exit 2;; esac
test ! -e "$out"; test -f "$ui/status.txt"
free=$(df -Pk . | tail -1 | awk '{print $4}');test "$free" -gt 786432
mkdir -p "$out/logs" "$out/bin"
exec > "$out/transcript.log" 2>&1
set -x
root=$(pwd);deps="$root/target/debug/deps"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 INSTA_UPDATE=no INSTA_WORKSPACE_ROOT="$root"
export CARGO_PKG_VERSION=0.3.1 CARGO_PKG_NAME=rndr_ui CARGO_MANIFEST_DIR="$root/crates/rndr_ui"
git rev-parse HEAD > "$out/implementation-head.txt"
df -h .;du -sh target
# Every original exact binary/artifact identity and all unchanged source bytes.
python3 - "$base" "$ui" <<'PY'
import pathlib,hashlib,sys
base,ui=map(pathlib.Path,sys.argv[1:])
for receipt in [base/'binary-identities-sha256.txt',base/'current-rlibs-sha256.txt',base/'dependency-rlibs-sha256.txt',ui/'handoff-rlibs-sha256.txt',ui/'ui-source-after-sha256.txt',ui/'unchanged-core-paint-adapter-inputs-sha256.txt']:
 for row in receipt.read_text().splitlines():
  h,name=row.split(maxsplit=1);assert hashlib.sha256(pathlib.Path(name).read_bytes()).hexdigest()==h,name
PY
# Re-execute verified unchanged core/paint/actual adapter and prior13 probes;
# no rebuild needed because their complete inputs and artifacts are unchanged.
for binary in "$base"/bin/core-* "$base"/bin/paint-* "$base"/bin/actual-adapter "$base"/bin/probe-*; do
 "$binary" > "$out/logs/reused-$(basename "$binary").log" 2>&1
done
# Cargo coverage (Fresh alone is not identity proof; exact UI run is separate).
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui > "$out/logs/cargo-libraries.log" 2>&1
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib > "$out/logs/cargo-render.log" 2>&1
cargo check --offline --locked -p rndr --all-targets --no-default-features > "$out/logs/umbrella.log" 2>&1
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit > "$out/logs/native-source-check.log" 2>&1
cargo tree --offline --locked -p rndr_ui --edges normal > "$out/logs/normal-tree.log" 2>&1
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --all-targets > "$out/logs/clippy.log" 2>&1
cargo clippy --offline --locked -p rndr_ui --all-targets -- -D warnings > "$out/logs/clippy-ui-strict.log" 2>&1
cargo fmt --all -- --check > "$out/logs/workspace-format.log" 2>&1
# Link originals unchanged to corrected ALL-source UI, independently rebuilt core/paint.
compile() { rustc --edition=2021 -C debuginfo=0 -L "dependency=$ui/current" -L "dependency=$base/current" -L "dependency=$deps" --extern "rndr_core=$base/current/librndr_core.rlib" --extern "rndr_ui=$ui/current/librndr_ui.rlib" --extern "rndr_paint=$base/current/librndr_paint.rlib" "$@"; }
for name in adversarial geometry; do
 compile --test "$root/evidence/a2-r1-20261101/independent-review/$name.rs" -o "$out/bin/$name"
 "$out/bin/$name" --nocapture > "$out/logs/$name.log" 2>&1
done
compile --test crates/rndr_ui/tests/a2_toolbar_allocation.rs -o "$out/bin/final-regressions"
"$out/bin/final-regressions" --nocapture > "$out/logs/final-regressions.log" 2>&1
compile crates/rndr_ui/examples/passive_gallery.rs -o "$out/bin/passive-generator"
"$out/bin/passive-generator" "$out/passive-gallery"
diff -ru evidence/ui-gallery "$out/passive-gallery" > "$out/logs/passive-comparison.log"
rustfmt --check --edition 2021 crates/rndr_ui/src/controls.rs crates/rndr_ui/tests/a2_toolbar_allocation.rs crates/rndr_ui/tests/a2_toolbar_content_fit.rs > "$out/logs/format.log" 2>&1
shasum -a 256 "$out/bin/"* > "$out/binary-identities-sha256.txt"
git diff --stat -- crates Cargo.lock
df -h .;du -sh target
printf 'PASS: unchanged binaries rerun, Cargo suites/checks, independent5+geometry1, final regression5, passive6 identical\n' > "$out/status.txt"
