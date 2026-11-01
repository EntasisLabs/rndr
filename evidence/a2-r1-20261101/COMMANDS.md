# R1 exact commands and identities

Working directory throughout:
`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
Starting clean HEAD: `31006f0dfb834ea90e8b20ebac3b41d10dcdf88d`.
Final implementation: `aba338dfa77a50cd92b6b182fe78074584a20841`.

Preflight: `pwd; git rev-parse HEAD; git status --porcelain; df -h .`.
Read both historical verifier scripts completely before any invocation. Historical runners were NOT run in their existing output paths.

## Baseline / focused builds

```sh
B=/tmp/rndr-a2-independent-SQwoYo/exact/current
O=/tmp/rndr-a2-r1-build # created new; no Cargo target
rustc --edition=2021 --test crates/rndr_ui/tests/a2_toolbar_allocation.rs \
 -C debuginfo=0 -L "dependency=$B" -L dependency=target/debug/deps \
 --extern "rndr_core=$B/librndr_core.rlib" \
 --extern "rndr_ui=$B/librndr_ui.rlib" -o "$O/baseline-regressions"
"$O/baseline-regressions" --nocapture
# Compile0, process101: 0 passed / 5 failed, expected baseline failures.
```

The first compile attempted `Parent.0` (invalid field), failed before tests, and is retained in `baseline-compile.log`. Corrected harness uses `Parent.entity`; successful compile receipt is `baseline-compile-corrected.log`. This is a harness spelling error, not a product failure. The final identical permanent fixture was used for baseline and corrected runs; its SHA and baseline rlib/binary SHA are in `baseline-regression-identities.sha256`. Entire original baseline source receipt was checked against Git31006f0d (`baseline-git-source-identities.sha256`). Independently built rlibs, not colliding Cargo snapshot artifacts, are used.

Initial focused corrected-source run at e3847d2:
```sh
rustc --edition=2021 --crate-name rndr_ui --crate-type rlib crates/rndr_ui/src/lib.rs \
 -C debuginfo=0 -C metadata=a2_r1_exact -L "dependency=$B" -L dependency=target/debug/deps \
 --extern "rndr_core=$B/librndr_core.rlib" -o "$O/librndr_ui.rlib"
rustc --edition=2021 --test crates/rndr_ui/tests/a2_toolbar_allocation.rs \
 -C debuginfo=0 -L "dependency=$O" -L "dependency=$B" -L dependency=target/debug/deps \
 --extern "rndr_core=$B/librndr_core.rlib" --extern "rndr_ui=$O/librndr_ui.rlib" -o "$O/fixed-regressions"
"$O/fixed-regressions" --nocapture
# 5 pass; intermediate only, superseded by final full-source verification.
```

## Full exact-source verification

The new `verify-ui.sh` is adapted from the inspected historical `verify-handoff-ui.sh`: expected new UI count37, record changed gallery diff rather than require old geometry, preserve all historical files. It rebuilds ALL UI source with explicit independent core/paint rlibs, verifies every non-UI input plus dependency/artifact hashes, links all UI tests and generator explicitly, and runs eight galleries twice. It refuses existing output and requires >512MiB free.

```sh
sh evidence/a2-r1-20261101/verify-ui.sh \
 /tmp/rndr-a2-independent-SQwoYo/exact /tmp/rndr-a2-r1-final-ui
# e3847d2: exit101 at old 3xGo160pt narrow-policy assertion.
# Raw transcript/logs/source hashes preserved in initial-exact-run/; no test weakened.
# Retain original conservative total budget PLUS new actual-share check, commit aba338d.
sh evidence/a2-r1-20261101/verify-ui.sh \
 /tmp/rndr-a2-independent-SQwoYo/exact /tmp/rndr-a2-r1-verified-ui
# aba338d: exit0, UI37 pass, repeated galleries identical.
sh evidence/a2-r1-20261101/verify-rest.sh \
 /tmp/rndr-a2-independent-SQwoYo/exact \
 /tmp/rndr-a2-r1-verified-ui /tmp/rndr-a2-r1-rest
# exit0, all suites/checks/probes/passive comparison pass.
```

The complete expanded rustc commands/actual source paths and rlib identities are in `exact-ui/transcript.log` and `checks/transcript.log`, not inferred from Cargo Fresh. `verify-rest.sh` verifies original direct binaries and ALL unchanged inputs before rerunning them, requires >768MiB free, then executes:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib
cargo check --offline --locked -p rndr --all-targets --no-default-features
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit
cargo tree --offline --locked -p rndr_ui --edges normal
```

It compiles unchanged original independent `adversarial.rs`, `geometry.rs`, permanent regression fixture and passive generator with `-C debuginfo=0`, explicit final UI rlib and verified unchanged core/paint rlibs. Exact commands and outputs in checks/. Its real adapter binary rerun is original source-linked adapter, not synthetic replacement or native launch. UI source hashes rechecked after all work; unchanged core/paint/render/Cargo.lock diff verified empty.

## Preservation / sealing checks

- Original independent manifest parsed/hash-checked in place:404/404 valid; original report/requested manifest hashes match. All305 durable text-subset copies compared byte-for-byte to originals. No compiled output copied.
- Every504 pre-existing evidence path at Git31006f0d compared to current Git hash-object; SHA256 inventory is `historical-preserved.sha256`.
- All six current-source passive outputs compared with `diff -ru evidence/ui-gallery /tmp/rndr-a2-r1-rest/passive-gallery`, exit0.
- Standard new manifest: generate SHA256 + TWO spaces + root-relative path for every file under this directory except the manifest itself, then run `shasum -a 256 -c evidence/a2-r1-20261101/MANIFEST.sha256` from repository root. Original copied manifests remain provenance, with original absolute/relative paths; the new root manifest covers the actual durable subset.
- `git diff --name-status aba338dfa77a50cd92b6b182fe78074584a20841..HEAD` after sealing must contain only additions under this evidence directory; implementation/test/docs tree remains exactly the verified implementation.
