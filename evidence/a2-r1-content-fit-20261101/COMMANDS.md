# Command ledger and reproducibility

Worktree: `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-ui-council-v0-1-component-library`.
All commands were local/offline; no native launcher was executed. Historical runners that overwrite old evidence were inspected, not run against their archived output directories.

## Entry and canonical reads

```sh
pwd; git status --short; git rev-parse HEAD
df -h . /tmp; du -sh target
```

HEAD was exact clean92e98004d886a9ccfc3d2d05eaf090b591e10cc9. Read full `docs/design/UI Library Council — Specification.md` (1–339), Review Guide(1–70), A2-components(1–100), A1-api-decisions(1–44), A0-A1-checkpoint(1–147) and implementation-tracking(1–70). Initial combined output was truncated; bounded reads recovered complete ranges, including specification1–85/86–170/171–255/256–339 and checkpoint1–75/76–147. Read complete fresh REVIEW.md, COMMANDS.md, verify-ui.sh, verify-rest.sh, fresh-probes.rs, fractional-survey.rs and the integer/fractional failure logs. Inspected actual core layout/font/App schedule and affected UI controls/plugin/callers/tests.

`validate-reuse.py` records the exact original review/manifest hashes and validates all216 manifest rows. It validates147 non-UI source rows against old hashes/current bytes/exact starting Git,79 original binary rows,3 original rlibs,5 dependency rlibs,22 starting UI source rows and fresh3-rlib inventory. `reuse-validation-final.json` contains every validated path/hash. Original source/rlib/dependency/binary inventories are copied in `reused-input-identities/`.

## Original adapted runners — inspect before running

The complete fresh review runners were read before adaptation. Exact adaptations are preserved in `verify-ui.sh`, `verify-rest.sh`, `verify-rest-v2.sh`. They refuse existing output directories. The complete scripts, actual `set -x` transcripts and per-command logs are the authoritative command detail, not Cargo “Fresh” messages.

- UI adaptation compiles every current UI source module/library/test/example directly; includes the new permanent fixture in the source inventory and adjusts expected suite count37→41 (32 original+5 corrected+4 new). Uncommitted candidate verification uses a diff inventory, not a false clean-tree requirement. Final committed source identity is checked separately.
- Rest runner keeps offline/locked/jobs2/debug0/incremental0 and the existing target. Adds workspace formatting and clippy. Original aggregate strict clippy fails on two existing core-test warnings; v2 preserves aggregate warning output and adds strict UI-only clippy. No unrelated source changes or suppression.

Common actual environment:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export CARGO_PKG_VERSION=0.3.1 CARGO_PKG_NAME=rndr_ui CARGO_MANIFEST_DIR="$PWD/crates/rndr_ui"
export INSTA_UPDATE=no INSTA_WORKSPACE_ROOT="$PWD"
```

## Executed sequence (distinct output names preserve every attempt)

```sh
O=/tmp/rndr-a2-content-fit-UJYjbC
B=/tmp/rndr-a2-independent-SQwoYo/exact
R=/tmp/rndr-a2-fresh-review-4TfleE
U="$R/exact-ui"
# Initial new permanent fixture, unchanged starting UI negative control:
rustfmt --edition 2021 crates/rndr_ui/tests/a2_toolbar_content_fit.rs
rustc --edition=2021 -C debuginfo=0 --test crates/rndr_ui/tests/a2_toolbar_content_fit.rs \
  -L "dependency=$B/current" -L "dependency=$U/current" -L dependency=target/debug/deps \
  --extern "rndr_core=$B/current/librndr_core.rlib" --extern "rndr_ui=$U/current/librndr_ui.rlib" -o "$O/baseline-content"
"$O/baseline-content" --nocapture
# Above: 0/4 pass, exit101. Compile succeeds. Exact old fixture snapshot retained.
```

An explicit focused library/test build in `initial-final/` then demonstrated4/4 on the initial correction. Full rustc flags, metadata and dependencies match the retained full UI runner; its compile/test logs are preserved. Source was changed with digest-fenced edits, then rustfmt, not shell replacement of repository source.

```sh
sh "$O/verify-ui.sh" "$B" "$O/exact-ui"          # expected test-import compile failure, preserved
sh "$O/verify-ui.sh" "$B" "$O/exact-ui-v2"       # UI41 + galleries pass
sh "$O/verify-rest.sh" "$B" "$O/exact-ui-v2" "$O/rest" # aggregate strict core-test lint failure
sh "$O/verify-rest-v2.sh" "$B" "$O/exact-ui-v2" "$O/rest-v2" # complete pass, qualified warnings
sh "$O/run-fresh-probes.sh" "$B" "$U" "$O/fresh-baseline" "$R"
sh "$O/run-fresh-probes.sh" "$B" "$O/exact-ui-v2" "$O/fresh-final" "$R"
python3 "$O/audit-preservation.py" "$O"
git show 92e98004:crates/rndr_ui/tests/a2_toolbar_allocation.rs > "$O/original-allocation.rs"
sh "$O/run-original-boundary.sh" "$B" "$U" "$O/original-boundary-old-ui" "$O/original-allocation.rs"
sh "$O/run-original-boundary.sh" "$B" "$O/exact-ui-v2" "$O/original-boundary-final-ui" "$O/original-allocation.rs"
# Add hidden-ancestor/malformed guardrail to permanent fixture:
sh "$O/run-fresh-probes.sh" "$B" "$O/exact-ui-v2" "$O/intermediate-hidden-failure" "$R"
# Above: guardrail catches missing-Rect panic,3/4 pass. Correct source conservatively.
```

### Authoritative final source run

Preflight checked free disk>2GiB, observed2.74GiB; no cleanup or new Cargo target.

```sh
rustfmt --edition 2021 crates/rndr_ui/src/controls.rs crates/rndr_ui/tests/a2_toolbar_content_fit.rs
sh "$O/verify-ui.sh" "$B" "$O/exact-ui-final"
sh "$O/verify-rest-v2.sh" "$B" "$O/exact-ui-final" "$O/rest-final"
sh "$O/run-fresh-probes.sh" "$B" "$U" "$O/fresh-baseline-final" "$R"
sh "$O/run-fresh-probes.sh" "$B" "$O/exact-ui-final" "$O/fresh-final-final" "$R"
sh "$O/run-original-boundary.sh" "$B" "$O/exact-ui-final" "$O/original-boundary-final-exact" "$O/original-allocation.rs"
python3 "$O/validate-reuse.py" "$O"
python3 "$O/audit-preservation-final.py" "$O"
```

`run-fresh-probes.sh` compiles the **unchanged** original fresh-probes.rs and fractional-survey.rs plus the current permanent fixture, with explicit verified core/selected UI rlibs. It records input/artifact hashes, compile logs, runtime logs and exact exits. Baseline exits are101/0/101 for fresh assertions/survey/permanent fixture; final all0. Survey prints13→0 mismatches over175 configs; no175-test claim.

The rest runner's actual Cargo commands include:

```sh
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features accesskit --lib
cargo check --offline --locked -p rndr --all-targets --no-default-features
cargo check --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit
cargo tree --offline --locked -p rndr_ui --edges normal
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --all-targets
cargo clippy --offline --locked -p rndr_ui --all-targets -- -D warnings
cargo fmt --all -- --check
```

It reruns verified unchanged core/paint/actual-adapter/conformance binaries, directly recompiles prior independent adversarial/geometry fixtures against the new exact UI, repeats corrected5, runs regenerated passive gallery and compares six files against preserved passive goldens. The UI runner directly compiles UI and operating/passive generators, runs two separate operating gallery processes, and records source/artifact hashes. It does not call `cargo run` for a native app.

## Commit, identities and seal

```sh
git diff --check
cargo fmt --all -- --check
# Explicitly stage only the five implementation/contract files; no old evidence.
git commit -m 'fix(ui): validate toolbar rows against snapped text content'
# Produced91c008bdc1054f42f67dcd5d45729290ddb4ffa4; implementation worktree clean.
python3 "$O/collect-identities.py" "$O"
python3 "$O/audit-preservation-final.py" "$O"
df -h . /tmp; du -sh target "$O"
```

`collect-identities.py` stores full before/after Git blob inventories, per-changed-file before/after SHA256/blob IDs, all retained Mach-O/rlib artifact hashes and final suite counts. Exact initial fixture/intermediate implementation snapshots are retained and checked against original recorded hashes. Copying compact evidence excludes binaries, never changes original evidence, and preserves raw file bytes. `seal-evidence.py` verifies source against implementation commit, final tested source inventory, history inventory, original review identity, fixture/probe/artifact inputs and full manifest before writing the new standard manifest/seal. Its post-seal checker independently verifies every manifest row and source identity. The evidence-only commit follows, and final diff/status checks establish clean exact HEAD with source unchanged.

## Safe reproduction

From the exact tested worktree, **inspect** all scripts, then:

```sh
sh evidence/a2-r1-content-fit-20261101/reproduce-final.sh
```

This wrapper is supplied for future reproduction; the equivalent individual commands above were executed. It admits only unchanged relevant source at91c008b, checks a>2GiB safety floor, validates original reusable identities, creates a fresh bounded `/tmp` parent and refuses existing child outputs. The original `/tmp` dependencies must still exist and validate. Do not run archived overwriting scripts against historical evidence or create a second Cargo target. If disk/identity is unsafe, stop and report; do not clean unrelated work or use a stale artifact.

Final outcome remains owner-arranged independent review, **not acceptance/integration/native certification**.

### Seal preflight correction

The first seal preflight exited1 before creating any seal: the compact-copy loop enumerated the original manifest, whose standard format omits itself. `independent-review/MANIFEST.sha256` therefore needed an explicit copy. The exact original file was copied separately after full original216/216 and expected-hash validation, then preflight rerun. `seal-preflight-first-failure.txt` records this evidence-packaging failure; no tested source or original evidence changed.

```sh
cp /tmp/rndr-a2-fresh-review-4TfleE/MANIFEST.sha256 evidence/a2-r1-content-fit-20261101/independent-review/MANIFEST.sha256
python3 evidence/a2-r1-content-fit-20261101/seal-evidence.py
python3 evidence/a2-r1-content-fit-20261101/check-seal.py
(cd evidence/a2-r1-content-fit-20261101 && shasum -a 256 -c MANIFEST.sha256)
git add evidence/a2-r1-content-fit-20261101
git commit -m "docs(ui): seal A2 text-content fit correction evidence"
git status --porcelain
git diff --exit-code 91c008bdc1054f42f67dcd5d45729290ddb4ffa4 HEAD -- crates Cargo.toml Cargo.lock docs/design
```
