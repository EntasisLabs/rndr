#!/bin/sh
# Executed from the repository root; all outputs remain in this evidence scope.
set -eu
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
E=evidence/native-observation-v1
# Post-seal safety guard: the command sequence below is preserved verbatim,
# but rerunning it must not overwrite the committed evidence/manifest.
if [ -f "$E/MANIFEST.sha256" ]; then
    echo 'Refusing to overwrite sealed evidence; use final-commands.log commands with fresh output paths. Use check_artifacts.py to verify existing artifacts.' >&2
    exit 2
fi
mkdir -p "$E"
: > "$E/final-commands.log"
run() {
    name=$1; shift
    printf '%s:' "$name" >> "$E/final-commands.log"
    printf ' %s' "$@" >> "$E/final-commands.log"
    printf '\n' >> "$E/final-commands.log"
    if "$@" > "$E/$name.log" 2>&1; then
        printf '%s exit=0\n' "$name" >> "$E/final-commands.log"
    else
        code=$?
        printf '%s exit=%s\n' "$name" "$code" >> "$E/final-commands.log"
        tail -40 "$E/$name.log"
        exit "$code"
    fi
}
{
    date -u
    git rev-parse HEAD
    git rev-parse HEAD^{tree}
    rustc -Vv
    cargo -V
    python3 --version
    df -h .
    du -sh target
    git status --short
} > "$E/final-environment.log"
git show --format=fuller --stat HEAD > "$E/implementation-commit.log"
git diff ff397cd9ef10678194068107043fcdbb3803d52a HEAD -- > "$E/implementation.diff"
git ls-tree -r HEAD -- Cargo.lock crates/rndr crates/rndr_core crates/rndr_paint crates/rndr_render crates/rndr_ui docs/native-observation.md tools > "$E/source-git-identities.log"
run final-format rustfmt --edition 2021 --config skip_children=true --check crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/gpu.rs crates/rndr_render/src/lib.rs crates/rndr_paint/src/lib.rs crates/rndr/examples/ui_gallery.rs
run final-client python3 -m unittest discover -s tools -p test_observe_client.py -v
run final-suites cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe
run final-disabled cargo test --offline --locked -p rndr_render --no-default-features --lib
run final-default-disabled cargo check --offline --locked -p rndr --example ui_gallery
run final-default-observe cargo check --offline --locked -p rndr --features observe --example ui_gallery
# Do not start another build with less than 1 GiB available.
python3 - <<'PY'
import shutil
free=shutil.disk_usage('.').free
print('pre-native-build free_bytes=',free)
if free < 1024**3: raise SystemExit('disk budget prevents further build')
PY
run final-build-native cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery
run final-native-light python3 tools/validate_observation_native.py target/debug/examples/ui_gallery "$E/final-native-light"
run final-native-narrow python3 tools/validate_observation_native.py target/debug/examples/ui_gallery "$E/final-native-narrow" --narrow --dark --200 --long
{
    git rev-parse HEAD
    shasum -a 256 target/debug/examples/ui_gallery
    df -h .
    du -sh target
    git status --short
} > "$E/final-after.log"
