# Independently executed commands
Working directory for every build/check below:
`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`

Initial preflight: `pwd; git rev-parse HEAD; git status --porcelain; df -h . /tmp`.
Fresh review output: `R=$(mktemp -d /tmp/rndr-a2-independent-XXXXXX)` produced `/tmp/rndr-a2-independent-SQwoYo`.
No CARGO_TARGET_DIR was redirected; existing worktree `target` only.

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
sh evidence/a2-20261101-mZVyJv/verify-current.sh "$R/exact"
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib
cargo check --offline --locked -p rndr --all-targets --no-default-features
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit
cargo tree --offline --locked -p rndr_ui --edges normal
sh evidence/a2-20261101-mZVyJv/verify-handoff-ui.sh "$R/exact" "$R/final-ui"
```
Both shell verifiers were read before execution. Their transcripts preserve exact rustc commands, dependency identities, source checks, and explicit rlib selection. `verify-current.sh` does NOT depend on a Cargo Fresh verdict; it rebuilds core, paint and all final UI source. The second command rebuilds UI again from current source and verifies unchanged core/paint/adapter inputs, then generates galleries twice and compares to sealed final galleries. Use NEW unused output directories if repeating. Do not overwrite this evidence or worker evidence.

Independent probes:
```sh
rustc --edition=2021 --test "$R/adversarial.rs" -C debuginfo=0 \
 -L "dependency=$R/exact/current" -L dependency=target/debug/deps \
 --extern "rndr_core=$R/exact/current/librndr_core.rlib" \
 --extern "rndr_ui=$R/exact/current/librndr_ui.rlib" -o "$R/adversarial"
"$R/adversarial" --nocapture
# 4 pass, 1 fail, test-process exit101; source and exact log preserved.
"$R/adversarial" --exact unequal_toolbar_labels_must_fit_each_row_share --nocapture
# exit101, toolbar-reproduction.log and .exit
rustc --edition=2021 --test "$R/geometry.rs" -C debuginfo=0 \
 -L "dependency=$R/exact/current" -L dependency=target/debug/deps \
 --extern "rndr_core=$R/exact/current/librndr_core.rlib" \
 --extern "rndr_ui=$R/exact/current/librndr_ui.rlib" \
 --extern "rndr_paint=$R/exact/current/librndr_paint.rlib" -o "$R/geometry"
"$R/geometry" --nocapture
# 1 pass, 64 independently checked combinations, exit0.
rustc --edition=2021 crates/rndr_ui/examples/passive_gallery.rs -C debuginfo=0 \
 -L "dependency=$R/exact/current" -L dependency=target/debug/deps \
 --extern "rndr_core=$R/exact/current/librndr_core.rlib" \
 --extern "rndr_ui=$R/exact/current/librndr_ui.rlib" \
 --extern "rndr_paint=$R/exact/current/librndr_paint.rlib" -o "$R/passive-generator"
"$R/passive-generator" "$R/passive-gallery"
diff -ru evidence/ui-gallery "$R/passive-gallery"
# exit0, all six passive states byte-identical.
```

`cargo-counts.json` parses the independent Cargo log, not worker counts. All five Cargo commands have no error lines and checks have Finished receipts; the enclosing original shell sequence did not set -e, so its final status alone is NOT used as proof.

For preservation, independently enumerated `git ls-tree -r 04642fe03f7603f228eaf9b83611641a95676e8d evidence` and compared each current `git hash-object`: all264 files unchanged. Also independently SHA256-validated all274 historical-preservation entries and all239 worker MANIFEST entries with a whitespace-splitting parser. The worker manifest uses a single separator space: `shasum -a 256 -c` rejects that syntax; raw attempt retained, parser results retained. No bytes were repaired or overwritten.
