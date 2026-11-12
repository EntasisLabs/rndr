# PR #149 CI repair handback

## Candidate and scope

Baseline: `43a0e83394ac80a9ac35bcffde479bef4d71cfd1`.
Committed qualified source: **`6698b63cdf11ab7054f431be5bdebe9b6028edc9`**.
This report/evidence is an evidence-only child; the source stays identical.
Work was performed in the assigned isolated `worktree/rndr-pr-149-ci-repair` undertaking.
The owner's native integration target is **`rndr-flow-f0-f5`, not `main`**.
No push, merge, PR edit, workflow dispatch, or native integration was performed.
Owner retains final review, integration, push and follow-up CI ownership.

## Root causes and complete source diff

CI run **43316407903** updated all three OS jobs from Rust 1.99.1 to
**1.100.0 (0ef1bc386, 2026-11-09)**. All three passed formatting, then failed
at the first workspace Clippy step. Later CI steps were skipped.
The full run and failed-step logs were retrieved with read-only `gh` calls;
relevant diagnostics are retained in `rndr-pr-149-ci-evidence/remote-errors.txt`.

1. `crates/rndr_core/src/freeze.rs`: replace deprecated `fetch_update` with
   `try_update`. **This is not a blind new-API substitution:** workspace MSRV is
   already **1.95**, five other core call sites already use `try_update`, and
   the installed standard-library source marks `atomic_try_update` stable since
   **1.95.0**. It also shows `fetch_update` forwarding to `try_update`.
   `msrv-api.txt` preserves those excerpts. A handwritten CAS loop is unnecessary.
   Checked overflow refusal (`checked_add(1)`), `AcqRel` success / `Acquire`
   failure ordering, returned lease, and `AcqRel` decrement are unchanged.
2. `crates/rndr_core/src/structured/text_ops.rs`: eliminate
   `clippy::needless_range_loop` using `iter_mut().enumerate().take().skip()`.
   Enumeration remains **absolute**, not relative to the selected slice.
   The same inclusive block range, endpoint byte offsets, validation order,
   run slicing/mark assignment, and final normalization owner are retained.
3. Targeted tests: three private freeze tests cover nesting/final-drop reopening,
   overflow refusal without wrapping, and eight concurrent held leases; one
   structured-composer test covers a nonzero starting block, partial endpoints,
   a full middle block, reversed selection, a same-block selection, unchanged
   identities/text/selection, and exact undo/redo restoration.

Source diff: **three files, 88 insertions / four deletions**, including tests.
New report/harness/receipt files are separate additions. Whole-candidate
`git diff --cached --check` reports whitespace in captured command output
(including interleaved panic/test lines and remote log blank records), not in
source or authored documentation. Those receipt bytes were retained rather than
silently rewriting command output; source whitespace and formatting checks pass.
Only two production functions change. No manifests, lockfile, MSRV, workflow,
renderer/flow production code, public APIs, lint allowances, or existing test
assertions were changed. No unrelated formatting churn.

## Verification: exact commands, environment and results

Local: **rustc 1.98.0 (88d9e12ae, 2026-08-18)**,
**cargo 1.98.0 (797e8a9bc, 2026-08-05)**,
**clippy 0.1.98 (88d9e12ae1, 2026-08-18)**,
`aarch64-apple-darwin`. Installed wasm target was reused.
No toolchain was upgraded or installed. API compatibility with MSRV 1.95 is
source/stability-metadata evidence, **not an executed 1.95 build**.
CI's 1.100 compiler was **not** available locally and was not executed here.

All builds used the existing F5 shared target:

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-flow-f5-retained-transitions-and-close-guards/target'
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
```

Exact per-gate command lines, stdout/stderr, exit status and before/after free
KiB are recorded in `rndr-pr-149-ci-evidence/final-checks/` and `qualification/`.
The reproducible driver is `rndr-pr-149-ci-checks.sh` (POSIX syntax checked).
Every following CI Clippy configuration passed with **`-D warnings`**, without
`--no-deps` or lint suppression:

```sh
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_render -p rndr --features dialog --all-targets -- -D warnings
cargo clippy --locked -p rndr_render -p rndr -p web-hello --target wasm32-unknown-unknown -- -D warnings
cargo clippy --offline --locked -p rndr --no-default-features --features ui --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr --no-default-features --features liquid,canvas --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_core --features tokio --all-targets -- -D warnings
```

The initial offline wasm attempt refused an uncached `js-sys v0.3.105` download.
Retrying without `--offline` fetched the locked dependencies and **passed**.
No lockfile change resulted; this is not an unresolved CI blocker.

| Command (all Cargo tests also use `--offline --locked`) | Result |
|---|---|
| `cargo fmt --check`; `git diff --check 43a0e83 -- crates` | PASS |
| `cargo test -p rndr_core --no-default-features --lib freeze::tests` | 3/3 focused tests |
| `cargo test -p rndr_core --no-default-features --test structured_composer` | 9/9 tests |
| `cargo test -p rndr_core --no-default-features` | **838 passed, seven inherited ignored** |
| `cargo test -p rndr-flow --no-default-features -- --nocapture` | **144 passed** |
| `cargo test -p rndr_liquid_ui -- --nocapture` | **115 passed, two inherited ignored** |
| `cargo test -p rndr_render --no-default-features --lib` | **60 passed** |
| `cargo test -p rndr_render --no-default-features --test close_validator_api` | **one passed** |
| `sh docs/reports/rndr-flow-f5-probes.sh` | Original F4 reviewer probes: **3 + 1 + 3 + 1 passed** |
| `sh docs/reports/rndr-flow-f5-seam-evidence/run-preserved-probes.sh 6698b63` | Original F5 reviewer assertions: **13 + 4 + 9 passed** |
| `sh docs/reports/rndr-pr-149-ci-seam-probes.sh 6698b63` | Byte-unchanged independent seam assertions: **14 passed** (seven independent + seven helper) |
| `python3 docs/reports/rndr-pr-149-ci-audit.py` | PASS; exact source/history and selected fresh receipts verified |

Fresh detailed lifecycle receipts preserve **300 F1/F2 full 19-counter
teardowns**, **360 F4 cycles / 720 actual exit acknowledgments**,
**360 F5 full-resource cycles**, and **360 terminal-window cycles**.
Existing panic/order, tiny-budget and close-safety tests were not weakened.
These remain headless evidence, not native GUI/event-loop certification.

### Attempts and disk limit (do not collapse these into one green run)

- `checks/`: exploratory precommit pass. Native Clippy and tests passed;
  wasm was initially offline-blocked. The exact-source preserved-probe guard
  correctly refused the then-uncommitted source. Those refusals were resolved.
- `preserved-postcommit.txt`: original 13/4/9 suites passed on committed source.
- `final-checks/`: **all 16 recorded gates exited zero** on exact committed
  source, including all seven Clippy configurations and original 13/4/9 suites.
- `qualification/`: redundant final driver run, now with aggregate failure
  propagation and the additional independent seam suite. **16 gates exited zero;
  the repeated preserved-probe gate refused before compiling** because free
  space was **4,309,600 KiB**, below that unchanged historical harness's
  **4,456,448-KiB (4.25-GiB) floor**. The driver correctly exited 1.
  The additional independent seam suite passed 14/14. This is **not** a new
  assertion failure; the identical original suites had already passed twice
  against the same source. Their higher disk guard was not lowered or edited.

The new driver monitors a 3-GiB floor for its own bounded commands; the inherited
higher guard remains intact. Initially about 4.4 GiB was free; at final inspection
about **4.1 GiB** remained (4,308,436 KiB). Free space varied during execution.
No broad cleanup, `cargo clean`, or deletion of shared/historical artifacts was
performed. No disk-full build occurred. Further broad builds/toolchain installs
were not attempted. Per-gate disk receipts are retained rather than inferring
whole-machine consumption solely from one before/after number.

## Frozen evidence and fingerprint impact

**No frozen audit artifact, saved probe, old report or fingerprint was rewritten.**
The new audit verifies all **589** frozen F5 independent source fingerprints
against the original baseline Git blobs and confirms all old report paths are
byte-unchanged. Against the new source, **586 match and exactly three differ**:
`freeze.rs`, `structured/text_ops.rs`, and `tests/structured_composer.rs`.
`rndr-pr-149-ci-evidence/source-digests.txt` is a **new** 589-file manifest
matching the exact committed repair source.

Old exact-source auditors are intentionally historical: their original
source-fingerprint/allowlist checks cannot certify this changed revision.
They were not edited to produce a misleading green result. The new audit checks
both historical truth and current selected receipts; it does not recertify the
entire original 27-gate F5 review or erase its exclusions.

## GitHub status, residual risks and owner follow-up

Read-only commands used `/opt/homebrew/bin/gh`, with
`XDG_CACHE_HOME=/tmp/medousa-gh-ci` for sandbox-safe cache writes:
`run view 43316407903 --repo EntasisLabs/rndr --log[-failed]`,
`pr checks 149 --repo EntasisLabs/rndr --json name,state,link`, and
`run view 43316405835 --repo EntasisLabs/rndr --json headSha,conclusion,jobs`.
`remote-checks.json` and `codeql.json` retain their responses.
**CodeQL Actions, CodeQL Rust and the CodeQL aggregate succeeded** on the original
PR revision. No additional remote check failure was identified beyond the three
known CI jobs. Those jobs still refer to the original remote source: this worker
has not pushed the repair, so there is **no new remote green-CI claim**.

Owner must review this candidate, integrate onto **the PR branch**, push, and
inspect the fresh Rust **1.100** macOS/Linux/Windows checks. The full workspace
runtime/all-feature tests, native browser/platform smoke tests, package/docs
pipeline and real GUI/IME behavior were not rerun here. Nine inherited ignored
performance/accounting tests remain unrun. Cross-platform/new-toolchain lint
or later CI failures remain possible; no concrete additional source blocker
was reproduced by the seven local Clippy configurations.

All existing F0-F5 trust, destructor, nontransactional-destruction and exceptional
opaque-panic-payload retention limits remain unchanged. There is no new universal
leak-free/bounded-panic-recovery, durability or performance claim. F6 and
application migration remain outside scope.
