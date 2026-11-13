# PR149 CI repair — independent bounded review

## Verdict: FAIL — incomplete Rust 1.100 CI repair

Reviewed candidate: `9961f763b74c24927d8be7b7c652f5269948f768`.
Qualified source: `6698b63cdf11ab7054f431be5bdebe9b6028edc9`.
Baseline: `43a0e83394ac80a9ac35bcffde479bef4d71cfd1`.
This directory is review evidence only. The review child must not be confused
with the candidate or qualified source above. No production/test source changes,
fixes, push, merge, branch integration, guard edits, or toolchain installation
were performed. Owner retains assessment, sealed native review, integration to
`rndr-flow-f0-f5` (never main), push and remote CI ownership.

### Blocking finding: four remaining deprecated atomic calls

CI `.github/workflows/ci.yml` first runs
`cargo clippy --workspace --all-targets -- -D warnings` on stable Rust (recorded
remote version 1.100). Both crates below are unconditional workspace members.
The candidate still contains:

- `crates/rndr_flow/src/lib.rs:43`: `AtomicU64::fetch_update` in authority allocation.
- `crates/rndr_flow/src/topology.rs:32`: `AtomicU64::fetch_update` in topology freeze.
- `crates/rndr_liquid_ui/src/lifecycle.rs:14`: `AtomicU64::fetch_update` in serial allocation.
- `crates/rndr_liquid_ui/src/lifecycle.rs:469`: `AtomicU64::fetch_update` in serial reservation.

These are ordinary production methods, not platform- or feature-excluded probe
code. The integer-atomic macro has the same `fetch_update` deprecation for U64
as Usize. Independently extracted installed std source marks it deprecated since
1.99, with the same rename diagnostic seen remotely for core's Usize call.
`-D warnings` promotes it to an error. The original core errors stop compilation
before downstream crates can expose these calls. Replacing core's one use does
not repair the entire workspace CI gate. The seven genuine local **1.98** passes
do not contradict this: that compiler predates the deprecation.

This is a concrete static blocker/prediction for Rust 1.100, **not an executed
local 1.100 compiler reproduction**. Do not approve this as a complete CI repair.
No correction was implemented during review.

## Complete source-diff assessment: PASS for the changes actually made

Independently inspected the full baseline-to-candidate non-doc diff: exactly
three files, 88 insertions/four deletions, two changed production functions.
Candidate and qualified source have identical non-doc trees. No manifest,
lockfile, toolchain/workflow, public signature or old assertion changes.

### Freeze API/MSRV/ordering/overflow

`try_update` on integer atomics is stable since **1.95.0**, matching workspace
MSRV. Installed std integer implementation has the stability attribute on the
actual method; `fetch_update` forwards arguments/closure directly to it.
The replacement therefore preserves its return value and CAS-loop semantics.
Success uses `AcqRel`; initial load and failed CAS use `Acquire`; weak-CAS retry
can re-evaluate the pure `checked_add(1)` closure. `None` returns failure without
increment/lease construction. `usize::MAX` cannot wrap. A successful increment
still precedes construction of the Arc-owned noncloneable lease. `active` remains
an Acquire load; drop remains AcqRel `fetch_sub(1)`. There is no new synchronization
or admission guarantee beyond the existing caller contract.

Both editing and structured writer paths use this same freeze state; composition
checks and error translation remain unchanged. Tests cover two held leases,
last-drop reopening, synthetic MAX-1/MAX refusal and successful drop accounting.
The overflow fixture deliberately does not represent MAX real leases; that is
appropriate for testing the arithmetic refusal branch, not a full ownership proof.

The eight-thread test is deterministic on successful paths: every thread
acquires before the first barrier; every count assertion precedes the second
barrier; nobody drops until all assertions have reached that second barrier.
A failed assertion could strand peers at the second barrier, so a regression can
hang rather than fail promptly; this is a test-diagnostics limitation, not a
current flaky-count race. Fresh execution passed, and 100 further existing-binary
invocations passed. No model checker, exhaustive schedule exploration or TSan.

### Structured text semantics

`ordered` validates both positions through `local`: block identity exists,
endpoint is not atomic, offset is within text and on a UTF-8 boundary, and not
inside a pill. It sorts `(block_index, byte_offset)`, rejects cross-cell ranges
and atomic blocks within the inclusive interval before mutation. Thus
`0 <= first.0 <= last.0 < blocks.len()`.

The new `.enumerate().take(last.0 + 1).skip(first.0)` enumerates BEFORE skipping:
indices remain absolute. It yields precisely the old inclusive block interval,
in the same order. Since `last.0 < blocks.len() <= usize::MAX`, `last.0 + 1`
cannot overflow for any valid Vec/state (and Block is not a ZST). There is no new
out-of-range silent truncation on reachable validated inputs.

First-block start and last-block end remain the original byte offsets; middle
blocks use `[0, block.len())`. Per block, prefix, middle and suffix slices, mark
cloning, allocation order and replacement of `runs` are unchanged. Empty ranges
and exact boundary endpoints retain the same behavior; no newline becomes a run
or receives a mark. Immutable access used to construct slices is followed by
assignment through the same block's mutable reference instead of reindexing.
Parent operation dispatch/normalization and transaction/history ownership are
unchanged. There is no Unicode interpretation change: byte slicing and validated
UTF-8 boundaries remain in the same helpers.

New composer coverage genuinely tests a nonzero first block, partial endpoints,
a fully selected middle block, reverse direction and a same-block selection;
asserts unchanged text, directed selection and block IDs plus exact undo/redo
block restoration. Its ASCII fixture/Strong-mark oracle is not exhaustive
Unicode, empty-boundary, pill, all-mark or invalid-input coverage. Existing pill
and mixed-mark tests also passed. The limited new coverage is appropriate for
the tiny traversal rewrite, not certification of the entire editor.

## Evidence validity and claims

Read the complete repair report, audit script, new driver and preserved-probe
harness, relevant exact-source logs, remote diagnostic/check JSON, and workflow.
Independently checked per-log exit suffixes against both summary tables, all
seven Clippy command flags and successful completion records, and exact-source
environment records. Recorded 1.98 passes are credible local receipts, **not**
1.100 or three-platform certification.

`final-checks` contains 16 zero-exit gates. `qualification` contains 17 gates,
16 zero-exit and one preserved-probe refusal. Its log has no assertion output;
combined with the unchanged harness's `set -eu` disk test and recorded free KiB,
this supports refusal before compilation at its 4.25-GiB floor. Earlier original
13/4/9 suites passed on the same source. Do not relabel the aggregate repeat as
green. The report explicitly distinguishes it; that distinction is correct.
No inherited guard was edited and no broad repeat was attempted by this reviewer.

The original auditor was rerun successfully. It parses the recorded core 838/7,
flow 144/0, consumer 115/2, renderer 60+1, preserved suites 26, reviewer probes 8
and independent seam 14 results and lifecycle coordinate receipts. Those are
implementation-side results re-audited here, NOT freshly rerun full suites.

Independent fingerprint verification confirmed all **589** historical digests
match baseline Git blobs; all old reports/harness artifacts remain byte-unchanged.
Current source is **586 unchanged / exactly 3 changed**. The new 589-file manifest
matches repair source; it does not retroactively recertify old exact-source
qualification. The frozen original auditors necessarily remain historical.

### Nonblocking evidence/documentation weaknesses

1. `msrv-api.txt` preserves the stability attribute attached to `pub fn update`,
   not `pub fn try_update`. Its intended conclusion is true, but the excerpt is
   not direct proof of that specific API. This review records the correct integer
   `try_update` attribute and CAS body in `static-verification.txt`.
2. The report's seven-local-config pass claim is accurate. Treating it as proof
   no further Rust 1.100 source blocker exists is overbroad: the four sites above
   are a counterexample. The report's literal statement that none was reproduced
   locally is not itself a fabricated compiler result.
3. The audit script regenerates `source-digests.txt` rather than checking its
   prior bytes, trusts Clippy summary statuses without checking Clippy log bodies,
   and is not an independent compiler verifier. Rerunning it left tracked bytes
   unchanged here; this review separately checked the Clippy logs.
4. The driver's environment `sh -c` uses semicolons without `set -e`, so the final
   `df` can mask failure of its first `git diff --exit-code`. That command alone
   is not a reliable clean-source gate. Independent Git/blob checks close that
   evidence gap for this candidate; the harness should not be advertised as a
   general cryptographically sealed qualification mechanism.

## Workflow risk review and limits

All seven Clippy argument configurations are represented locally, including wasm,
no-default, dialogs, facade ui/liquid+canvas and core tokio. But host cfg paths on
Windows/Linux are not checked by macOS+wasm compilation. Stable is rolling, not
pinned to local 1.98 by the pinned toolchain action. `--all-targets` is not
`--all-features`, and later all-feature tests, compiler/rustdoc warnings, platform
link/native libraries, browser/WebKit/WebView smokes, package/docs steps and full
workspace runtime tests remain possible additional blockers. No claim that the
four enumerated sites are the only possible remaining CI failures. No other
specific platform failure established in this bounded review. Recorded CodeQL
success belongs to the original remote revision, not the repair candidate.

No local 1.95/1.100 build; no fresh remote CI; no native GUI/IME/backend/durability
or performance certification. Nine inherited ignored tests remain unrun.
Existing opaque-panic-payload/destructor/resource exceptions are not erased.

## Fresh commands/results

Using existing shared F5 target with `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`,
`CARGO_PROFILE_DEV_DEBUG=0`, `CARGO_PROFILE_TEST_DEBUG=0`:

- `cargo test --offline --locked -p rndr_core --no-default-features --lib freeze::tests`: exit 0, 3/3.
- `cargo test --offline --locked -p rndr_core --no-default-features --test structured_composer`: exit 0, 9/9.
- `python3 docs/reports/rndr-pr-149-ci-audit.py`: exit 0; tracked evidence unchanged.
- `python3 docs/reports/rndr-pr-149-ci-independent-review/verify.py`: exit 0;
  static/fingerprint/log checks pass; identifies four remaining CI blockers.
- Existing `rndr_core-6ee1ebda0b1c170c --exact freeze::tests::concurrent_leases_preserve_count`:
  100/100 subprocess runs passed, no rebuild.

Focused Cargo invocations reused existing target artifacts (0.24s/0.06s, no
compile lines). Free space was 4,337,624 KiB before fresh checks and 4,332,916 KiB
after focused tests/audit, above the 3-GiB admission floor. No broad builds,
cleanup or guard changes. Raw fresh outputs and independent checker accompany
this report. Terminal commit receipt supplies the exact evidence-child HEAD.

Commit hygiene: whole-evidence `git diff --cached --check` returned 2 solely for
Cargo's trailing blank line in each raw `freeze.txt` / `composer.txt` receipt.
Those raw outputs were retained verbatim. Authored review/checker whitespace
checks pass; this is not a production-source whitespace failure.
