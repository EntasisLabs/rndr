# rndr-flow F5 implementation and qualification

## Status

Worker candidate for sender assessment and independent review, **not project acceptance or integration**. Based on accepted F4 `bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5`. No approval, merge, push or deployment is performed by this worker. F6 and Medousa migration remain unimplemented.

The production/test/tooling commit precedes an evidence-only child to avoid self-referential test receipts. Exact tested commit, toolchain and final gate outcomes are recorded in `rndr-flow-f5-evidence/qualified-final/environment.txt`, `summary.tsv` and `audit.txt`. This source-candidate report is intentionally written before those gates; the evidence child completes the exact results section without changing qualified source.

The original source candidate `ccbe77929b651080864fe1a346ade65471d29979` passed all 22 gates; its receipts remain in `qualified/`, but are **not final-candidate evidence**. Manual follow-up identified native manager ownership across secondary windows. The correction adds joint-preparation before manager teardown and immediate queued-phase observation. Corrected source is qualified separately in `qualified-final/`; no original receipts are overwritten.

## Delivered

- Typed bounded retained push/replace/back/forward/select/close. Each retained entry owns its existing `ScopeOwner` and `PresentationLease`, route metadata and exact native document target—not another document/history/UI tree. Replace preserves its position; retained back/forward preserve incarnations.
- Bounded callback inbox plus owned operation/decision rounds and terminal outcome ring. Equal outstanding close sets coalesce; conflicting tab/native operations queue while unrelated tabs proceed. Checked authority/serial/round exhaustion and capacity refusal preserve current ownership/state. Observers never choose.
- Owned save/discard/cancel policy; all-target native revision and lease/participant validation before atomic retained commit. Technical failure/panic/lost actual save jobs/results retain protection. Fresh retry decisions reject old/foreign/duplicate decisions and acknowledgments. Actual owned save work uses existing executors/threads, not a new runtime.
- Concrete source/structured authoritative writer freeze, exact source file write+sync helper and immutable native structured persistence seam. Programmatic edits, undo/redo, source reload and editor/IME mutation cannot bypass freeze through readonly setters. Active/pending composition blocks admission without clearing preedit, including hidden/shared non-manager Worlds. Missing participants stay pending/queued rather than being assumed quiescent.
- Exact existing `CloseGuard` request mapping and unchanged native `CloseDrain` policy. All registered manager documents (including unleased native documents), shared/hidden leases and Worlds are covered. Freeze survives Failed and native Ready; one World's Ready disposes nothing. New World/lease admission refuses under protection. A manager World cannot publish Ready while its native document store still has a lease outside the jointly prepared teardown cohort. This preserves the actual shared native authority, not merely metadata; application leases outside Worlds must end explicitly. The new three-World secondary-manager test proves this barrier while an unrelated primary remains live. Unavoidable native document additions invalidate Ready and require explicit cancel/restart. Actual terminal World loss, not Ready, permits retained tab and native protection reconciliation.

See [host contract and F6 handoff](../design/rndr-flow-f5-host-contract.md) for exact ownership, limits, hook ordering, persistence semantics and caller obligations.

## Narrow core/editing production justification

These changes are necessary for F0 §6/F5: a flow-only boolean or readonly metadata cannot freeze actual writer admission.

| Source | F5-only reason / compatibility boundary |
| --- | --- |
| `rndr_core/src/freeze.rs`, `lib.rs` | Owned counted nonclone `DocumentFreeze`, no executor/policy/backend change. No freeze exists unless explicitly acquired. |
| `rndr_core/src/editing.rs` | Source writer/history/reload and editor composition checks honor the owned freeze independently of UI readonly. Additive read-only native identity enumeration detects omitted manager documents. Ordinary unfrozen behavior is preserved. |
| `rndr_core/src/structured.rs` | Structured native writer/history/composition checks honor the same opt-in freeze; source and structured authorities remain distinct. |
| `rndr_core/src/structured_native.rs` | Refuse early removal of a frozen structured authority; additive read-only exact handle enumeration for native close coverage. |
| `rndr_core/tests/close_freeze.rs` | Native source/structured state equality, nested ownership, readonly independence, composition refusal and removal compatibility. |

No render, facade, UI, parser, SDK, manifests/lockfile, backend or executor production source is changed. The adapter's multiworld test includes the **unchanged** `rndr_render/src/close.rs` coordinator directly to exercise actual runner policy headlessly. It is not a mock metadata promise, but also **not native/GUI certification**. Core full tests and both editing compatibility configurations are qualification gates.

F3's original idempotent `close_document` and stale/invalid/exhaustion refusal precedence remain covered by their unchanged tests. F3 retirement budgets and F4 work/request/receipt/panic implementations remain unchanged. New synchronous topology/coverage snapshots are explicit observation/admission cost, not hidden inside retirement budget units.

## Deterministic adversarial coverage

| Concern | Committed proof |
| --- | --- |
| Cancel byte/revision/history/selection/resource equality; observer inactivity | `tests/f5.rs::cancel_preserves_exact_bytes_revision_history_selection_and_resources` |
| Confirmed exactly-once cleanup; coalesced close; stale/duplicate/foreign decisions and saves | `f5.rs::confirmed_disposal_once_stale_duplicate_foreign_decisions_and_saves_refuse`, cancel/coalescing test |
| Technical save failure/panic/loss; retry and edit-after-save-start | `f5.rs` failure and source-writer tests; `f5_saves.rs` drops actual owned jobs/results and runs real worker threads |
| Real source write+sync and quit/save liveness | `f5.rs::actual_source_save_lane_is_live_while_close_waits`, `f5_quit_save.rs` actual file persistence from channel-controlled owned producer while native close waits |
| Retained back/forward IDs, replace position, forward-truncation cancel | `f5.rs::retained_back_forward_replace_and_forward_truncation_guard_exact_incarnations` |
| Atomic multi-entry validation on unavoidable participant loss | `f5.rs::multi_entry_validation_is_atomic_on_unavoidable_participant_loss` |
| Counter/decision exhaustion; total queued+pending capacity; no replacement/partial history change | private `history_tests.rs` production scalar seams, unchanged F3 lease/counter regressions |
| Reentrant callback submission, conflicting queues and unrelated work | `f5.rs::reentrant_submission_conflict_queue_and_unrelated_selection_progress`; native Ready conflict case in `f5_admission.rs` |
| Shared hidden leases, missing Worlds, native Failed/Ready/Cancel/retry and dynamic World admission | `f5_native.rs` plus seven unchanged included CloseDrain policy tests |
| Unavoidable manager-document addition while Ready | `f5_admission.rs::unavoidable_new_document_while_ready_invalidates_and_requires_explicit_restart` |
| Pending IME without clearing preedit, including other hidden lease Worlds | `f5.rs` pending Input case, `f5_shared_ime.rs`, native core structured composition test |
| Native source/structured programmatic/history/reload/removal/readonly boundaries | `rndr_core/tests/close_freeze.rs`, F5 source test, unchanged complete core/editing compatibility suites |
| Bounded resource baselines and terminal windows | `f5_cycles.rs`: 360 stable full `Counts`/`WorkCounts`/`HistoryCounts` rows after bounded outcome-ring warm-up, plus final root/World zero; `f5_windows.rs`: 360 actual World admission/destruction/reconciliation cycles, exact baseline and shared document survival |
| Preserved F1–F4 / panic and ack contracts | Fresh full UI teardown vector proof, flow suite including nine F3 budget proofs, all F4 branch/order matrices and all three byte-unchanged saved reviewer suites (last paused-destructor probe twice) |

No sleep-based race oracle is used; actual threaded save cases use channel/join control. Fixed seeds are explicit test coordinates, not a claim of exhaustive interleavings.

## Exact candidate evidence

Qualification runner: `sh docs/reports/rndr-flow-f5-gates.sh`.
Auditor: `python3 docs/reports/rndr-flow-f5-audit.py docs/reports/rndr-flow-f5-evidence/qualified-final`.

The runner records every exact command/result, toolchain/commit, ignored tests and disk readings. It uses one local target tree, `CARGO_BUILD_JOBS=1`, incremental disabled, debug info disabled and a monitored 4.25 GiB safety floor. No unrelated caches/worktrees/source/history are deleted. Historical F1–F4 receipts and sources remain unchanged; fresh qualification reads only the new qualified directory. Development failures and corrections are preserved separately and are not counted as final evidence.

Exact results will be completed in the evidence-only child after qualification. All source fingerprints are checked against both current bytes and exact Git HEAD blobs; the evidence child must not alter qualified code/tests/manifests. The audit also checks all 19 F1/F2 baseline components, F4 actual acknowledgments, F5 resource rows and protected historical sources, rather than relying on green scalar test counts alone.

## Remaining limits / F6 handoff

- This is an **opt-in host adapter**, not automatic application installation. Hosts must install existing CloseGuard at creation, bind exact Worlds, enumerate all participants/documents, run the adapter at the destructive runner boundary, respect adapter errors and retain protection owners through the attempt. The complete integration sequence is in the host contract.
- No OS event-loop/GUI/IME, clipboard/accessibility, release performance, atomic file replacement/directory durability, destination-security, external SDK termination, blocked-IO/backend quiescence, `panic=abort` or arbitrary double-panic certification. Successful host save acknowledgment asserts actual persistence policy, not backend authority invented by flow.
- Completed external saves are not rolled back on later cancellation; an undelivered/dropped save result fails closed and may require retry. Technical failure alone never discards data or releases freeze. Unavoidable topology loss/additions reject the current protection rather than silently choosing new targets.
- F1 whole-stream staging, F2 split-CRLF and F3 retained-token/depth-proportional destruction/unbudgeted observation caveats remain. Budget progress is not a total-latency or destructor bound. Application route payloads/native document memory remain their owners' responsibility.
- Nine inherited ignored cost/diagnostic tests are expected to remain unrun; exact names/results belong to the final evidence. No ignored performance test is promoted to a claim here.
- F6 requires separate application authorization and its real typed routes, authoritative writers/dirty state, encoders/destinations, window hooks/decision UI and backend completion/exit evidence. Then run the four epic application proofs against an exact reproducible baseline with preserved SDK/session/approval/security semantics. No Medousa source edits or F6 proof claims are made by this undertaking.

Sender retains independent assessment/review, corrections and native sealed approval/integration.
