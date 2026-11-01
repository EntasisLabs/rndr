# Executed commands and log mapping

Commands ran in the existing authorized worktree rooted at the Git baseline `28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7`. Shell tool receipts retain chronological invocations and exit codes; redirected logs below retain compiler commands and test output. No network dependency fetch or native grants were requested.

## Entry / reproduction before modifications

```sh
df -h .
git status --short
git rev-parse HEAD
pwd
mkdir -p evidence/native-observation-v1-correction
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-review/client_probes.py > evidence/native-observation-v1-correction/baseline-client.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-review/run_rust_probes.py > evidence/native-observation-v1-correction/baseline-rust.log 2>&1
```

Entry: clean baseline, about 1.3 GiB available. Both reproduction drivers exit 0 by asserting the three defective behaviors. No implementation had yet been edited. File inspection used bounded source reads plus `grep`, `sed`, `head`, `tail` and hashes. One attempted `rg` inspection returned command-not-found; grep was used instead, with no installed tools or grants.

## Common build environment

Before every Cargo command in the sessions:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
```

No Cargo invocation omitted `--offline --locked`. The direct Rust replay compiles serially with `-C debuginfo=0`; exact rustc arguments and dependency paths are in its logs. No target directory or previous artifact tree was broadly deleted.

## Initial fixtures and intermediate verification (retained, not final claims)

```sh
cargo test --offline --locked -v -p rndr_render --no-default-features --features observe --lib > evidence/native-observation-v1-correction/focused-rust.log 2>&1
cargo test --offline --locked -v -p rndr_render --no-default-features --features observe --lib > evidence/native-observation-v1-correction/focused-rust-2.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p 'test_observe*.py' -v > evidence/native-observation-v1-correction/python.log 2>&1
rustfmt --edition 2021 --config skip_children=true crates/rndr_core/src/text.rs crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/lib.rs crates/rndr_render/src/text/mod.rs
cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe > evidence/native-observation-v1-correction/suites.log 2>&1
cargo test --offline --locked -v -p rndr_render --features observe --lib > evidence/native-observation-v1-correction/parley-default.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p 'test_observe*.py' -v > evidence/native-observation-v1-correction/python-final.log 2>&1
cargo test --offline --locked -v -p rndr_render --no-default-features --lib > evidence/native-observation-v1-correction/disabled.log 2>&1
cargo check --offline --locked -v -p rndr --example ui_gallery > evidence/native-observation-v1-correction/default-disabled.log 2>&1
cargo check --offline --locked -v -p rndr --features observe --example ui_gallery > evidence/native-observation-v1-correction/default-observe.log 2>&1
cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery > evidence/native-observation-v1-correction/native-build.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-correction/native-light > evidence/native-observation-v1-correction/native-light.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-correction/native-narrow --narrow --dark --200 --long > evidence/native-observation-v1-correction/native-narrow.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-correction/run_source_regressions.py > evidence/native-observation-v1-correction/source-regressions.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p 'test_observe*.py' -v > evidence/native-observation-v1-correction/python-final-2.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-correction/run_source_regressions.py > evidence/native-observation-v1-correction/source-regressions-final.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-correction/check_correction.py > evidence/native-observation-v1-correction/integrity.log 2>&1
```

First focused Rust command: exit 101 (extra `>` and captured closures unsupported by the system fn-pointer API). All later listed intermediate commands exit 0. `focused-rust-2.log` has 44 passing tests, intermediate Python logs have 18 then 20 as tests expanded. The source replay and checker were subsequently tightened; use their final logs and saved final scripts, not intermediate results, for sealing claims. Intermediate native binary SHA256 `3b2e6da96b6abaa277046e8907e59f7007e91639183cb107619c0f77fb12a0e5`.

## Final source / native verification

After separately versioning Parley pending-upload consumption:

```sh
rustfmt --edition 2021 --config skip_children=true crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/text/mod.rs
cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe > evidence/native-observation-v1-correction/suites-final.log 2>&1
cargo test --offline --locked -v -p rndr_render --features observe --lib > evidence/native-observation-v1-correction/parley-default-final.log 2>&1
cargo test --offline --locked -p rndr_render --no-default-features --lib > evidence/native-observation-v1-correction/disabled-final.log 2>&1
cargo check --offline --locked -p rndr --example ui_gallery > evidence/native-observation-v1-correction/default-disabled-final.log 2>&1
cargo check --offline --locked -p rndr --features observe --example ui_gallery > evidence/native-observation-v1-correction/default-observe-final.log 2>&1
cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery > evidence/native-observation-v1-correction/native-build-final.log 2>&1
shasum -a 256 target/debug/examples/ui_gallery
df -h .
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-correction/native-final-light > evidence/native-observation-v1-correction/native-final-light.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-correction/native-final-narrow --narrow --dark --200 --long > evidence/native-observation-v1-correction/native-final-narrow.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-correction/run_source_regressions.py > evidence/native-observation-v1-correction/source-regressions-sealed.log 2>&1
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p 'test_observe*.py' -v > evidence/native-observation-v1-correction/python-sealed.log 2>&1
shasum -a 256 crates/rndr_core/src/text.rs crates/rndr_render/src/lib.rs crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/text/mod.rs docs/native-observation.md tools/observe_client.py tools/test_observe_client.py tools/test_observe_correction.py > evidence/native-observation-v1-correction/source-inputs.sha256
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-correction/check_correction.py > evidence/native-observation-v1-correction/integrity-final.log 2>&1
git diff --check
git add crates/rndr_core/src/text.rs crates/rndr_render/src/lib.rs crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/text/mod.rs docs/native-observation.md tools/observe_client.py tools/test_observe_client.py tools/test_observe_correction.py
git diff --cached --stat
git commit -m 'fix(observe): bind paint engine resources and harden client file handling'
```

Every final command exits 0. Source replay driver deliberately runs three original-observer red regressions with expected child exit 101 and asserts those failures; the driver itself exits 0. Production suite totals: 576 affected minimal tests; 69 default/Parley renderer tests; 22 observer-disabled renderer tests; 20 Python tests; 22 direct observer/fresh-paint tests. Commands overlap and are not additive unique-test counts. Final native SHA256 `ce544034520d1a8cc6d9af538ffa0f3502d0438303f84240e55e8813f3e44e09`. Environment identities, disk and binary hash are in `environment-sealed.log`; implementation commit is `57458a7a8b1b164ac4959ce1226b7a19c01c2e0b`.

## Evidence sealing and replay discipline

`implementation.diff` is generated by the exact baseline-to-implementation Git diff; `MANIFEST.sha256` covers every file in this correction evidence directory except itself. Manifest generation/verification and evidence-only commit occur after the report is written and are retained by the shell tool receipts and final Git commit. No approval/merge/integration tools are invoked.

Replay native commands into **new directories**; saved evidence is not scratch output. The source replay writes only its exact target scratch directory. `check_correction.py` is read-only except stdout and can be rerun after the evidence seal without changing files. Test/build dependencies are pinned existing local artifacts, not an independent full dependency rebuild. See `REPORT.md` for the specific filesystem atomicity/deadline, custom engine/concurrency and native certification limits.
