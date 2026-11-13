# F4 scoped correction handback — R1/R2

## Authority, candidate and provenance

This is an implementing-coder correction in the **same authorized F4 undertaking**,
not approval, integration or independent review. Original candidate
`9e09c575f222384d5e3c843511552f64d0cbbf4d` failed independent review. The accepted
review deliverable, review-only descendant
`acaa7ede97f01791f19fe4629ad3e3157011aee4`, remains in history with its full report,
probe source and logs unchanged. Accepted F3 remains
`f16e5572d35450d586dd4f10e00591fa36fb2b04`.

Correction history is additive, never rewritten:

- `3b5db30c3224a6e87a4f8d35a300e0c9bc908c22`: timer destruction ordering/catch,
  aggregate outcome precedence, permanent reviewer and ordering regressions,
  separate qualification runner/auditor; parent is `acaa7ede`.
- `14c082e45ad2b5945aa198d32a6ee082eb5d6e66`: strict-lint repair using the standard
  no-op waker; no production change. The first full attempt stopped on this lint,
  not a failing behavior test; its evidence is preserved.
- `bda61046daef65661d697bd90e4665d6827c9417`: related-path audit correction marks
  timer/feed cleanup attempts **before** the cancellation/destruction call, so a
  caught single panic cannot falsely leave `cleanup_attempted=false`. Task
  cancellation already used this ordering. Permanent matrices assert it.
- The commit containing this report adds only documentation/evidence to that
  final qualified source. Its exact OID and immediate parent are supplied in the
  terminal handback after an additional full exact-HEAD rerun. A report cannot
  embed its own commit OID without changing it.

## R1 resolved at the owned timer boundary

`TimerCompletion::drop` now takes the actual `Completion` out of its field and
runs its destruction under `catch_unwind` **before** setting actual-exit
acknowledgment. Payload destruction returns or unwinds first; a single caught
panic records `Panicked`. Automatic field drop cannot then destroy that payload
after acknowledgment. Independent runtime/World progress is retained.

The receipt lives throughout payload destruction, including unwind. Caller
clones retain their own shared receipt; event exit is not final receipt disposal.
The reviewer observed a pending receipt during the old early acknowledgment and
**did not demonstrate capacity leakage**. Neither this report nor the fix claims
that such a leak was reproduced.

Permanent coverage:

- `tests/f4_corrections.rs` promotes all three saved independent probes. The
  timer probe additionally requires update not to unwind, receipt=1 and no exit
  acknowledgment *inside* the destructor, and acknowledgment with receipt=0
  afterward. The World-effect abandonment probe remains a passing control.
- `tests/f4_timer_drop.rs`: **12 cases**, six orderings with normal versus one
  panicking owned payload destruction: cancelled future event; published event;
  clone dropped before event; clone retained after event; World loss before
  firing; World loss after publication with retained clone. It verifies actual
  inside-drop status, charge while a clone is retained, final release, same-World
  task progress where that World survives, and independent-World task progress.
  The caller-owned cloned payloads in this matrix do not themselves panic; the
  single controlled panic is at the owned timer-event boundary being tested.

## R2 resolved without manufacturing exit

All outcome transitions in the existing task/producer/timer/effect wrappers
preserve aggregate evidence using one private precedence rule:

`Panicked > Failed > Succeeded > DroppedUnknown > Running`.

This is evidence precedence, not a new public mutation API or executor.
`ProducerExit::run` and guard drop cannot overwrite a known panic/failure with
success/unknown. `Tracked` completion/drop and World-effect completion/abandonment
use the same rule. Remaining direct panic assignments only raise to the maximum
outcome. Actual exit is still recorded only by the real producer/future/event
boundary. Caught cancellation panic, quiet pumps or logical death cannot
acknowledge a channel-blocked producer.

**Return values versus aggregate status:** `run` returns its closure's normalized
result. It can return `Ok(())` while `WorkStatus` truthfully retains a cleanup
panic. `track`, like owned local/background tasks, returns `Err(Panicked)` at
Ready if aggregate panic is already known. Cleanup occurring after either output
can still raise aggregate failure. Neither output certifies native commit/save,
receipt disposal, or backend success. Inspect `WorkStatus` for aggregate outcome.

Permanent coverage:

- The saved real-thread producer probe is promoted, not replaced with an observer
  acknowledging unrelated work.
- `tests/f4_terminal_order.rs`: **20 actual producer cases**, cleanup before and
  after exit for closure-run and tracked-future success, `Failed`, `Panicked`,
  actual panic and guard/future drop. Channel handshakes select both orders, with
  no sleeps. A deliberately blocked producer stays unacknowledged/charged after
  cleanup panic and repeated pumps. Each case checks retained panic, truthful
  cleanup attempt, eventual actual exit, independent task progress and baseline.
- Private tests check all 25 terminal pairs in both orders and verify dropping a
  tracked pending future preserves known Failed/Panicked without early ack.
  Existing local/background failure/panic/drop, retained-Job startup failure and
  World-effect tests remain unchanged and freshly executed.

## Red/green and evidence separation

All **new** logs live under
[`rndr-flow-f4-correction-evidence/`](rndr-flow-f4-correction-evidence/).
No original implementation or independent review evidence was overwritten.
Interleaved raw test output contains a few trailing spaces/blank lines. A local
`.gitattributes` exempts only these new receipt `.txt` files from whitespace lint
so their exact bytes are preserved (matching an existing evidence convention).
Production/test source and Markdown whitespace checks remain enabled.

- `red-reviewer.txt`: at original production source `acaa7ede`, the copied minimal
  reviewer probes produce exit **101**, **1 passed / 2 failed** (R1/R2).
- `green-reviewer.txt`: immediately after the production fix, the same probes
  produce **3 passed / 0 failed**. The later strengthened permanent probe is also
  included in the full final-source qualification.
- Development compile logs are retained, including the test-only moved-World
  setup error and its repair. The root `summary.tsv` is the first qualification
  attempt stopped by Clippy's manual-noop-waker warning. It is **not** presented
  as a successful final run.
- `intermediate-14c082e/` preserves the full 21/21 successful intermediate run,
  before the additional cleanup-attempt bookkeeping repair. Its original output
  path was `qualified/`; only these new logs were moved, not altered, to preserve
  the final destination for the final source. It is not substituted for the final
  run or for independent review.
- **Final source qualification:**
  [`qualified/summary.tsv`](rndr-flow-f4-correction-evidence/qualified/summary.tsv),
  complete per-command logs and
  [`qualified/exact-source-audit.txt`](rndr-flow-f4-correction-evidence/qualified/exact-source-audit.txt).
  Environment records `bda6104`; **all 23** fingerprints equal both current bytes
  and that commit's blobs, including every flow Rust source/test/manifest,
  lockfile and consumer proof/manifest. Those bytes remain identical in the final
  documentation/evidence commit.

Reproduce from repository root, supplying a **new** evidence destination (the
runner refuses to overwrite an existing summary):

```sh
sh docs/reports/rndr-flow-f4-correction-gates.sh target/f4-correction-exact-head
python3 docs/reports/rndr-flow-f4-correction-audit.py target/f4-correction-exact-head
```

The final post-commit run uses exactly that destination in the **existing target
only**; it is a log directory, not another Cargo target tree. Full terminal
receipts and exact-HEAD audit are returned in the handback. All commands use
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, `--offline --locked`.

## Fresh final-source qualification

**21/21 gates passed**; these are new executions, not inherited results:

| Gate / proof | Result |
| --- | --- |
| Full feature-off flow unit/integration/doctests | **73 passed, 0 ignored** (original 66 plus seven new permanent tests, including the 32-case ordering matrices) |
| Full feature-off core tests/doctests | **831 passed, 7 ignored** |
| Liquid parser tests/doctests | **16 passed, 0 ignored** |
| Full Liquid UI + consumer suites/doctests | **115 passed, 2 ignored**, including unchanged accepted F1/F2 regressions and both native consumer proofs |
| Executed facade editing compatibility, feature-off and Liquid-on | **2 + 2 passed**; not merely compiled; no window opened |
| F4 lifecycle repetition | **360/360 unique cycles**, 120 each for seeds 1/7/42; **720** actual task/producer exit acknowledgments; complete F3 Counts and WorkCounts equality asserted by executed test after each cycle |
| Unchanged F1/F2 lifecycle repetition | **300/300** rows, all **19 counters** equal their per-World baseline and **zero pending external acknowledgments on every row**; independently parsed by the new auditor |
| Existing F3 budgets and default full-capacity proof | All unchanged F3 units, nine budget proofs and retained reviewer regressions pass; 4,096 work slots/full 1,024 commands reclaim in **33,375 charged one-unit visits**, not elapsed-time evidence |
| Strict lint, formatting, whitespace | Flow all-targets feature-off Clippy and consumer all-targets `--no-deps` Clippy, both `-D warnings`; fmt/diff checks pass |
| Feature-off/facade/dependency boundaries | Feature-off flow and facade libs; facade Liquid-on lib; normal/build and reverse dependency graphs; unchanged protected production paths and original evidence all pass |
| Exact source | 23 refreshed fingerprints checked against current and committed blobs; no original evidence reused as a source fingerprint for corrected code |

The **nine ignored core/UI tests remain unrun**, not passes. No release probes,
native GUI/OS/real IME certification, broad random soak or exhaustive interleaving
claim is made. Earlier worker/reviewer gate results are historical only.

## Scope, limits and sender handback

Only **`crates/rndr_flow/src/work.rs`** changes production behavior relative to
`acaa7ede`; three new flow integration-test files and private unit regressions
supply coverage. The two report/readiness documents are updated, and new
correction report/runner/auditor/evidence are added. No core/facade/UI production,
manifest or lockfile change in this correction. No new runtime/executor/Tokio or
flow-to-UI edge, no F5/F6/Medousa/SDK scope, main integration, push or deploy.
Atomic request preflights, mutation-boundary stale rejection, owning-World drains,
receipt charging and bounded retirement are unchanged.

Disk was monitored before/after every gate and every second during commands;
runner interrupts at **4.25 GiB**, above the requested 4 GiB floor. Free space
remained around **7.6 GiB**, with no unrelated deletion, redundant target or GUI
workspace build. Actual readings are in each summary; the final handback supplies
the post-exact-HEAD reading.

Existing limits remain: keep guards with actual producers; drain actual owning
Worlds and runtime buffers; retained receipts keep charges; unknown blocked work
is not joined; host native writer traits do not intercept bypass writers.
Synchronous observations/admission and arbitrary callback/destructor/executor
duration are outside retirement budgets. Retained-token final destruction can
remain depth-proportional. No total-latency/constant-stack-final-drop guarantee,
arbitrary double-panic/abort recovery, backend IO termination or durability claim.
F1 staging cost and inherited F2 raw-stream split-CRLF limits remain explicit.

**Worker correction delivered for fresh independent exact-head review, not F4
acceptance.** Sender retains review, correction and sealed native approval/
integration ownership. The independent report remains FAIL for its original
candidate; this coder has not edited that verdict or independently approved the
new candidate.
