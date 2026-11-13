# rndr-flow F4 candidate report — 2026-11-12

## Scoped correction supersedes the original handback

The independent review of `9e09c575f222384d5e3c843511552f64d0cbbf4d`
**failed** on R1/R2; standard green gates did not prove those compositions.
Review-only parent `acaa7ede97f01791f19fe4629ad3e3157011aee4` and all original
implementation/review evidence are preserved. See the new
[correction report](rndr-flow-f4-correction.md),
[21-gate runner](rndr-flow-f4-correction-gates.sh), and
[separate fresh qualification](rndr-flow-f4-correction-evidence/qualified/summary.tsv).

R1 now destroys the actual timer completion under the owned catch boundary
**before** acknowledging returned/unwound destruction. A single destructor panic
is `Panicked`, not unknown, and does not escape update. R2 uses monotonic aggregate
terminal evidence across producer/task/timer/effect transitions: known panic or
failure cannot become success/unknown. Actual-exit acknowledgment stays separate;
blocked producers remain unacknowledged/charged. The review did **not** demonstrate
a timer capacity leak, and the correction makes no such claim.

Permanent regressions promote all three saved reviewer probes and add 12 timer
and 20 real producer ordering cases. Final corrected-source fingerprints and
exact-HEAD rerun details belong to the correction handback, not the old logs.
Facade compatibility is now **executed**, not merely compiled.

Everything below is the **historical original implementation handback**, with its
then-fresh results and limits retained for provenance. Its blanket matrix
completion claim was disproved by R1/R2 and is not evidence of correction or
acceptance. Neither worker delivery nor this correction is sender acceptance;
fresh independent exact-head review and sealed native integration remain required.

## Historical outcome and authority

Implemented F4 in the isolated undertaking based on accepted F3
`f16e5572d35450d586dd4f10e00591fa36fb2b04`. The candidate is the commit containing
this report; the terminal handback supplies its exact full OID and exact-HEAD
verification. **Worker completion is not acceptance.** Sender retains independent
review, corrections, sealed native approval and integration. No main-branch
integration, push, publication or deployment was performed.

The [updated readiness document](../design/rndr-flow-f4-readiness.md) contains the
actual API contracts, all five implementation slices and the complete
case-to-test matrix. Historical F3-waiting/F4-unauthorized wording was replaced;
F3's acceptance is not reopened.

## Implemented result

1. **Independent checked lanes.** Typed `LaneKey`, exact native `Identity<D>`,
   authority/scope/activity/lane-generation/World-bound request tokens; default
   32 lanes/scope. Capacity/exhaustion refusal preserves the old valid request
   and returns the untouched submitted future/value. Ordered delivery keeps one
   generation and rejects wrong native identity, duplicates and non-increasing
   sequences. Explicit unrecoverable suspend refusal and exact cancellation.
2. **Concrete owned work.** Existing local/background tasks, bounded feeds,
   one-shot timers, and owning-World removal effects. Default 4,096 work slots and
   256 effects/World. Rc/non-Send local futures and injected executors preserved.
   Child-first cancel/drop precedes custom cleanup; World-only unsubscribe is
   extracted and attempted before that cleanup. No competing runtime/timer wheel.
3. **Actual mutation boundaries.** Generic authoritative writer traits plus a
   concrete SourceDocuments adapter and consumer-side F2 proof. Captured target,
   operation, native revision, flow ownership/activity/lane and sequence validate
   synchronously at admission/commit, including published Input events. No
   selected entity/tab/projection fallback. Source ordered chunks advance only
   this operation's native receipts; no-op sequence consumption creates no
   history/revision, and outside edits cannot be blessed by rebasing. Owned task
   `Completion::transpose` preserves identity/sequence/receipt into the native
   writer, including explicit failure handling.
4. **Honest terminal/retirement accounting.** Future poll/drop, producer closure,
   local/background panic and explicit failure normalize into bounded status
   slots. Logical death, physical retirement, cancellation signal, cleanup attempt,
   acknowledged exit, reserved/queued/published receipts and unknown blocked work
   are distinct. Output receipts reserve **before poll**: there is no future-exit
   to publication charging gap. A retained Job after executor-start panic stays
   charged, reports no nonexistent cancel signal, and cannot later erase panic
   with user success. Retired slots release only after exact exit and final receipt
   disposal, not quietness, cancelled handles or publication ineligibility.
5. **Qualification and handback.** Full deterministic matrix, actual controlled
   external producer closures (cooperative versus blocked via handshakes), native
   source and UI contracts, lifecycle vector audit, reproducible commands and
   committed raw evidence.

Two implementation-audit regressions are permanent: (a) exited futures cannot
accumulate uncharged queued/published payloads while their World is not advanced;
(b) an application scope earlier than its target World is revisited after that
World's retirement. Both now have exact negative/progress tests.

## Historical original verification (not fresh correction evidence)

Run from repository root:

```sh
sh docs/reports/rndr-flow-f4-gates.sh
```

**20/20 gates passed**, with exact commands, exit status and free-space readings
in [summary.tsv](rndr-flow-f4-evidence/summary.tsv). Each command's complete output
is in its correspondingly named `.txt` file. The runner uses `--offline --locked`,
`CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, the same target tree, and stops before
starting another gate if free space falls below 4 GiB.

| Fresh check | Actual result |
| --- | --- |
| `cargo test -p rndr-flow --no-default-features` | **66 passed**, 0 ignored: 37 unit tests plus integration/retained probes. Includes all 29 unchanged F3 units, nine budget proofs, the 100-cycle/300-callback proof and three retained reviewer probes. The existing lease-refusal snapshot only gained an exhaustive case for the new handle traversal phase. |
| `cargo test -p rndr_core --no-default-features` | **831 passed, 7 ignored**, summed unit/integration/doctest summaries; current runtime/feed/timer/source contracts included. |
| `cargo test -p rndr_liquid` | **16 passed**, 0 ignored. |
| `cargo test -p rndr_liquid_ui -- --nocapture` | **115 passed, 2 ignored**, including two new consumer-side tests and unchanged F1/F2 regressions. Parser + pre-existing UI tests preserve the accepted 129-test corpus. |
| F4 repetition | **360/360 cycles**: 120 each for seeds 1, 7, 42; complete F3 Counts/F4 WorkCounts baseline equality after every cycle; **720** task/producer exit acknowledgments. |
| Unchanged F1/F2 repetition | **300/300 teardowns**: seeds 1, 7, 42, 100 each; all **19 counters** equal their per-World baseline, zero pending external acknowledgments every time. |
| Default work capacity/full commands | 4,096 owned feed slots, 32 lanes/scope, full 1,024-command queue; one-unit pumps reclaim everything in **33,375 charged units**. This is work accounting, not a time measurement. |
| Strict lint | Flow all-targets feature-off Clippy and consumer all-targets `--no-deps` Clippy, both `-D warnings`: passed. |
| Formatting/whitespace | `cargo fmt --all --check`, `git diff --check`: passed. Staged diff is checked again before commit. |
| Feature-off/facade | Flow feature-off lib; facade feature-off and Liquid-on lib checks; feature-off/Liquid-on `editing_compatibility` test-target **compilation**: passed. Facade test execution was not rerun; no GUI build/window test claim. |
| Dependency/boundary | Flow normal/build graph contains only feature-off core and its existing dependencies; no Tokio/UI/renderer edge. Reverse production graph keeps flow opt-in. Core, facade and accepted UI production source are byte-unchanged from baseline. |

The committed [audit program](rndr-flow-f4-audit.py) independently parses every
F4 repetition line and every F1/F2 19-counter vector; it checks cycle/seed
completeness, exact vector equality and zero pending acknowledgments, rather than
accepting a final green summary alone. See
[matrix-audit.txt](rndr-flow-f4-evidence/matrix-audit.txt).

These are **fresh** headless results on the final candidate source, not reused
historical F1/F2/F3 evidence. Earlier platform/release-performance qualification
is not imported as fresh evidence. The 9 ignored tests are not counted as run.

### Exact-source and exact-commit evidence

Qualification precedes committing: `environment.txt` therefore records the
accepted baseline HEAD while the F4 files were in the working tree. This is not a
claim that F4 tests existed on F3. [source-digests.txt](rndr-flow-f4-evidence/source-digests.txt)
pins every built flow Rust source/test/manifest plus Cargo.lock and the consumer
manifest/test. Those files are verified unchanged in the final commit. After
commit, the complete flow suite and consumer seam are rerun against the exact
HEAD; its native receipt and `target/f4-exact-head.txt` are returned with the full
OID. No source changes occur after qualification. A commit cannot embed its own
OID in its report without changing that OID.

## Dependency, disk and scope reconciliation

- No changes anywhere under `crates/rndr_core`, facade source/manifest, or
  `crates/rndr_liquid_ui/src`. Wrappers/status/receipt reservations sufficed;
  **no separately scoped core seam was necessary or edited**.
- UI's only graph change is an explicit **dev dependency** on feature-off flow
  for its two consumer-side proofs; Cargo.lock adds that one local package edge.
  Flow never depends on UI, and facade does not export/adopt flow.
- One worktree-owned target tree (about **2.4 GiB** at final inspection); no
  redundant target trees, full workspace/GUI builds, unrelated cache deletion or
  other worktree mutation. Initial free space was about **11 GiB**; final gates
  left about **9 GiB**. All gates stayed above the 4 GiB stop threshold.
- Evidence is approximately 288 KiB, not a build-artifact dump. Existing safe
  caches were reused within the same target; no network downloads were needed.

## Limits and residual obligations

- Generic/native consumer traits are host integration contracts, not automatic
  interception of bypass writers. Keep exact native session/turn incarnations in
  D, and use synchronous atomic native admission/commit with no await/reentrant
  gap. The concrete source and consumer-side UI proofs are delivered; no Medousa,
  F5 close/save/quit or F6 real app workflow adoption is implied.
- Keep each producer exit guard/wrapper with its **actual producer**, not its UI
  observer. Unsupported raw external work cannot be retrospectively joined.
  Blocked producers/custom jobs remain explicitly unknown/charged until exit;
  no backend SDK cancellation or durable job termination is inferred.
- Host must advance existing runtime/event buffers and drain each owning World's
  effects. Retained output clones keep the charge. Atomic replacement needs
  capacity for still-charged work; it does not silently evict/reclaim old valid
  work in admission. Limits bound admitted records/handles, not arbitrary host
  payload byte sizes or the duration of callbacks/executors/destructors.
- The maximum-epoch test exercises the production adapter's scalar preflight;
  it does not overwrite private core counters or claim 2^64 real refreshes. Core
  `advance` remains unchanged and is guarded before mutation with exclusive feed
  ownership. Fixed seeds/controlled orderings are not all possible interleavings.
- F3 exact observations/admission remain synchronous outside retirement budget.
  Retained liveness tokens can retain internal ancestors after Counts reach zero;
  final release can cause **depth-proportional destruction**. No total-latency,
  stack-safe final-token destruction or real-time claim is made.
- No OS/native GUI/real IME/packaging certification, release performance probe,
  arbitrary destructor double-panic/abort recovery or external IO join claim.
  F1 staging-cost and F2 inherited raw-stream split-CRLF caveats remain explicit.

No required headless race-matrix row remains merely proposed. The precise
non-headless/unrun obligations above remain separate; sender review/acceptance
and sealed native integration are still outstanding.
