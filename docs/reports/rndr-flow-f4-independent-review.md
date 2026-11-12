# Independent adversarial F4 review — FAIL

Date: 2026-11-12. Review target **`9e09c575f222384d5e3c843511552f64d0cbbf4d`**, directly atop accepted F3 **`f16e5572d35450d586dd4f10e00591fa36fb2b04`**.

**Recommendation: do not accept/integrate this candidate. Two P2 correctness findings require correction and fresh review.** Standard qualification passes; it does not cover the failing compositions below. This is an independent reviewer deliverable, not an implementation, sender acceptance, project approval or integration receipt.

The commit carrying this report is a **review-only documentation/evidence descendant** of the target, not a corrected F4 candidate. Its OID is supplied in the terminal handback. Production source, manifests, lockfile and committed worker evidence are untouched. Temporary integration probes were removed from `crates/` after execution. No main integration, push or deploy occurred.

## Severity-ranked findings

### R1 — P2: cancelled timer acknowledges before payload destruction and fails to normalize a single destructor panic

**Location:** `crates/rndr_flow/src/work.rs:627–635`, `TimerCompletion<D,T>::drop`; runtime timer payload destruction reached from the existing frame update. This is not a proposed core change.

`TimerCompletion::drop` sets `exit_acknowledged = true` and (if running) `DroppedUnknown` in its destructor body. Its `completion` field, including the user payload, is only destroyed **after** that body returns. There is no catch/terminal normalization around that field destruction. A single payload destructor panic therefore escapes `Sim::update`, with status still `DroppedUnknown`, not `Panicked`. This differs from the correctly ordered/caught `Tracked::drop_inner` and `Effect::drop` paths.

Minimal deterministic sequence, no sleeps or private-counter injection:

1. Bind one Sim World; start an owned long-delay timer with a payload whose destructor records `WorkStatus` and then panics once.
2. Drop the owning scope; run one-unit coordinator pumps so the timer handle is cancelled.
3. Call `Sim::update` within the review harness's `catch_unwind`.
4. Observe the status inside the payload destructor and after the unwind.

Fresh actual result:

```text
timer update panicked=true
during destructor: retired=true cancellation_signalled=true cleanup_attempted=true
  exit_acknowledged=true pending_deliveries=1 terminal=DroppedUnknown
after: exit_acknowledged=true pending_deliveries=0 terminal=DroppedUnknown
```

Expected: do not acknowledge actual destruction before it has returned/unwound; normalize this single unwind to `Panicked` at the owned boundary and preserve independent progress. The documented exclusions cover arbitrary **double** destructor panic, abort/kill and external IO, not this deterministic single unwind. Public `Observation` says exit is acknowledged only after actual wrapped exit/drop. The F4 report also claims honest destructor/terminal normalization.

**Impact and limits:** cancellation can interrupt the host update and hide a known panic as unknown. An observer sees a completed exit while payload destruction has not completed. **The receipt is still pending during destruction, so this probe does not demonstrate premature work-slot reclamation, a capacity bypass, or lost domain data.** Those stronger claims are not made.

Full minimal source: `rndr-flow-f4-independent-evidence/independent_review.rs`, test `timer_payload_destructor_must_be_normalized_before_exit_ack`.

### R2 — P2: real successful feed-producer exit overwrites a known cancellation/destructor panic

**Location:** `crates/rndr_flow/src/work.rs:173–190`, `ProducerExit::run`, composed with `Coordinator::handle_step`'s caught feed cancellation failure. Related transition to review: `ProducerExit::drop` at 200–207 also unconditionally changes unacknowledged terminal status to unknown; the deterministic failing probe below exercises `run`, not a separately claimed Drop reproducer.

`handle_step` correctly catches a queued feed payload's destructor panic and sets the work status to `Panicked`. `ProducerExit::run` subsequently assigns `Succeeded` unconditionally when the **actual producer closure** returns `Ok(())`. This erases the already observed failure. The task `Tracked::poll` path preserves a pre-existing panic; this producer path does not.

Minimal deterministic sequence:

1. Create a bounded owned feed. On a real producer thread, run `exit.run` around sending one payload whose destructor panics once; pause that producer on a channel handshake after enqueue.
2. Drop the owner and pump retirement. Feed cancellation clears its queue; the adapter catches the panic. Assert terminal `Panicked` and producer exit not yet acknowledged.
3. Release the channel handshake so the **actual producer closure** returns `Ok(())`, and join that producer thread. The wrapper covers the send, wait and exit; it is not an observer acknowledging unrelated work.
4. Assert the known panic remains visible.

Fresh actual result:

```text
before producer exit: terminal=Panicked exit_acknowledged=false
after producer success: retired=true cancellation_signalled=true
  cleanup_attempted=false exit_acknowledged=true pending_deliveries=0 terminal=Succeeded
assertion failed: left Succeeded, right Panicked
```

This is not an observer manufacturing an early acknowledgement: the producer closure actually exits. Its success simply cannot retroactively make failed cancellation/destruction successful. The missing terminal-precedence invariant violates F0 §5/epic §3.6 and the candidate's explicit no-panic-to-success claim.

**Impact:** clients inspecting the terminal status after drain see false success, not the known failure. The probe does not claim a stale write or a missing exit charge. Preserve failures while recording the producer's actual exit independently; permanent tests need both operation orderings and relevant terminal combinations. No fix was implemented here.

Full minimal source: same file, test `producer_success_must_not_erase_payload_cleanup_panic`.

## Deterministic reproduction and control

From the review tree (or with these review files copied into the exact candidate), run:

```sh
sh docs/reports/rndr-flow-f4-independent-evidence/reproduce.sh
```

The script refuses an existing probe file or changed candidate source, checks the 4 GiB floor, installs only a disposable integration test, uses the **existing target tree**, runs

```sh
CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 \
  cargo test --offline --locked -p rndr-flow --test independent_review -- --nocapture
```

and removes the probe via a trap. Expected on the reviewed candidate: **exit 101, 1 passed / 2 failed**. Complete final output: [adversarial.txt](rndr-flow-f4-independent-evidence/adversarial.txt). The payload panics are intentional, singly controlled, and caught by the appropriate reviewed boundary or the outer review harness; the failing assertions above, not merely printed panic hooks, establish the defects.

The third independent probe, `world_abandonment_must_not_ack_before_capture_destruction_or_hide_panic`, is a **passing control**: dropping a World with an owned effect capture that panics during destruction records `exit_acknowledged=false` inside destruction and `Panicked`/acknowledged afterward. This rules out treating all World-specific destructor handling as broken and highlights the timer inconsistency. Core/flow effect containment means the outer World drop need not propagate that panic.

## Independent fresh qualification (not inherited worker results)

All commands below ran at the exact candidate, using `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, offline/locked, and the pre-existing worktree target. Raw logs are separate in [rndr-flow-f4-independent-evidence/](rndr-flow-f4-independent-evidence/). The copied gate script changes only evidence destinations/auditor destination; committed worker logs were not overwritten.

| Fresh action | Result |
| --- | --- |
| Independent copy of 20-gate runner: `sh docs/reports/rndr-flow-f4-independent-evidence/gates.sh` | **20/20 exit 0**, commands/free-space readings in `summary.tsv` |
| Flow feature-off unit/integration/doctests | **66 passed**, 0 ignored; includes unchanged F3 units, nine budget proofs, retained reviewer probes and lifecycle proof |
| Full feature-off core tests/doctests | **831 passed, 7 ignored** |
| Parser `rndr_liquid` | **16 passed**, 0 ignored |
| Full `rndr_liquid_ui` tests/doctests | **115 passed, 2 ignored**, including both flow consumer tests and unchanged accepted regressions |
| F4 repetitions | **360/360 unique cycles**, seeds 1/7/42, 120 each; **720** task/producer exit acknowledgments |
| Unchanged F1/F2 teardown repetitions | **300/300**, all **19** vector components equal their per-World baseline and zero pending external acknowledgments on **every** row |
| Strict Clippy | Flow feature-off all-targets and consumer all-targets `--no-deps`, both `-D warnings`, passed; rerun again after disposable probes were removed (`lint-*-exact.txt`) |
| Formatting/diff | `cargo fmt --all --check`, `git diff --check` passed; formatting rechecked after removing probes |
| Facade feature-off/Liquid-on checks; compatibility target compile gates | Passed |
| **Additional actual execution**, not just worker compile gates | `cargo test --offline --locked -p rndr --no-default-features [--features liquid] --test editing_compatibility`: **2 + 2 passed** (`compatibility-*-executed.txt`) |
| Dependency and production-boundary gates | Passed; no new runtime/Tokio/flow-to-UI production dependency; UI flow edge dev-only |
| Additional adversarial probes | **1 passed / 2 failed**, exit 101: R1/R2 above |

Nine ignored core/UI tests remain **unrun**, not counted as passes. Worker evidence was inspected for comparison only; the table above is the reviewer's own fresh execution. The inherited report's 20 green gates are not being substituted for independent proof.

### Full-vector and exact-identity audit

The review's separate [independent-audit.py](rndr-flow-f4-independent-evidence/independent-audit.py) checks:

- Exact candidate and parent OIDs at execution time; initial tracked tree clean.
- **All 20 recorded source fingerprints** against both current bytes and `git show <candidate>:<path>`.
- No tracked source changes under `crates/`, workspace manifest or lockfile; protected `crates/rndr_core`, facade and `crates/rndr_liquid_ui/src` unchanged from F3.
- Unique/comprehensive F4 seed/cycle coordinates and exact cumulative acknowledgement counts.
- Every F1/F2 raw 19-element vector against its full baseline, not a scalar total or final line.

Fresh output: [independent-audit.txt](rndr-flow-f4-independent-evidence/independent-audit.txt). The original auditor was also rerun with only its input destination changed.

**F4 evidence precision:** F4 log rows print selected zero/baseline fields, not a raw complete Counts vector. I inspected the executed test body (`tests/f4.rs:716–725`): it asserts complete `flow.counts() == baseline` and `flow.work_counts() == WorkCounts::default()` after every cycle. Thus full F4 equality is a compiled-test assertion, not an invented raw-log vector audit. F1/F2 really do log all 19 elements.

## Contract/implementation audit and remaining matrix gaps

Reviewed full F0 contract and epic, both parent and candidate F4 readiness documents, candidate report, implementation diff, tests, runner/auditor and actual core runtime/feed/timer/source plus UI consumer contracts. The original readiness is a governing requirement source, not replaced by the worker's green matrix. No broader native/application certification is inferred. The old readiness's required outcome/panic cases are not discharged by newly worded completion claims: R1/R2 are missing combinations despite the report's “no required headless row remains proposed” statement.

| Area | Independent assessment / limits |
| --- | --- |
| Atomic admission before cancelling good work | Checked lane/work serial/capacity preflight order, 32-lane bound, checked generation changes and returned rejected values; fresh near-MAX/capacity tests passed. MAX feed testing is a scalar production preflight test, not a fabricated real 2^64-epoch run. |
| Identity and authoritative writes | Request authority/scope/activity/lane/World plus captured native target/revision/operation; no selected-target fallback. Generic writer is a host contract, not interception. Source adapter checks exact batch target/base/origin and calls native commit synchronously; ordered native revision receipts advance only for successful own writes. |
| Source/Input refusals | Fresh source and UI tests cover published Input, readonly, composition, stale revision, wrong operation/target, no-op and native failure with zero text/history/revision/event/cross-target change; no rebase/replacement of ordered chunks. Native composition proof is headless, not native IME certification. |
| Suspension/hidden | Unrecoverable ordered activity suspension refused; hidden retains work; resume waits for retirement. No new blanket suspension semantics for descendants or backend work are inferred. |
| Real runtime/producer exits | Reviewed and reran custom executor, Rc local, retained Job/start-panic, poll/drop, cooperative/blocked producer handshakes. Unknown blocked jobs remain charged. R2 exposes missing interaction between cancellation failure and later actual producer exit. |
| Reserved/queued/published/cloned receipts | Reviewed pre-poll reservation, transpose, clone sharing and final disposal; fresh receipt-capacity tests passed. Logical ineligibility is not reclamation. R1's receipt still protects capacity despite its false exit timing; no measured capacity bypass claimed. |
| Owning World and cleanup order | Actual World-local effect extraction/catches, child-first handle retirement before custom cleanup, wrong-World isolation, reverse cross-World ordering and app-scope-before-World lag regression passed freshly. Independent World-destruction panic control passed. |
| Full queue/bounded progress | Fresh 4,096-slot/1,024-command test passed with one-unit pumps; inspected incremental traversal/ordered cursors and no added hidden global/ancestry scan in the charged retirement step. Full workload reports 33,375 charged units, not elapsed time. F3 budget/regression tests passed. |
| **Missing timer destructor matrix** | R1: cancelled future timer payload destructor panic, acknowledgement during destructor, and independent progress after that panic were not covered. Add cancellation and publication/drop order variants; this review directly reproduces cancellation. |
| **Missing terminal precedence matrix** | R2: failure during cancellation, then real producer success/failed/drop. Only success-after-panic is directly failed here; related orderings should be permanently covered without conflating exit acknowledgement and outcome. |
| Exhaustiveness | Fixed seeds/manual schedules are bounded evidence, not all interleavings. No broad random soak, release performance, arbitrary double panic/abort, native windows, F5/F6 or Medousa SDK/backend proof performed. |

## Scope, disk, and recommendation

No production implementation/fix was made. Independent probes lived temporarily under `tests/`; only documentation, saved probe source and review evidence are retained. No changes to committed worker evidence. No redundant target tree, workspace/GUI build, unrelated deletion, integration, push or deployment. Facade compatibility test execution compiled existing render dependencies but opened no window and was not a GUI/workspace build. Free disk stayed above the 4 GiB floor (about 9.1 GiB initially; about 7.8 GiB after additional facade execution). No cleanup of another worker's artifacts was attempted.

Keep the accepted limitations explicit: no total-latency guarantee; synchronous exact observation/admission/ancestry costs outside the retirement budget; retained-token final destruction can be depth-proportional; hosts must drain each actual owning World/runtime; unknown blocked work remains charged; host writer traits are contracts, not automatic interception; F5/F6/Medousa/GUI/backend SDK cancellation are excluded.

**Exact-candidate verdict: FAIL.** Request a scoped implementing-coder correction for R1/R2, permanent regressions, fresh full qualification and a new independent exact-head review. Do not treat this review deliverable or the standard green gates as sender/project acceptance.
