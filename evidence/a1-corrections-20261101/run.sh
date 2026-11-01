#!/bin/sh
# Run from the undertaking root. A fresh output directory is mandatory: never overwrite custody logs.
set -eu
out=${1:?usage: sh evidence/a1-corrections-20261101/run.sh NEW_OUTPUT_DIRECTORY}
if test -e "$out"; then echo "Refusing to overwrite $out" >&2; exit 2; fi
mkdir -p "$out"
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
printf 'jobs=2 dev/test debug=0 incremental=false; existing target; offline locked\n' > "$out/environment.txt"
git rev-parse HEAD >> "$out/environment.txt"
df -h . >> "$out/environment.txt"
du -sh target >> "$out/environment.txt"
for item in core paint ui accesskit umbrella tree gallery; do
    case "$item" in
        core) set -- test --offline --locked -p rndr_core ;;
        paint) set -- test --offline --locked -p rndr_paint ;;
        ui) set -- test --offline --locked -p rndr_ui ;;
        accesskit) set -- test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests ;;
        umbrella) set -- check --offline --locked -p rndr --all-targets --no-default-features ;;
        tree) set -- tree --offline --locked -p rndr_ui --edges normal ;;
        gallery) set -- run --offline --locked -p rndr_ui --example passive_gallery -- target/ui-a1-corrections-gallery ;;
    esac
    printf 'cargo' >> "$out/commands-status.txt"
    printf ' %s' "$@" >> "$out/commands-status.txt"
    if cargo "$@" > "$out/$item.log" 2>&1; then
        printf ' exit=0\n' >> "$out/commands-status.txt"
    else
        result=$?
        printf ' exit=%s\n' "$result" >> "$out/commands-status.txt"
        tail -60 "$out/$item.log"
        exit "$result"
    fi
done
# No GPU or app/runtime crates may enter normal UI dependencies.
if grep -E 'wgpu|winit|rndr_render|rndr_paint|rndr_app' "$out/tree.log"; then
    echo 'GPU/runtime dependency check failed' >&2; exit 1
fi
printf 'GPU-free normal dependency assertion exit=0\n' >> "$out/commands-status.txt"
diff -rq evidence/ui-gallery target/ui-a1-corrections-gallery > "$out/gallery-comparison.log"
printf 'diff -rq evidence/ui-gallery target/ui-a1-corrections-gallery exit=0\n' >> "$out/commands-status.txt"
shasum -a 256 -c evidence/ui-gallery-sha256.txt > "$out/preserved-gallery-hashes.log"
shasum -a 256 -c evidence/recovery/logs-sha256.txt > "$out/preserved-recovery-hashes.log"
git diff --exit-code f7aa9703b7258e5ed546fc8436b3676f2705d9b0 -- evidence/recovery 'evidence/ui-*' > "$out/archived-evidence-diff.log"
printf 'preserved gallery/recovery hashes and archived evidence diff exit=0\n' >> "$out/commands-status.txt"
rustfmt --edition 2021 --check crates/rndr_core/src/interaction.rs crates/rndr_core/src/hotkey/registry.rs crates/rndr_core/tests/a1_interaction.rs crates/rndr_core/tests/a1_review_corrections.rs > "$out/format.log" 2>&1
printf 'rustfmt --edition 2021 --check four changed source/test files exit=0\n' >> "$out/commands-status.txt"
df -h . >> "$out/environment.txt"
du -sh target >> "$out/environment.txt"
cat "$out/commands-status.txt"
