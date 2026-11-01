#!/bin/sh
set -eu
base=${1:?verified core dir}; ui=${2:?verified UI dir}; out=${3:?new output dir}; probes=${4:?unchanged fresh probe dir}
test ! -e "$out"; mkdir -p "$out"
root=$(pwd); export CARGO_PKG_VERSION=0.3.1 CARGO_PKG_NAME=rndr_ui CARGO_MANIFEST_DIR="$root/crates/rndr_ui"
compile() { rustc --edition=2021 -C debuginfo=0 -L "dependency=$base/current" -L "dependency=$ui/current" -L "dependency=$root/target/debug/deps" --extern "rndr_core=$base/current/librndr_core.rlib" --extern "rndr_ui=$ui/current/librndr_ui.rlib" "$@"; }
shasum -a 256 "$base/current/librndr_core.rlib" "$ui/current/librndr_ui.rlib" "$probes/fresh-probes.rs" "$probes/fractional-survey.rs" crates/rndr_ui/tests/a2_toolbar_content_fit.rs > "$out/inputs.sha256"
for name in fresh-probes fractional-survey; do
 compile --test "$probes/$name.rs" -o "$out/$name" > "$out/$name-compile.log" 2>&1
 set +e
 "$out/$name" --nocapture > "$out/$name.log" 2>&1
 result=$?
 set -e
 echo "$result" > "$out/$name.exit"
done
compile --test crates/rndr_ui/tests/a2_toolbar_content_fit.rs -o "$out/content-tests" > "$out/content-tests-compile.log" 2>&1
set +e
"$out/content-tests" --nocapture > "$out/content-tests.log" 2>&1
result=$?
set -e
echo "$result" > "$out/content-tests.exit"
shasum -a 256 "$out/fresh-probes" "$out/fractional-survey" "$out/content-tests" > "$out/artifacts.sha256"
