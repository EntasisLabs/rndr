# F5 independent correction re-review

## Verdicts (deliberately separate)

- **Corrected in-scope safety: PASS, conditional on the documented concrete host contract.** R1's dependency-closed readiness algorithm, R2's final incoming authority validation, and R3's refusal ownership correction passed source review, fresh unchanged repro, permanent regressions, and additional independent safety probes. No additional actionable defect was reproduced in those corrected paths **when all Worlds and actual closing participants are supplied and the final validation really precedes destruction**.
- **Full F5 readiness: BLOCKED / NOT CERTIFIED.** The public Session integration required to establish those preconditions does not exist. Two new desired safety assertions reproduce consequences of the documented seam gap. They are retained failures, not erased, fixed, or counted as qualification successes. A request on a public guard is not proof of actual runner participation; a second sync alone cannot undo an already-set secondary `closed` flag.
- Review acceptance is not project/candidate approval. Sender retains final assessment, scope decisions, corrections and sealed native lifecycle. No approval, merge, push or deployment was performed.

## Exact identity and scope

| Role | Commit |
|---|---|
| Exact clean candidate initially inspected and qualified | `3c59d5ba35ab78d9760f1cb2016d995af77d1823` |
| Corrected source, candidate's parent | `eed1878119f11ee66e73cdb0a6fe24a1027205eb` |
| Previous independent evidence / corrections baseline | `45c8b0cd7f4707446bab5a68fdaa3fab7e9621f9` |
| Accepted F4 / unchanged `main` | `bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5` |

The handback may add this report and distinct evidence only. The final evidence-only child is identified by Git and the terminal handback; all 586 committed source fingerprints must still match. No qualified production/test source, prior review evidence, correction report, design proposal, renderer/runtime/backend/SDK/security/policy implementation, or manifest was changed. No F6/application migration. No unrelated cleanup. Temporary test files existed only during bounded executions and were removed by traps.

Read correction report and seam blocker in full; reviewed the complete corrected production diff against the independent baseline, the inherited F5 native writer/topology/retained implementations and affected renderer consumers. Reviewed the three permanent correction suites (13 distinct correction tests plus repeated unchanged runner tests), retained bounds, writer-freeze, admission/save/quit coverage. Saved production deltas and the real public Session source trace under the new evidence directory for precise provenance; none is a patch to apply.

## R1 — dependency-closed readiness

`ProtectionHost::sync_native` starts with all validated Phase::Ready attempts, repeatedly removes managers whose live document presentations are outside the remaining set, and publishes readiness only after convergence. This is a greatest fixed point: blockers propagate along arbitrarily long finite dependency chains; jointly ready cycles are retained, not deadlocked by a least-fixed-point construction. Publication is independent of attempt order. Withdrawal invalidates previously published Ready while retaining the writer/topology protection. Register-world admission stays frozen.

Fresh permanent matrix and unchanged original safety repro use the **actual byte-unchanged `rndr_render/src/close.rs` consumer**, not an imitation, covering six orderings, external/application blockers, chains, cycles and withdrawal. Properly participating independent secondary closes remain possible. Source review confirmed that manager destruction, not presentation count alone, is the ownership hazard.

**Essential boundary:** `sync_native` currently sees public guard states, not private `CloseSlot.active/requested/closed` intent. A fabricated guard request can therefore be mistaken for a closing participant. The new seam assertion below proves why the actual-participation requirement is necessary; the corrected fixed point is not a substitute for a Session seam.

## R2 — final incoming identity / ownership

`Retained::tick` validates the incoming entry at admission and again at the final callback-free boundary, including Select's retained destination, not only newly constructed Push/Replace entries. It re-resolves command/tab/entry identity, live owner ancestry and original affected/tabs before commit. `ProtectionPass::validate_entry` requires the exact registered target, live manager and document, native-ID existence/revision access, held scope/document lease, and `Arc::ptr_eq` to the **same held allocation owned by that entry**. An unrelated valid matching lease cannot authorize it.

Permanent coverage freshly exercised empty and nonempty histories, wrong scope/document/native authority, owner/manager loss during decisions and saves, queued loss, stale select, and destructor/reentrant submission. New independent probes add:

1. Push and Replace with a wrong owned lease **while a separate valid same-scope/document decoy lease exists**: both fail and preserve original entries/selection/owner.
2. Push-pruning-forward and Replace, crossed with Save and Discard: incoming native ID disappears from a separate still-live manager after AwaitingDecision. All four fail before installation or disposal; the complete original Fixture snapshot (bytes/revisions/history/selection/resource counts) remains equal. This is native identity loss, not merely coordinator owner loss.
3. Queued Select loses its captured destination while another operation holds its tab: cancellation of the blocker does not reinterpret the target or change current selection.

All four new independent tests pass (multiple coordinates inside tests). Failure can retain freeze until explicit resolution; this is not partial mutation of the original history. The documented threat model is trusted host metadata with callback-free validation; arbitrary malicious custom TransitionHost implementations are not certified.

## R3 — atomic native refusal and technical failures

`choose_native` checks operation/round exhaustion, live Coordinator World, exact supplied bound World, existing guard and exact request before retry/removal. Cancel mutates only after these checks. Save/Discard remove only for callback-free concrete choice and **reinsert on either Result**; round/request retry happens only on success. Missing/foreign/stale admission is not cancellation and does not rotate the decision.

Remove-then-fallible audit:

- native choice retains/reinserts on Err;
- synchronous native save catches the actual persistence closure's panic and reinserts the attempt on Result;
- owned-result completion reinserts on refusal; dropping the consumed undeliverable SavedWork marks technical loss, not consent;
- sync's temporary attempt vector is restored after ordinary fallible World/guard lookup; no user persistence callback runs there;
- retained choice operates in place, with checked round advancement and no premature guard take;
- retained commit completes identity/ownership validation before changing original entries/selection, and drops retired application values outside inbox borrows. Reentrant submissions are deferred/revalidated.

Fresh permanent tests cover omitted Worlds, foreign Coordinator, stale decision/request, Save/Discard/Cancel, exact freeze/decision preservation, technical failure, dropped work/result, save panic and retry. New independent owned-result probe refuses omitted-World delivery with `Err(Pending)`, preserves the exact native state and writer freeze, then observes Failed on resync. Repeated omitted Save/Discard/Cancel against that Failed decision retain the same decision and freeze. This expected technical failure is not a safety failure.

No claim is made that arbitrary panicking process infrastructure, OOM/abort, or malicious unsafe code has transactional recovery. The concrete user save callback is inside the owned panic boundary; no new unbounded executor/runtime was introduced.

## Session seam: confirmed blocker and scope assessment

### Real existing PUBLIC integration

Inspected actual `rndr_render::Session` construction/API and `RunnerState::reconcile_close`, not just private includes. Session takes Apps/window setup/configuration; its run path moves them into private runner-owned slots. App update callbacks have one World. There is **no public all-World production visitor/hook at native destruction**, nor public actual-close cohort metadata. Independently assembled HostWorld slices in tests do not demonstrate a usable Session integration.

Real ordering (saved in `public-session-trace.txt`):

- `about_to_wait` spawns queued windows, pumps slots/hidden frames, calls `reconcile_close`, then `sync_flow`.
- `window_event` delivers the individual slot event, then calls the same `reconcile_close` directly. A hook only in about_to_wait would miss this destructive path.
- `reconcile_close` calls private CloseDrain::step. That can consume Ready and set secondary `closed`. If process exit is not committed, newly wanted hidden frames can then run. Secondary `slot.close()` and slot retention/removal occur afterwards.
- Process exit has `hub.try_commit_exit`'s atomic queue/admission fence. That is necessary existing behavior, not an all-World protection hook and not a fence around the later secondary hidden-frame path.

This confirms the reported static production gap. **No public native/GUI success reproduction is claimed:** precisely because the required public integration is absent. Source inspection of the real public entry points establishes absence/order; private runner tests below only characterize its consumption semantics.

### Reproducible unresolved safety assertions (high, full-F5 blockers)

Saved unchanged-runner harness: `seam-boundaries.rs`, `run-seam-boundaries.sh`, `seam-boundaries.txt`.

**B1 — actual close participation is not implied by CloseGuard state.** A really active secondary manager A and inactive B; B holds A's document and has its own registered document. Manually request B's public guard, protect/Discard both and sync all Worlds. The fixed point marks both Ready, but the actual runner consumes only A: `A_closed=true B_active=false B_closed=false`. Desired assertion that A survive B's non-closing live presentation fails. This establishes the missing precondition, not a claim that Session automatically generates this fabricated request. The host API currently cannot observe the real cohort to enforce it.

**B2 — secondary consumption is sticky across later invalidation.** The real unchanged runner sets `closed=true`, guard is then invalidated as a callback could do before destruction, and a further runner step leaves `closed=true guard=Pending(...)`. Desired assertion that the secondary survive fails. Thus merely adding a hook after the current step and invalidating Ready on hook error is **insufficient**. Sticky tentative destruction must be withdrawn/recomputed, or final consumption must be deferred until after all callbacks and the hook.

Run returns **exit 101, 7 unchanged runner tests pass / 2 new desired safety assertions fail**. Both failures are saved and remain unresolved. The second test manually simulates the callback mutation; it is not a public Session/GUI execution or an implemented renderer fix. The real Session source independently verifies that callbacks occupy that ordering window.

### Minimal opt-in proposal: necessary, plausible, not yet implementation-ready proof

The blocker document proposes the right narrow renderer seam rather than a new document store/runtime. I assess it **necessary and directionally sufficient only with the following concrete semantics**:

1. Opt-in callback gets nonescaping mutable access to **all retained Worlds**, with exact stable identities and actual requested/active/cancelled/tentative-close intent. Include visible/hidden/primary/secondary/just-created and unguarded participants. No private-field tricks by applications and no unsafe aliasing/escaping World borrows.
2. Public guard requests cannot independently authorize cohort membership. Since current sync_native accepts no runner-intent parameter, the host wrapper must reject guard/cohort mismatches or a separately reviewed narrow API must carry authoritative participation. Merely exposing an unused active flag does not fix B1.
3. Common reconcile boundary covers **both event paths**. Run pending dynamic/hidden callbacks and settle relevant topology before final host validation. No application/save/build callback may run between successful validation and actual native destruction. If work appears later, defer consumption and revalidate; do not silently skip hidden Worlds.
4. Errors **and panics** fail closed for the entire opt-in destructive attempt: no process exit, no secondary close/removal, no dropped freezes, and no successful decision invented. B2 requires split prepare/consume or explicit reversible/recomputed tentative flags. An after-step hook alone is not sufficient. Panic testing must exercise the real public renderer hook, not only save closures.
5. Retain the atomic process queue fence: a concurrent open either joins a future validated cohort or is rejected after committed shutdown. Preserve queue ownership and generation consistency. Secondary destruction also needs a stable cohort/no-build-callback interval; do not treat the process-only queue fence as covering it automatically. Newly admitted Worlds or native stores force a new validation pass.
6. No-hook applications preserve current primary/secondary/quit/cancel behavior and legacy API. The hook is optional and fail-closed only when opted in; core document semantics, scheduler/backend/security policy and F6/application migration remain outside the proposed change.

This is design assessment, **not authority to implement**, and not a claim that the current proposal has already passed integration. Renderer source/behavior changes and public-path tests require sender's explicit scope decision. Minimality does not mean one callback line: correcting the existing consume/callback/destruction ordering is part of the smallest coherent seam. No need was established for a new SDK, security policy, document ownership graph, broad executor or application migration.

Required future proof before any full F5 PASS: real public Session all-World hook with two/three native Worlds, proper and mismatched participation, chains/cycles/withdrawal, hidden/dynamic callback mutation, both event paths, hook Err/panic, secondary tentative-close revocation, open race at queue fence, no-hook compatibility. Headless private includes cannot substitute for that public integration proof.

## Fresh verification and exact commands

Distinct new directory: `docs/reports/rndr-flow-f5-rereview-evidence/`. Original independent and correction evidence remains byte-unchanged.

Executed serially from the authorized checkout:

```sh
sh docs/reports/rndr-flow-f5-correction-evidence/run-corrected-head-probes.sh \
  > docs/reports/rndr-flow-f5-rereview-evidence/original-repro.txt 2>&1
sh docs/reports/rndr-flow-f5-corrections-gates.sh \
  docs/reports/rndr-flow-f5-rereview-evidence/qualification
sh docs/reports/rndr-flow-f5-rereview-evidence/run-additional.sh \
  > docs/reports/rndr-flow-f5-rereview-evidence/additional.txt 2>&1
sh docs/reports/rndr-flow-f5-rereview-evidence/run-seam-boundaries.sh \
  > docs/reports/rndr-flow-f5-rereview-evidence/seam-boundaries.txt 2>&1
python3 docs/reports/rndr-flow-f5-corrections-audit.py \
  docs/reports/rndr-flow-f5-rereview-evidence/qualification
```

The saved original repro runner differs from the original parent-only runner only in corrected-source fence/name; it `cmp`s the unchanged original assertions before compiling. Exact SHA256 of `adversarial.rs`: `998eaf8c25ee4daaba7afd1d962ce402a9ea0239248314dc44748b44274eabf4`. **13/13 pass, exit 0.** Additional safety: **4/4 pass, exit 0**. Seam boundary characterization: **7 pass / 2 fail, exit 101**, deliberately separate.

**22/22 fresh standard gates, all exit 0.** Summary includes exact per-gate disk bounds; each individual log starts with its exact command. Environment: offline/locked Cargo, jobs=1, incremental=0, dev/test debug=0; compatible existing target/cache reused, no cleanup. Qualification source cleanliness/fingerprints were measured with temporary test files removed. Fresh auditor and post-run repeat both pass all **586** committed fingerprints against exact HEAD blobs. Final evidence-child audit repeats this condition.

| Evidence | Fresh result |
|---|---|
| Flow full suite | 135 passed, 0 ignored |
| Explicit host-adapter repeat | 14 passed |
| Core | 834 passed, 7 ignored |
| Liquid / consumer | 16 passed / 115 passed, consumer 2 ignored |
| Compatibility feature off / liquid | 2 + 2 passed |
| F1/F2 | 300 cycles, all 19 counters equal per-World baseline, external pending=0 |
| F3 | all nine budget proofs retained/passed |
| F4 | 360 cycles / 720 actual acknowledgments, full resource equality; unchanged reviewer probes, 20 producer + 12 timer + 11 tracked + 45 task panic coordinates |
| F5 | 360 full Counts/WorkCounts/HistoryCounts baselines + 360 terminal-window cycles; final root/World teardown to zero |
| Lint/build/deps | all scoped lint, facade off/liquid, headless feature-off, dependency/reverse-dependency gates passed |
| Contract/source retention | F0–F4 protected contracts/proofs, manifests, work/request/budget sources, renderer and prior reports unchanged; 586 exact-source fingerprints |

Nine inherited ignored tests explicitly **NOT RUN**: core snapshot_storage_accounting, editor_scale_costs, scroll_and_fold_stability, unique_line_editor_costs, markdown_virtual_cost, markdown_idle_shape_cost, model_costs; consumer liquid_stream_cost and liquid_density_cost. No release performance/RSS/total latency, platform UI, native event-loop, backend quiescence or production durability certification is implied.

Disk floor during qualification was 5,392,436 KiB at recorded gate boundaries, above required 4,456,448 KiB (4.25 GiB); active gate monitor detected no floor breach. Later probes/checks also remained above the floor, approximately 5.1 GiB free. No unrelated artifacts removed.

Prior sender sandbox filesystem `PermissionDenied` is an execution-environment limitation, not evidence of a safety defect. This fresh authorized checkout run exercised the durable-save test successfully. No such permission failure occurred in this re-review. The two seam failures are Rust safety assertion failures with concrete runner state, **not PermissionDenied**. No diagnostic exit was reinterpreted as a green safety gate.

## Handback

Recommended sender disposition: corrected-safety evidence can be accepted **for partial review**, but retain full-F5 blocker and require a narrowly authorized renderer seam stage if desired. Keep candidate/project unapproved. Report, commands, logs, 586-source identity evidence and both reproducible unresolved seam assertions are retained. Production and permanent tests are unchanged; no fix was attempted.

Evidence packaging note: the staged raw Cargo logs and saved unified source diff contain original trailing/EOF whitespace. A whole-evidence `git diff --cached --check` returned 2 for that formatting only, before any commit attempt. Raw receipts were deliberately preserved, not normalized. The report and authored probe/runner files pass scoped whitespace checks; exact-source qualification's whitespace gate had already passed before evidence packaging. This is neither a source defect nor a safety-test failure.
