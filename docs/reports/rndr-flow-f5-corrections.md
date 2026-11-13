# F5 independent-FAIL corrections — safety fixes qualified; Session seam blocked

## Outcome and exact identity

**Partial outcome, not full F5 completion or native integration certification.** R1–R3 are corrected and all requested fresh headless safety/qualification assertions pass. The required real public `Session` integration path does **not** exist in the inspected API. Its concrete blocker and minimal proposed renderer seam are documented for sender assessment, not implemented by expanding this assignment.

- Correction baseline / preserved independent review child: `45c8b0cd7f4707446bab5a68fdaa3fab7e9621f9`.
- Corrected and freshly qualified source commit: **`eed1878119f11ee66e73cdb0a6fe24a1027205eb`**, direct child of that baseline.
- Original rejected candidate: `286818f9bcac048b725a650594ea88fa9b1d1624`.
- Accepted F4 / unchanged local `main`: `bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5`.
- Final handback adds only correction report/evidence and a verified documentation line citation correction to the source commit; its exact child hash is returned with the handback. No crates/manifest/lockfile bytes change after qualification.

Sender accepts review work, not the original candidate. This work does not approve, merge, push, deploy, implement F6/application migration, or change renderer/runtime/backend/SDK/security/policy authority. Sender retains independent re-review, corrections and sealed native integration.

## R1: dependency-closed manager cohort

`ProtectionHost::sync_native` now starts from **all validated Ready attempts** and monotonically removes any manager whose held document lease belongs outside the remaining set. Removal repeats to a greatest fixed point. An application lease blocks B, and removing B also removes A when B presents A, through arbitrary represented chains. Starting with the full set preserves legitimate jointly closing cycles without external blockers. Publishing native Ready uses membership in the resulting set, not merely the internal phase. Previous Ready is invalidated on withdrawal; owned freezes remain held. No document store copy or implicit manager transfer is introduced.

Permanent regressions:

- The **unchanged** original R1 desired-safety assertion now reports `A=Pending`, `B=Pending`, and unchanged runner `closed A=false B=false`.
- `manager_fixed_point_chain_cycle_order_and_withdrawal` executes **30 scenarios**: all six actual World iteration orders for application-blocked three-manager chains, free cycles, cycle cancellation after Ready, application-blocked cycles and free chains. Each calls the actual unchanged `CloseDrain::step`, rather than asserting private flow phases alone. Safe chains/cycles really get all expected secondary close flags; blocked/withdrawn cohorts do not. The unrelated primary stays open.

This requires actual native runner participation and immediate host validation, not fabricated guard requests for a non-closing slot. The public production scheduling/access blocker below remains explicit.

## R2: final incoming identities and ownership

Retained commit now re-resolves the exact command, tab, owner/parent and affected/truncation set **after** waiting and validating outgoing protection. It additionally validates the incoming entry immediately before metadata mutation. The required callback-free `TransitionHost::validate_entry` contract is implemented by `ProtectionPass`: live flow document and manager, exact registered native target, exact native identity existence, and the **same lease allocation** held for the proposed scope/document. The existence of some other matching lease is insufficient. Push/replace/select are checked at admission and final commit. Select also refuses a disposed retained destination. No callback/await occurs between the final checks and metadata mutation.

The original different-document repro now reports `Failed`, incoming scope `Disposed`, **old scope still Visible and installed**, not two disposed scopes after a false commit. The same-document control continues to refuse. Matrices cover push branch truncation and replace during AwaitingDecision and Saving, exact lease-to-scope/document mismatch, unregistered target combinations, missing native identity, real incoming manager World destruction, queued incoming loss and dead selection. Valid originals/selection remain untouched on refusal. The small metadata-only test double implements the new required host method; no pre-F5 SDK or facade change is made.

## R3: native refusal retains owned protection

Native choice now preflights the World, coordinator membership, exact current native request and CloseGuard before removing the attempt. The fallible protection choice result is captured and the attempt is reinserted on either result. **Native retry happens only after successful Save admission.** Thus omitted/foreign/stale admission cannot advance or drop the prior operation/decision/freeze or partially retry the native request.

The unchanged omitted-World repro now returns `Err(Pending)` with `freeze=true` and the **same exact DecisionId**. Additional tests cover omitted Save/Discard/Cancel, a foreign World, foreign coordinator, stale decision/changed native request, omitted save execution, technical save failure and save panic. Failure stays owned/frozen; it is not inferred Cancel/Discard.

Remove-then-fallible audit:

- `choose_native`: all World/guard/request preflight precedes removal; fallible protection choice is captured and always reinserted. Explicit Cancel removes only after successful native cancel admission.
- `save_native` and `complete_native_owned_save`: already capture the pass result and reinsert before returning. Consuming/loss of an actual owned save result can mark technical failure, but does not cancel/drop the attempt freeze.
- `sync_native`: now restores its temporarily taken attempt vector on **every ordinary fallible return** from validation/readiness publication. Missing-World sync retains existing protection as before. Idle removal is explicit native cancellation; terminal World loss is handled only by reconciliation.
- `unregister` validates all fallible checks before record removal. Retained cancel/reconcile/commit removals are explicit cancellation, terminal loss or the post-validation nonfallible metadata commit. Reentrant payload destruction stays outside inbox borrows, with the original destructor-submission assertion preserved.

These guarantees concern typed refusal/owned technical failure, not arbitrary malicious resource replacement, process loss, dishonest persistence success or rollback of external saves that already completed.

## Real public Session seam: concrete blocker

See [`../design/rndr-flow-f5-session-seam-blocker.md`](../design/rndr-flow-f5-session-seam-blocker.md) and corrected host contract. The existing public `Session` has only `new`, `window`, `run`; live slots, their Apps, all-World reconciliation and close consumption are private. Per-App systems cannot supply simultaneous access to every private session World. Both `window_event` and `about_to_wait` call private reconciliation after callback batches. There is also a hidden-frame callback lane **between** `CloseDrain::step` and actual secondary `slots.retain` destruction. A hypothetical pre-step-only hook would not establish the required final boundary.

No public Session production integration proof is claimed. Private-module includes are real headless runner-consumption tests, **not** a public host integration path. The minimal proposed separately authorized scope is an opt-in Session all-World/actual-close-participation visitor plus fail-closed request/consume ordering and callback/topology revalidation in the existing renderer close owner (`rndr_render/src/lib.rs`, and `close.rs` only if splitting/rechecking is needed). The proposal includes error/panic behavior, both event paths, hidden callbacks, dynamic opens and no-hook compatibility tests. It introduces no renderer-to-flow dependency or application migration. **No such renderer diff was applied.**

## Fresh exact-source verification

Original review report, saved `adversarial.rs`, canonical independent logs and prior qualification/evidence/scripts remain unchanged. The saved repro SHA-256 remains `998eaf8c25ee4daaba7afd1d962ce402a9ea0239248314dc44748b44274eabf4`; the permanent `f5_corrections.rs` is byte-identical.

Executed at corrected source `eed1878`:

```sh
sh docs/reports/rndr-flow-f5-correction-evidence/run-corrected-head-probes.sh
sh docs/reports/rndr-flow-f5-corrections-gates.sh \
  docs/reports/rndr-flow-f5-correction-evidence/qualification
python3 docs/reports/rndr-flow-f5-corrections-audit.py \
  docs/reports/rndr-flow-f5-correction-evidence/qualification
```

The separately named corrected-head runner fences production bytes against the exact source commit, copies the original saved repro unchanged to a separately named temporary test, compares its bytes, runs serially and removes only that temporary file via trap. **13/13 pass**, including all six original independent review assertions and all seven unchanged runner tests. The previous sender sandbox filesystem `PermissionDenied` was an environment-only fourth failure, not a safety defect; the filesystem test **passed in this correction environment**. Assertions were neither skipped nor weakened to obtain this result.

All **22/22 standard gates pass**, followed by a post-22-row audit:

| Suite | Passed | Ignored |
| --- | ---: | ---: |
| Flow | 135 | 0 |
| Explicit native host adapter rerun (flow subset) | 14 | 0 |
| Core | 834 | 7 |
| Liquid parser | 16 | 0 |
| Consumer | 115 | 2 |
| Editing compatibility feature-off / liquid | 2 / 2 | 0 |

Flow rises from 108 to 135 because the three permanent correction files execute 27 tests: 13 distinct correction tests and 14 repeated unchanged runner test executions. The separate saved repro and explicit host reruns are **not additive distinct coverage**. The correction gate set is unchanged from the original except separate output/auditor routing. The separately named auditor changes only the exact flow total to 135 and explains the added tests; every original resource/cycle/panic/source assertion is retained.

Fresh preserved proofs:

- **F1/F2:** 300 teardown cycles, all **19 counters** equal per-World baselines; global listener 1, all other counters and external acknowledgment pending 0.
- **F3:** all nine budget proofs.
- **F4:** 360 cycles / **720 actual exit acknowledgments**, full Counts/WorkCounts equality; producer/timer matrices (20 + 12), tracked/task branches (11 / 45), unchanged reviewer 3+3 and six-case paused-destructor probe twice.
- **F5:** 360 full Counts/WorkCounts/HistoryCounts baseline cycles after bounded warm-up and final root/World zero; 360 terminal-window admission/reclamation cycles with shared-document retention and exact baselines.
- **586 source fingerprints** (583 inherited + three new test files) equal current bytes and exact committed blobs. F0–F4 protected contracts, old reports/evidence, manifests, facade/runtime/SDK/render owners remain unchanged. The correction itself makes no core source edits relative to the reviewed baseline.
- Formatting, source whitespace, strict scoped lint, feature/facade and dependency gates pass. Raw output is preserved as output, not sanitized to manufacture whitespace success.

## Evidence, environment and limits

All new receipts are separate under `docs/reports/rndr-flow-f5-correction-evidence/`:

- `adversarial-corrected-qualified.txt` and `run-corrected-head-probes.sh`.
- `qualification/summary.tsv`, exact per-gate commands/results, `environment.txt`, `source-digests.txt`, `audit.txt`.
- `post-qualification-audit.txt` validates all 22 completed zero-exit rows.
- `correction-identity-and-scope.txt`, source/seam trace and raw receipt inventory; `development/flow.txt` is development evidence, not substituted for final qualification.

Rust/Cargo **1.98.0**, `aarch64-apple-darwin`; one existing compatible target/cache, one build job, incremental disabled, dev/test debug zero. Minimum recorded qualification free space **5,406,920 KiB (~5.16 GiB)**, above the monitored **4.25 GiB** floor. No unrelated cleanup, target tree, download, executor or runtime was introduced.

**Nine inherited ignored cost/diagnostic tests remain explicitly unrun**: seven core and two consumer, names/reasons in the audit. No native/GUI/IME, backend quiescence, durability/performance or deployment certification is implied. Saves retain their earlier destination, durability and completed-external-effect limits. Budgeted retirement is not a total-latency guarantee; retained ancestry/final-token destruction caveats remain. Exhaustion uses deterministic local counter tests, not physical u64 exhaustion.

**Handback:** in-scope safety fixes and fresh qualification complete; public Session integration remains a concrete blocker requiring sender design/scope assessment. Hold any full F5/native integration claim. Preserve independent re-review and sealed lifecycle authority; no approval or integration was attempted.
