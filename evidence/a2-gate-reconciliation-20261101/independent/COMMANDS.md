# Independent re-review command ledger

Working directory: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
Output `O=/tmp/rndr-a2-rereview-OgFoQ8`; unchanged dependency output `B=/tmp/rndr-a2-independent-SQwoYo/exact`; prior review `R=/tmp/rndr-a2-fresh-review-4TfleE`.

## Preflight and read-only audit

```sh
pwd; git rev-parse HEAD; git status --porcelain; df -h . /tmp
ls docs/design; ls "$R"; ls evidence/a2-r1-content-fit-20261101
```

Read canonical Specification/Guide/API/A2/checkpoint/tracking and prior independent REVIEW/COMMANDS/new REPORT/COMMANDS. Initial canonical reads preceded the build; complete bounded recovery of truncated canonical sections was completed during the audit. Complete build/reuse/probe runners were read before adaptation/use. Bounded Specification1–90/91–180/181–270/271–360, A2 1–50/51–110, tracking1–35/36–90 recovered initial combined truncation. Checkpoint complete147 lines. Inspected controls diff92..HEAD, plugin systems, core App/layout/font/text engine and affected operating/editor tests/callers. `rg` is unavailable; switched to grep/sed. Two guessed paths (`plugin.rs`, `a1_interaction.rs`) did not exist, and an inspection attempted the wrong receipt name (`reuse-validation.json` rather than `reuse-validation-final.json`); these were read-only discovery errors, not build failures. No source edit resulted.

Retained `correction.diff`, `seal.diff`, canonical/toolchain identities and full HEAD blob tree. Exact implemented source was unchanged throughout.

## Independent identity validation and entire fresh UI build

Scripts copied from committed evidence were read before use. They refuse existing build output directories. No archived historical runner was executed into an old output path.

```sh
O=$(mktemp -d /tmp/rndr-a2-rereview-XXXXXX)
cp evidence/a2-r1-content-fit-20261101/{verify-ui.sh,verify-rest-v2.sh,validate-reuse.py,run-fresh-probes.sh} "$O/"
python3 "$O/validate-reuse.py" "$O" > "$O/reuse-validation.log"
git ls-files -z crates Cargo.toml Cargo.lock | xargs -0 shasum -a 256 > "$O/source-before.sha256"
git ls-tree -r HEAD -- crates Cargo.toml Cargo.lock > "$O/blobs-before.txt"
git rev-parse HEAD > "$O/head-before.txt"
git status --porcelain > "$O/status-before.txt"
df -h . > "$O/resource-before.txt"
sh "$O/verify-ui.sh" "$B" "$O/exact-ui"
sh "$O/verify-rest-v2.sh" "$B" "$O/exact-ui" "$O/rest"
```

All above completed successfully. The UI script explicitly freshly compiles complete current UI library, all UI tests and both headless generators with direct rustc, verified core/paint/dependency paths and debuginfo0; source/artifact before/after inventories, actual command transcript and logs are inside exact-ui. The rest script reruns verified unchanged binaries, freshly compiles adversarial/geometry fixtures, repeats corrected5, regenerates passive gallery and executes supplemental Cargo checks/tests/lints. Both scripts are retained verbatim.

Common Cargo environment in rest runner:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
# Existing worktree target only. No second CARGO_TARGET_DIR.
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib
cargo check --offline --locked -p rndr --all-targets --no-default-features
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit
cargo tree --offline --locked -p rndr_ui --edges normal
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --all-targets
cargo clippy --offline --locked -p rndr_ui --all-targets -- -D warnings
cargo fmt --all -- --check
```

The native gallery command is **check only**, not run.

## Unchanged baseline→final negative controls

```sh
sh "$O/run-fresh-probes.sh" "$B" "$R/exact-ui" "$O/baseline" "$R"
sh "$O/run-fresh-probes.sh" "$B" "$O/exact-ui" "$O/final" "$R"
```

Each freshly compiles unchanged original fresh-probes.rs/fractional-survey.rs and final permanent fixture against the explicitly selected UI. Input/artifact hashes, compile/runtime logs and exit files retained. Baseline fresh2pass2fail/exit101; content0pass4fail/exit101; survey13 mismatches175 configs/exit0. Final4/4,4/4 and0 mismatches/exit0. Survey is diagnostic, not175 assertion tests.

```sh
cp evidence/a2-r1-content-fit-20261101/run-original-boundary.sh "$O/"
git show 92e98004:crates/rndr_ui/tests/a2_toolbar_allocation.rs > "$O/original-allocation.rs"
sh "$O/run-original-boundary.sh" "$B" "$R/exact-ui" "$O/boundary-baseline" "$O/original-allocation.rs"
sh "$O/run-original-boundary.sh" "$B" "$O/exact-ui" "$O/boundary-final" "$O/original-allocation.rs"
```

Old fixture baseline5pass/exit0; final2pass3fail/exit101. Expected obsolete-boundary failures preserved, not claimed as final suite passes.

## New independent tests and cost instrumentation

`new-adversarial.rs` was written only under the new temp output. `run-new.sh` contains the exact direct rustc command and explicit final UI/core linkage. Initial4 tests passed; source/compile/runtime logs retained as `new-adversarial-v1.*` before extending it with a fifth real-editor/source ownership test. `operating.rs` is an exact copy of the current gallery fixture, not modified repository source. Final5/5 pass.

`cost-probe.rs` wraps BuiltinText through the public TextEngine interface and counts actual layout calls for100 unrelated labels with0/1/8/32 toolbar candidates, surviving/rejected rows. `run-cost.sh` preserves the exact compile/runtime commands. One test/eight scenarios pass; no native timing interpretation.

```sh
sh "$O/run-new.sh" "$O"
# Equivalent to the executed cost compile/run pair:
sh "$O/run-cost.sh" "$O"
```

These script names write into their selected output; for future reproduction use a fresh parent with copied input scripts/probes and a freshly built exact-ui. Do not rerun into this sealed review output or old archived directories.

Independently attempted strict aggregate lint, preserving nonzero result:

```sh
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --all-targets -- -D warnings > "$O/strict-aggregate-clippy.log" 2>&1
echo $? > "$O/strict-aggregate-clippy.exit"
```

Exit101 on unchanged core test lines79/125, exactly two needless-borrow diagnostics. Strict UI passes in rest logs. No lint suppression/cleanup.

## Final preservation and identity checks

```sh
(cd evidence/a2-r1-content-fit-20261101 && shasum -a 256 -c MANIFEST.sha256) > "$O/candidate-manifest-validation.log"
python3 "$O/audit.py" "$O" > "$O/audit.log"
git ls-files -z crates Cargo.toml Cargo.lock | xargs -0 shasum -a 256 > "$O/source-after.sha256"
git ls-tree -r HEAD -- crates Cargo.toml Cargo.lock > "$O/blobs-after.txt"
git ls-tree -r HEAD > "$O/full-head-blobs.txt"
git rev-parse HEAD > "$O/head-after.txt"
git status --porcelain > "$O/status-after.txt"
cmp "$O/source-before.sha256" "$O/source-after.sha256"
cmp "$O/blobs-before.txt" "$O/blobs-after.txt"
python3 "$O/validate-reuse.py" "$O" > "$O/reuse-validation-after.log"
python3 "$O/audit.py" "$O" > "$O/audit-after.log"
git diff --check; df -h .; du -sh target "$O"
```

All successful. `audit.py` checks every historical evidence blob from92 (961), all current234 source files against HEAD, original32 tests against31006f0d, seal source equality, operating17 files against starting/worker/repeat and passive6 against goldens. Original reusable inventories copied without modifying them. `all-build-artifacts.sha256` records29 built Mach-O/rlib outputs; runner inventories separately hash reused artifacts and dependencies. `cargo-and-reuse-counts.json` records per-log test totals without additive-coverage claims.

Standard SHA256 manifest generated last over every review file except itself; validated once after generation. REVIEW/COMMANDS and binaries are included. No original evidence directory is rewritten. Manifest excludes itself as standard; its digest is returned separately to owner. No native launch, source/doc mutation, approval, integration, push, network or cleanup.
