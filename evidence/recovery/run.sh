#!/bin/sh
# Run from the repository root; uses only the existing local target/cache.
set -eu
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
mkdir -p evidence/recovery
for item in core paint ui accesskit umbrella tree gallery; do
    case "$item" in
        core) set -- test --offline --locked -p rndr_core ;;
        paint) set -- test --offline --locked -p rndr_paint ;;
        ui) set -- test --offline --locked -p rndr_ui ;;
        accesskit) set -- test --offline --locked -p rndr_render --no-default-features --features accesskit --lib a11y::tests ;;
        umbrella) set -- check --offline --locked -p rndr --all-targets --no-default-features ;;
        tree) set -- tree --offline --locked -p rndr_ui --edges normal ;;
        gallery) set -- run --offline --locked -p rndr_ui --example passive_gallery -- target/ui-recovery-gallery ;;
    esac
    if cargo "$@" > "evidence/recovery/$item.log" 2>&1; then
        printf '%s exit=0\n' "$item"
    else
        result=$?
        tail -60 "evidence/recovery/$item.log"
        exit "$result"
    fi
done
diff -rq evidence/ui-gallery target/ui-recovery-gallery > evidence/recovery/gallery-comparison.log
# Raw cargo receipts end with blank lines; preserve their exact bytes.
git diff --check -- . ':(exclude)evidence/*.log' ':(exclude)evidence/recovery/*.log'
# Historical *-reproduced.log failures are intentionally retained, not rerun here.
