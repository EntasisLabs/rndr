# Fresh review commands and replay mapping

Entry: `df -h .; git status --short; git rev-parse HEAD` → 716 MiB, clean exact `dc470507c5e65977ce34bbe6a6a9e4f1376f7983`. Inspected full nine-file `git diff 28a0ae307f869ef651d8ebcc5fdcd82a9dc31ce7 57458a7a8b1b164ac4959ce1226b7a19c01c2e0b`, full scoped test diff, reports/contract and affected surrounding callers. This directory alone is fresh review output. Do not replay native commands into sealed directories: supply NEW paths.

Environment before Cargo / Python work:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 PYTHONDONTWRITEBYTECODE=1
```

Executed commands, in order; all exit 0 unless specifically noted:

```sh
python3 -m unittest discover -s tools -p 'test_observe*.py' -v > evidence/native-observation-v1-rereview/python.log 2>&1
cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe > evidence/native-observation-v1-rereview/suites.log 2>&1
cargo test --offline --locked -v -p rndr_render --features observe --lib > evidence/native-observation-v1-rereview/parley.log 2>&1
cargo test --offline --locked -p rndr_render --no-default-features --lib > evidence/native-observation-v1-rereview/disabled.log 2>&1
python3 evidence/native-observation-v1-rereview/check_seals.py > evidence/native-observation-v1-rereview/integrity.log 2>&1
cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery > evidence/native-observation-v1-rereview/native-build.log 2>&1
python3 evidence/native-observation-v1-correction/run_source_regressions.py > evidence/native-observation-v1-rereview/source-replay.log 2>&1
python3 evidence/native-observation-v1-rereview/run_extra_rust.py > evidence/native-observation-v1-rereview/extra-rust.log 2>&1
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-rereview/native-light > evidence/native-observation-v1-rereview/native-light.log 2>&1
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-rereview/native-narrow --narrow --dark --200 --long > evidence/native-observation-v1-rereview/native-narrow.log 2>&1
# Initial extra-client probe exit 1; retained as extra-client-initial.log.
# Reviewer-only test assumption corrected as explained below, then:
python3 evidence/native-observation-v1-rereview/extra_client.py > evidence/native-observation-v1-rereview/extra-client.log 2>&1
cargo check --offline --locked -p rndr --example ui_gallery > evidence/native-observation-v1-rereview/default-disabled.log 2>&1
cargo check --offline --locked -p rndr --features observe --example ui_gallery > evidence/native-observation-v1-rereview/default-observe.log 2>&1
python3 evidence/native-observation-v1-rereview/check_pixels.py > evidence/native-observation-v1-rereview/pixels.log 2>&1
python3 evidence/native-observation-v1-correction/check_correction.py > evidence/native-observation-v1-rereview/inherited-checker.log 2>&1
shasum -a 256 evidence/native-observation-v1-correction/MANIFEST.sha256 target/debug/examples/ui_gallery
git diff --check -- crates tools docs
df -h .
```

Chronological detail: native commands ran serially in their original shell session; while they completed, extra-client/check commands used another session. Both native commands completed 0 and were polled, not replayed. Default checks ran between the initial client attempt and corrected client rerun. No native process was duplicated after unknown outcome.

Initial extra-client probe required success on an opened descriptor fd after its path was unlinked/replaced. This filesystem updates the old inode's ctime, causing the client correctly to deny. Initial source differed from final `extra_client.py` only in that it asserted `_descriptor(...) == {'old': True}` directly instead of allowing ValueError conservative denial. The failed log remains `extra-client-initial.log`; final test still forbids replacement data. Product code never changed. Source replay intentionally contains three old-observer child exit-101 red tests; driver exit 0. All current production suites and final new probes pass.

Direct compile recipes/dependency identities/source hashes are printed in `source-replay.log` and `extra-rust.log`. Scratch paths are `target/native-observation-correction` and `target/native-observation-rereview`; the former ends on intentionally old-observer red test output, not a shipping binary. Native executable validation uses existing compiled final binary SHA256 ce544034..., with cache-hit build evidence disclosed, not a fresh rebuild claim.

Final seal: hash every payload in this new directory except MANIFEST.sha256 into MANIFEST.sha256; recheck every row; stage ONLY this directory and commit review evidence. No approve/merge/integration tool is used. Parent of that review-only commit is exact candidate dc470507....
