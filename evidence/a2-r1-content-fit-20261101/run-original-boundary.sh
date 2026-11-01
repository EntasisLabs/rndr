#!/bin/sh
set -eu
base=${1:?};ui=${2:?};out=${3:?};fixture=${4:?}
test ! -e "$out";mkdir "$out"
rustc --edition=2021 -C debuginfo=0 --test "$fixture" -L "dependency=$base/current" -L "dependency=$ui/current" -L dependency=target/debug/deps --extern "rndr_core=$base/current/librndr_core.rlib" --extern "rndr_ui=$ui/current/librndr_ui.rlib" -o "$out/test" > "$out/compile.log" 2>&1
set +e
"$out/test" --nocapture > "$out/run.log" 2>&1
code=$?
echo "$code" > "$out/run.exit"
exit 0
