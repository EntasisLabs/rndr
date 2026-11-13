# PR149 CI completeness correction — implementation handback

## Identity and disposition

This is an append-only correction report, not a rewrite of the original repair
or independent FAIL verdict. Read those reports as historical claims about their
own revisions; in particular their original three-file completeness claim is
superseded here.

- Review/evidence parent: `e89d0bff32ea94941063af11c496162b5fe5a261`.
- Corrected, committed, freshly qualified source/tooling:
  **`e228c824618aec25b7f8657d1569472925486b24`**.
- Original repair candidate: `9961f763b74c24927d8be7b7c652f5269948f768`.
- Original baseline: `43a0e83394ac80a9ac35bcffde479bef4d71cfd1`.
- This report and its sibling receipts form an evidence-only child of corrected
  source. The terminal handback supplies its exact HEAD. No source changes
  occurred after qualification.

Independent correction review and sealed native assessment are still required.
No push, merge, native integration, main edit, workflow edit, toolchain install,
lint suppression, broad cleanup, or disk-guard weakening was performed. Owner
retains responsibility, correction review, sealed native evidence inspection,
integration onto **`rndr-flow-f0-f5` only**, push, and remote matrix verification.

## Complete correction diff and preserved semantics

Relative to the review parent, production changes are exactly four method-name
substitutions in **three files, four insertions/four deletions**:

1. `crates/rndr_flow/src/lib.rs:43`: authority allocation uses `try_update`.
   `AcqRel` success / `Acquire` load and failed-CAS ordering, pure checked-add
   closure, returned old-plus-one authority, and `Error::Exhausted` are unchanged.
2. `crates/rndr_flow/src/topology.rs:32`: topology freeze uses `try_update`.
   The same orderings, checked-add refusal, error translation and construction
   after successful increment remain. Owner/participant liveness validation and
   AcqRel decrement are unchanged.
3. `crates/rndr_liquid_ui/src/lifecycle.rs:14,469`: single and range serial
   allocation use `try_update`. Both Relaxed orderings, checked-add(1/count),
   `.ok()` return of the old serial/range start, nonwrapping exhaustion and
   all-or-nothing reservation semantics remain unchanged.

The read-only correction auditor verifies that replacing only those method-name
bytes in the parent's three files produces the exact corrected files. Existing
test assertions, signatures, manifests, lockfile, MSRV and workflow are untouched.
The integer-atomic compatibility API forwards its arguments and closure directly
to `try_update`, whose actual stability attribute is `atomic_try_update`,
**since 1.95.0**. The new `msrv-api.txt` quotes that actual method and CAS body,
not the `update` method. Installed documentation metadata is not a local 1.95
or 1.100 compiler execution.

The entire tracked nonhistorical Rust tree was checked: **598 files**, not just
core. No `fetch_update` token remains there. An all-tracked-Rust grep still finds
one frozen baseline snapshot under `evidence/`; it is not compiled workspace
source and was deliberately preserved. See `workspace-search.txt`. Historical
reports/logs also retain the old API spelling, correctly.

Two authorized new repair tools also change:

- `rndr-pr-149-ci-checks.sh`: environment gate uses `sh -ec`, so a failing first
  `git diff --exit-code HEAD -- crates Cargo.toml Cargo.lock` cannot be masked by
  later semicolon-separated commands. Overall gate failure aggregation remains.
- `rndr-pr-149-ci-audit.py`: validates the recorded original source manifest
  rather than writing it; validates historical receipts against original SOURCE
  Git blobs, not a later correction's working tree. It explicitly labels that
  original source, and cross-checks summary exit suffixes and Clippy log success
  bodies. The separate correction auditor validates current corrected source.

These five changed files total **19 insertions/11 deletions**. All remaining
correction files are new evidence/qualification helpers under this directory.
There is no broader tooling refactor. Cumulatively from the original baseline,
source/test changes are **six files, 92 insertions/eight deletions** (the original
three-file repair plus these four API renames).

## Fresh exact-source qualification

All following results belong to committed source `e228c824618aec25b7f8657d1569472925486b24`,
not to original source `6698b63`. `checks/environment.txt` records its clean
source gate, exact HEAD, tool versions and target. Local tools:
**rustc/Cargo 1.98.0, Clippy 0.1.98, aarch64-apple-darwin**.

```sh
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-flow-f5-retained-transitions-and-close-guards/target'
export CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
sh docs/reports/rndr-pr-149-ci-correction/run-checks.sh
```

The bounded runner loads only the corrected original driver's monitored `run()`
definition, not its full gate list. It runs no full core, renderer or preserved
probe rebuild. All **14 monitored gates exited zero**, and the driver exited 0.
Exact command lines, complete outputs, exit suffixes and before/after free KiB
are in `checks/` and `checks/summary.tsv`.

All seven Clippy argument configurations passed without warning suppression:

```sh
cargo clippy --offline --locked --workspace --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_render -p rndr -p rndr_ui --no-default-features --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_render -p rndr --features dialog --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_render -p rndr -p web-hello --target wasm32-unknown-unknown -- -D warnings
cargo clippy --offline --locked -p rndr --no-default-features --features ui --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr --no-default-features --features liquid,canvas --all-targets -- -D warnings
cargo clippy --offline --locked -p rndr_core --features tokio --all-targets -- -D warnings
```

| Fresh command | Result |
|---|---|
| `cargo fmt --check` | PASS |
| `cargo test --offline --locked -p rndr-flow --no-default-features -- --nocapture` | **144 passed, zero ignored** |
| `cargo test --offline --locked -p rndr_liquid_ui -- --nocapture` | **115 passed, two inherited ignored** |
| `cargo test --offline --locked -p rndr_core --no-default-features --lib freeze::tests` | **3 passed** |
| `cargo test --offline --locked -p rndr_core --no-default-features --test structured_composer` | **9 passed** |
| `python3 docs/reports/rndr-pr-149-ci-audit.py` | PASS, original-source/history audit only |
| `python3 docs/reports/rndr-pr-149-ci-correction/test-gates.py` | PASS, clean/dirty gate and manifest-corruption regressions |
| `python3 docs/reports/rndr-pr-149-ci-correction/verify.py` | PASS, exact corrected-source/history/receipt checks |

Flow tests include actual authority MAX-1/MAX refusal, identity/lease exhaustion,
freeze-dependent transitions and native-close protections. Liquid tests include
single serial MAX-1/MAX exhaustion, complete range reservation overflow without
partial consumption, keyed identity, lifecycle/retirement and retained-resource
teardowns. Their existing assertions were not changed. Caught panic output in
these suites is expected and not treated as a failure; final per-suite statuses
and counts are checked. There is no new topology-MAX synthetic test, exhaustive
CAS schedule proof or new memory-model claim.

`test-gates.py` runs the actual environment command in a disposable, directory-
confined Git fixture: clean passes; both unstaged and staged source changes fail
before rustc/df. It also injects a corrupt manifest **read**, without ever writing
historical bytes: the original repair auditor rejects it rather than regenerating
it. See `gate-regressions.txt`. Fixture files were removed by its scoped temporary
directory; no shared build/history cleanup occurred.

## Evidence and disk accounting

All report/review/receipt/fingerprint files already tracked at the review parent
remain byte-identical, except the two explicitly authorized new repair tools.
The prior FAIL review is intact. Original auditors remain historical; only the
new PR149 audit received the authorized read-only-manifest correction above.

`source-digests.txt` is a **new, generated-once** 589-file corrected-source
manifest. `verify.py` only validates it; rerunning the auditor never writes it.
All 589 entries match corrected source. Against original baseline source,
**583 remain unchanged and six differ**. No old fingerprint or exact-source
qualification is relabeled as covering this revision.

Disk was inspected before modification: **4,523,868 KiB available** (~4.3 GiB).
The reused target was explicitly selected; no broad rebuild was requested.
The same 3-GiB monitor remained active for all bounded gates; the lowest recorded
before/after sample was **4,521,908 KiB**. No disk-blocker receipt appeared.
Final pre-report inspection: **4,530,792 KiB** available. Values vary with other
host activity; this is not a whole-machine disk-usage attribution. The preserved
probe harness's higher 4.25-GiB floor remains untouched, and that harness was not
rerun for this correction.

## Remaining limits and required next action

**Local Rust 1.98 cannot establish Rust 1.100 CI green.** Its successful Clippy
runs predate the deprecation and alone cannot prove no other new-compiler lint
exists. Removing all four remaining calls addresses the concrete static review
blocker, not every possible future CI failure. No local 1.95/1.100 build and no
fresh remote CI were performed. Original remote CodeQL/CI receipts belong to the
old remote revision, not this correction.

The full core suite, renderer suite, original reviewer probe harnesses and
independent seam harness were **not freshly rerun on this corrected source**.
Their historical results remain useful only with that qualification. Workspace
all-targets Clippy compiles targets but does not execute all their tests. Two
Liquid UI ignored tests remain unrun here; seven other inherited ignored core
tests were outside this correction run (nine total inherited exclusions).
Full workspace/all-feature runtime tests, native Linux/Windows cfg paths,
package/docs steps, platform browser/WebView behavior and real GUI/IME behavior
remain unverified. No new durability, performance, backend-termination or
universal panic/leak-containment certification is claimed. Prior F0-F5 exceptions
remain unchanged; F6/application migration is outside scope.

Owner next: obtain independent correction review of the exact evidence-child
HEAD, inspect sealed native evidence, integrate only onto `rndr-flow-f0-f5`, push
PR149, and inspect the fresh Rust 1.100 macOS/Linux/Windows matrix. This worker
has not accepted or integrated its own candidate.
