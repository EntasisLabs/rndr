# Independent exact-candidate F4 re-review — FAIL (new R3)

Date: 2026-11-12. Same authorized F4 undertaking; independent reviewer only.

- Reviewed candidate: **`218e06c9998296a87f6393e182f03f9e40e97b92`**.
- Its immediate parent: **`bda61046daef65661d697bd90e4665d6827c9417`**.
- Accepted F3 / unchanged `main`: **`f16e5572d35450d586dd4f10e00591fa36fb2b04`**.
- Original candidate `9e09c575f222384d5e3c843511552f64d0cbbf4d` still has its valid
  historical **FAIL**, preserved with all original probes/logs/report at
  `acaa7ede97f01791f19fe4629ad3e3157011aee4`. Nothing there is overwritten.

**R1 and R2 are resolved on this candidate. A newly reproduced adjacent P2,
R3, prevents my acceptance recommendation.** All 21 standard qualification gates
passed freshly; those green gates do not cover the failing composition.
This report/evidence commit is a documentation-only descendant of the exact
candidate, not a fix, sender acceptance, approval, or integration. Final report
HEAD and parent are returned in the terminal handback, not self-embedded here.

## Blocking finding: R3 — P2, discarded tracked output escapes owned catch

**Location:** `crates/rndr_flow/src/work.rs:160–177`, especially **173–177**,
`Tracked<F>::poll`, exposed by `ProducerExit::track` (lines 215–219).
This is an adjacent, still-existing hole, **not a claim that the correction
introduced that branch**. The same normalization wrapper serves owned tasks.

The future's `poll` and its destruction are separately caught. When aggregate
status is already `Panicked`, the wrapper substitutes `Err(TaskFailure::Panicked)`
for the successful `value`. But that unused `Result<T, TaskFailure>` is then
automatically destroyed on leaving `poll`, **outside either owned catch**.
A panic in this library-discarded output destructor therefore escapes `poll`.
The documented track contract promises a normalized `Err(Panicked)` at Ready
when aggregate panic is already known; the wrapper does not actually return it
in this case. A host using an ordinary executor that does not add another catch
can lose the producer task/thread to the escaping unwind.

### Reproducer and actual result

The public-API reproducer is
[`rndr-flow-f4-rereview-evidence/independent_rereview.rs`](rndr-flow-f4-rereview-evidence/independent_rereview.rs),
test `tracked_discarded_output_single_unwind_is_normalized`:

1. Start a bounded owned feed and move its sender and `ProducerExit::track`
   future to the **actual producer thread**. The future enqueues one value,
   signals readiness, and blocks on a channel before returning a successful
   nontrivial output. No private status mutation, sleeps or observer ack.
2. On the owning thread, drop the scope and pump cleanup. Queue destruction
   panics; the adapter catches it and returns. Assert `Panicked`,
   `cleanup_attempted=true`, `exit_acknowledged=false`, one charged slot.
   Further quiet pumps still cannot acknowledge this blocked producer.
3. **After that unwind is completely caught and over**, release the producer's
   channel. It returns `Ok(Output)`. `Tracked::poll` observes aggregate panic,
   substitutes `Err(Panicked)`, and destroys the output outside its catch.
4. The output destructor first asserts **`!std::thread::panicking()`**, counts its
   one destruction, and raises its single panic. Only an outer **review-harness**
   catch catches it; the flow wrapper does not normalize it.

```text
discarded-success output_panic=true normalized=false
before: retired=true cancellation_signalled=true cleanup_attempted=true
        exit_acknowledged=false pending_deliveries=0 terminal=Panicked
after:  retired=true cancellation_signalled=true cleanup_attempted=true
        exit_acknowledged=true pending_deliveries=0 terminal=Panicked
```

This is **two sequential, non-overlapping unwinds**, not destructor panic during
an active unwind, process abort or arbitrary double-panic recovery. The test
checks that distinction explicitly. The output belongs to the wrapper when it
is discarded; it was never returned to an arbitrary caller to destroy.

**Narrow impact:** a normalized tracked-producer poll unexpectedly unwinds.
Aggregate status correctly stays `Panicked`, actual future exit is acknowledged,
and the slot reclaims after actual exit. No false success, fabricated exit,
capacity leak/bypass, stale native write, data loss, or escaping `Sim::update`
is claimed by this reproducer. Task wrappers share the branch, but their outer
core executor may add another panic catch; the directly proved public path is
`ProducerExit::track`.

The final probe run is **exit 101: 2 passed / 1 failed**. Raw final output:
[`adversarial.txt`](rndr-flow-f4-rereview-evidence/adversarial.txt).
The nonpanicking discarded-output variant **passes** under the exact same
channel ordering, proving aggregate panic otherwise returns the advertised
`Err(Panicked)`. A separate timer native-identity-destructor panic control also
**passes** with an unacknowledged exit and live receipt inside destruction,
then `Panicked`/acknowledged after the owned catch returns.

An earlier exploratory run is retained as `adversarial-initial.txt`; its normal
timer control was misleadingly named as an identity test and did not construct
that identity. It is **not** the identity-destructor proof. The final saved
source/log corrects that control and uses the actual channel-blocked producer.

Reproduce without changing production or historical evidence:

```sh
sh docs/reports/rndr-flow-f4-rereview-evidence/reproduce.sh
```

The script refuses changed candidate source or an existing probe, checks the
4.25 GiB margin, temporarily installs only the saved test, uses the existing
target with offline/locked, incremental=0, jobs=2, and removes it via a trap.
Expected result on this candidate: **101, 2 passed / 1 failed**.

**Required correction:** contain destruction of the wrapper-discarded output
within owned normalization; keep monotonic failure evidence and actual-exit/
receipt charging intact. Add a permanent non-unit-output regression for this
composition and audit the shared task paths. Do not weaken the contract or
reinterpret sequential caught panics as panic-during-unwind. No production fix
was made by this reviewer. Fresh qualification and exact-head independent review
remain required after a scoped implementing-coder correction.

## R1/R2 resolution and adjacent audit

I inspected the correction source/tests, full correction report and saved
red/green evidence, original full independent report/probes, governing F0/epic,
and both accepted-F3 parent and corrected F4 readiness contracts. F0/epic and
F3's original budget/regression sources are byte-unchanged from accepted F3.
Historical report statuses are not new authority. Green worker assertions were
not substituted for this review's own raw executions.

- **R1 closed:** `TimerCompletion::drop` takes the completion out and destroys
  it under `catch_unwind` before acknowledging actual returned/unwound
  destruction. The completion's field order preserves its receipt across
  payload destruction/unwind. Single owned payload/identity panic is caught as
  `Panicked`; independent progress survives. Retained clones keep the shared
  receipt charged until final disposal. Twelve permanent normal/panic cases
  freshly cover cancellation, publication, clone-before/after, and World loss.
  The original R1 probe **never established a capacity leak**; none is inferred.
- **R2 closed:** private monotonic evidence rank is
  `Panicked > Failed > Succeeded > DroppedUnknown > Running`. Poll/drop/run,
  timer and effect transitions preserve it; direct `Panicked` assignments only
  raise to the maximum. The rank is aggregate evidence, not chronology, backend
  success or an exit oracle. All 25 rank pairs/both orders and pending-drop
  tests pass. Twenty permanent **real channel-controlled producer** cases
  freshly cover run/track success, Failed, Panicked, actual panic and abandonment
  in both cleanup/exit orders. No ack is manufactured while those producers are
  blocked. These tests return unit results and therefore miss R3's destructor.
- **Truthful return distinction:** `run` returns its closure's normalized result,
  not aggregate cleanup status, so `Ok(())` can coexist with aggregate Panicked.
  This is now explicit in API/readiness/report and is not false aggregate success.
  `track` additionally substitutes aggregate panic at Ready, which exposes R3
  when discarding a nontrivial success. Neither return certifies native commit.
- **Cleanup attempt bookkeeping:** task/timer/feed attempts are marked before
  invoking cancellation/destruction. A caught timer/feed destructor panic can
  no longer hide the attempt. Effects mark an attempted callback before invoking;
  World-loss capture release remains unknown/attempt-not-run, not unsubscribe.
- Original unchanged saved independent probes were separately copied temporarily
  and executed: **3/3 pass**, including World-effect abandonment control.
  [`original-probes.txt`](rndr-flow-f4-rereview-evidence/original-probes.txt)
  records the new timer inside-drop ack=false/receipt=1 and post-success producer
  status Panicked. Original source and original failing receipts remain unchanged.
- No changes to request/commit boundaries: atomic checked lane/work preflight,
  captured native identity and exact owning World, pre-poll output reservation,
  transpose/shared receipts, stale/revision/readonly/composition refusal and
  synchronous authoritative writes remain intact. Fresh source and consumer
  tests preserve zero-write/history/revision/cross-target refusal assertions.
  Generic native-writer traits remain host contracts, not interception of bypass
  writers. Child-first handle attempts and owning-World effect extraction remain
  separate from custom cleanup; lagging Worlds and blocked work remain pending.

## Fresh independent qualification on the exact candidate

New evidence destination only:
[`rndr-flow-f4-rereview-evidence/`](rndr-flow-f4-rereview-evidence/).
All commands used the pre-existing target, `CARGO_INCREMENTAL=0`,
`CARGO_BUILD_JOBS=2`, offline/locked. No release or GUI workspace build.

```sh
sh docs/reports/rndr-flow-f4-correction-gates.sh docs/reports/rndr-flow-f4-rereview-evidence
python3 docs/reports/rndr-flow-f4-rereview-evidence/independent-audit.py
```

The first command is the inspected correction runner with a **new destination**,
not replay of its old evidence. It records exact candidate/parent, each command,
exit and disk readings. Its own audit was also rerun; my separate auditor checks
raw logs, exact-source blobs and stable full per-World baselines independently.

| Fresh gate/proof | Result |
| --- | --- |
| Standard runner | **21/21 zero exits**, `summary.tsv`; this excludes new adversarial failure |
| `cargo test -p rndr-flow --no-default-features -- --nocapture` | **73 passed, 0 ignored** |
| `cargo test -p rndr_core --no-default-features` | **831 passed, 7 ignored** |
| `cargo test -p rndr_liquid` | **16 passed, 0 ignored** |
| `cargo test -p rndr_liquid_ui -- --nocapture` | **115 passed, 2 ignored**; native flow consumer and accepted F1/F2 regressions |
| Executed facade `editing_compatibility`, feature-off / `--features liquid` | **2 + 2 passed**, not merely compiled |
| Strict Clippy | Flow feature-off all-targets and UI all-targets `--no-deps`, both **`-D warnings`**, passed |
| Formatting/diff | `cargo fmt --all --check`, `git diff --check` passed; final flow lint/fmt rerun after removing temporary probes |
| Feature-off/facade compilation | Flow feature-off lib, facade feature-off and Liquid-on lib checks passed |
| Dependency/boundary checks | Normal/build and reverse graphs passed; core/facade/UI production unchanged from F3; no runtime/Tokio/flow-to-UI production edge |
| F4 lifecycle | **360/360 unique cycles**, seeds 1/7/42, 120 each; **720 actual task/producer exit acknowledgments** |
| F1/F2 lifecycle | **300/300 unique teardown rows**; complete **19 counters** equal stable per-World baseline; **zero external ack pending every row** |
| F3 budget proofs | All **nine unchanged budget tests** and retained reviewer regressions passed; full 4,096-slot/1,024-command proof passes with 33,375 charged one-unit visits, not a time claim |
| Corrected permanent matrices | All **12 timer / 20 producer** orderings passed with real producer channel control |
| Exact-source audit | **23 refreshed fingerprints** match current bytes and exact candidate blobs, including all flow Rust sources/tests, flow/consumer manifests, lockfile and consumer proof |
| Original independent probes, new execution | **3 passed**, no failures |
| Additional independent adversarial probes | **2 passed / 1 failed, exit 101**, R3 above |

**Evidence precision:** F4 raw rows print selected fields, not a full vector.
The executed cycle test asserts complete `Counts == baseline` and
`WorkCounts == default` after **every** cycle, and checks actual status ack
before incrementing its count. F1/F2 actually logs all 19 counters; I inspected
`snapshot`/the baseline outside the cycle loop and independently verified each
row, unique cycle, and baseline stability. Exactly one global listener persists;
all other baseline counters are zero. Scalars or final lines were not substituted.

The correction's red/green logs, earlier author qualification and original
independent FAIL are **inherited historical comparison evidence only**.
Everything in the table is a fresh execution for this exact source. The nine
ignored tests remain **unrun**, not passed:
`snapshot_storage_accounting`, `editor_scale_costs`, `scroll_and_fold_stability`,
`unique_line_editor_costs`, `markdown_virtual_cost`, `markdown_idle_shape_cost`,
`model_costs`, `liquid_stream_cost`, `liquid_density_cost`.

## Scope, remaining limits and handback

This is a bounded headless independent correctness review, not release/performance,
GUI/OS/native IME, backend termination, durability, SDK/migration, F5/F6 or whole
project certification. Fixed seeds/controlled orderings are not exhaustive
interleavings. The existing exclusions for true destructor double-panic during
unwind, `panic=abort`, process loss and arbitrary blocked IO remain. R3 does not
need those excluded conditions.

Hosts must keep guards with actual producers, drain the **actual owning Worlds**
and runtime buffers, and retain charges for receipts/clones and blocked work.
A quiet runtime, cancellation flag or logical death is not a join/ack. World loss
is not executed unsubscribe. Callback/destructor/executor duration, exact
synchronous observation/admission and ancestor checks are not retirement-budget
latency guarantees. Retained-token final destruction can be depth-proportional.
F1 whole-stream staging and F2 raw-stream split-CRLF caveats remain unchanged.

No production fixes, manifest/lockfile changes or edits to prior worker/reviewer
evidence. Disposable tests are removed from `crates/`; only this new report and
its new evidence directory are committed. Receipt-only `.gitattributes` preserves
raw `.txt` whitespace without exempting source or Markdown. No unrelated deletion,
new target tree, integration, approval/apply, main merge, push or deployment.
Disk stayed about **7.6 GiB** free, above the runner's **4.25 GiB** stop margin
and requested 4 GiB floor; actual before/after readings are retained.

**Verdict: FAIL for `218e06c…` due to R3, with original R1/R2 closed.** Return to
implementing coder for the scoped missing containment/regression, then fresh
qualification/re-review. Sender assessment remains required. Delivery of this
review commit does not accept F4 or approve the project.
