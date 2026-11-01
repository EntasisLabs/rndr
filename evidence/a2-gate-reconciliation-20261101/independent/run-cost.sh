#!/bin/sh
set -eu
out=$1;base=/tmp/rndr-a2-independent-SQwoYo/exact
rustc --edition=2021 -C debuginfo=0 --test "$out/cost-probe.rs" -L "dependency=$out/exact-ui/current" -L "dependency=$base/current" -L dependency=target/debug/deps --extern "rndr_core=$base/current/librndr_core.rlib" --extern "rndr_ui=$out/exact-ui/current/librndr_ui.rlib" -o "$out/cost-probe" > "$out/cost-probe-compile.log" 2>&1
"$out/cost-probe" --nocapture > "$out/cost-probe.log" 2>&1
