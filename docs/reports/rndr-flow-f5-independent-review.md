# F5 independent review — FAIL

## Reviewed identity, scope and disposition

- Candidate: `286818f9bcac048b725a650594ea88fa9b1d1624`.
- Qualified production/source parent: `2f01d288b14c6e14caee538f9646ffe6aa018921`.
- Accepted F4 baseline: `bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5`.
- Independent review only. **Do not approve or integrate this candidate.** No production fix, F6 implementation, application migration, runtime/backend/SDK/security/policy change, push or deployment was performed.
- Original implementation receipts were not overwritten. All fresh receipts and saved repro source are under `docs/reports/rndr-flow-f5-independent-evidence/`.
- Any review commit is an evidence-only child of the exact candidate, not a replacement implementation candidate. The sender retains final assessment, corrections and sealed native lifecycle authority.

**Verdict: FAIL.** All 22 standard gates independently passed, but three additional deterministic desired-safety assertions fail. The most serious failure makes the unchanged runner close a native document manager while a shared presentation in another World remains open. Green scalar/cycle evidence does not establish F5 contract correctness.

## Findings

### R1 — P1/high: a phase-Ready cohort is not a jointly consumable teardown cohort

**Source:** `crates/rndr_flow/src/protection.rs:536–577`, particularly the `prepared` set at 538 and dependency exclusion at 554; unchanged `crates/rndr_render/src/close.rs:20–125`; `crates/rndr_render/src/lib.rs:2700–2732` (slot destruction).

**Saved repro:** `adversarial.rs`, `manager_cohort_must_be_transitively_consumable`; canonical log `adversarial-qualified.txt`.

Construct three live Worlds: unrelated primary C, secondary manager A, secondary manager B. B has a live presentation lease of A's native document. B also owns its own native document, retained by an application-scope lease outside every World. Request both secondary closes; protect/Discard both through the public adapter; run `sync_native` immediately before the unchanged `CloseDrain::step`. No intervening mutation, missing participant or direct guard manipulation is needed.

Observed:

```
cohort guards A=Ready(CloseRequest(1)) B=Pending(CloseRequest(2))
cohort runner closed A=true B=false unrelated_primary=false
```

Both internal phases are Ready before dependency filtering. A counts B as prepared, although B cannot actually close because its own manager has an outside-cohort application lease. B is invalidated to native Pending, but remains in the earlier `prepared` set used to bless A. Independent secondary-window close policy consumes A without consuming B. Dropping A's runner slot destroys the actual `SourceDocuments` owner while B's presentation is still live. An application lease is explicitly representable; it is not reasonable to treat its existence as permission to destroy the manager. Reordering Worlds does not turn the non-transitive phase set into an atomic cohort.

**Contract basis:** original F0 §6 protection across shared/hidden documents and terminal boundaries; epic §3.4/F5 all-target validation; F5 host contract's native-manager ownership, jointly prepared teardown cohort and freeze-until-consumed promises. No host scheduling discipline repairs this probe: the adapter is called with all Worlds at the documented destructive boundary. A stable actually-consumable dependency closure (or safe refusal) must be established, not merely independently phase-Ready operations. Preserve legitimate jointly closing cycles; do not replace this with a blanket “all shared managers may never close.” No fix was made here.

### R2 — P1/high: a lost replacement owner can commit and retire the valid original

**Source:** `crates/rndr_flow/src/history.rs:547–581` validates incoming entry ownership during initial resolution only; `history.rs:390–480` performs final protected-guard validation; `history.rs:665–736` installs replacement and retires the original; `crates/rndr_flow/src/protection.rs:920–946` validates protected outgoing documents/leases, not a different incoming entry.

**Saved repro:** `replacement_owner_loss_must_refuse_before_commit` in `adversarial.rs`; canonical log `adversarial-qualified.txt`.

Replace retained entry A with a newly owned entry for **a different document**. Initial resolution reaches AwaitingDecision while A remains installed. Explicitly dispose the proposed replacement's scope (unavoidable participant loss), then choose Discard for A and tick. The new entry is not part of A's protected-document lease snapshot. Final validation succeeds; the operation reports Committed and installs the dead replacement while disposing A:

```
replacement-loss phase=Some(Committed) ... new_state=Disposed old_state=Disposed
```

The desired assertions require no commit and preservation of A. The same-document positive control correctly rejects the operation, because that new lease happens to be included in the outgoing document's snapshot. That difference is why existing same-document retained tests miss this failure.

**Contract basis:** exact retained identity/incarnation, all-target validation immediately before commit, and no destructive partial application on unavoidable participant loss (F0 §6; epic §3.4/F5). Losing an incoming owner externally cannot be undone, but it must not authorize retiring the still-valid original. Host obligations to report unavoidable topology loss do not replace final coordinator validation. No fix was made here.

### R3 — P2/medium: refused native Save admission drops the existing attempt and freeze

**Source:** `crates/rndr_flow/src/protection.rs:284–289`: remove owned attempt first, then fallible `get_world_mut(worlds, world)?`; later reinsertion is bypassed.

**Saved repro:** `omitted_world_on_native_save_choice_must_retain_freeze` in `adversarial.rs`; canonical log `adversarial-qualified.txt`.

Prepare a valid native close with all Worlds. Keep its exact current DecisionId. Submit Save with the target World omitted from the supplied slice. The method returns the expected `Err(Pending)`, but unwinding the ordinary `?` return drops the removed attempt and all its RAII protection:

```
omitted-world result=Err(Pending) freeze=false decision=None native=Pending(CloseRequest(3))
```

No explicit Cancel, Discard or terminal World loss occurred. The source is writable and the decision is lost, although native close is still pending. Contrast: the same omission in `sync_native` correctly preserves the existing freeze/decision (independent positive control).

**Contract basis:** atomic refusal and technical failure retaining owned protection, rather than silently becoming cancellation (F0 §6; F5 owned operation/failure contract). Supplying all Worlds is a normal host obligation, but an explicit, typed refusal must not destructively consume previously admitted state. This probe does **not** claim immediate native exit or byte loss: Pending still blocks the runner. The reproduced defect is loss of freeze/operation state on refused admission. No fix was made here.

## Independent qualification

Run at the exact candidate before adding any review commit:

```
sh docs/reports/rndr-flow-f5-gates.sh \
  docs/reports/rndr-flow-f5-independent-evidence/qualification
```

The existing harness and oracle were inspected (including output-directory argument, shared target, low-debug/incremental settings, disk floor and source-identity checks). This is fresh execution, not a copy/relabel of author evidence. `qualification/summary.tsv` has 22 completed zero-exit rows; `post-qualification-audit.txt` reruns the auditor after the audit row was appended.

| Evidence | Fresh result |
| --- | --- |
| Flow | 108 passed, zero ignored |
| Native headless host adapter | 14 passed, zero ignored |
| Core | 834 passed, **7 ignored** |
| Liquid parser | 16 passed |
| Consumer | 115 passed, **2 ignored** |
| Compatibility feature off/on | 2 + 2 passed |
| Formatting/whitespace, focused lint, feature/facade checks, dependency checks | All appropriate gates zero exit |
| Exact-source auditor | 583 fingerprints match bytes and exact candidate HEAD blobs |
| F1/F2 baseline | Fresh 300 teardown cycles; all 19 vector counters equal per-World baseline; only global listener 1; all other counters/external pending 0 |
| F3 budgets | All nine preserved budget proofs |
| F4 lifecycle/actual exit | 360 cycles / 720 actual acknowledgments; full Counts/WorkCounts equality |
| F4 panic/terminal probes | 20 producer + 12 timer + 11 tracked / 45 task branch coordinates; unchanged 3+3 reviewer probes and six-case paused-destructor probe twice |
| F5 steady retained cycles | 360 full Counts/WorkCounts/HistoryCounts rows after bounded warm-up, plus final root/World teardown to zero |
| F5 terminal-window cycles | 360 admission/reclamation cycles, exact resource baselines and shared document retention |

The protected earlier contracts, evidence, manifests and render/runtime/SDK sources are unchanged. Parent/candidate relation and fingerprint checks are saved. These broad successes preserve important prior proofs but do not cover the three combinations above.

Toolchain: rustc/cargo 1.98.0, aarch64-apple-darwin. One compatible project `target/` was reused; `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`, dev/test debug 0. Recorded qualification minimum: **5,196,784 KiB free**, above the harness's 4.25 GiB floor. No unrelated source, caches or targets were deleted; no redundant target tree was created.

### Independent adversarial execution

```
sh docs/reports/rndr-flow-f5-independent-evidence/run-probes.sh
```

Expected current result: **exit 101**, 13 total tests: **10 passed / 3 failed**. These comprise six independent review tests (three desired-safety failures, two positive controls, one destructor/reentrant-submission proof) plus seven unchanged runner tests imported from the actual `close.rs`. Counts are not additive to the standard gate totals.

The runner fences production sources against the candidate, checks the disk floor, copies saved repro source temporarily to `crates/rndr_flow/tests/f5_independent_review.rs`, runs serially with the project's existing target, then removes only that temporary file via trap. No permanent production/test-source change is needed to reproduce. `adversarial-qualified.txt` records exact HEAD, repro SHA-256 and final source coordinates. Earlier development logs are retained for chronology, **not** substituted for the canonical final run: the first version used same-document replacement, which correctly refused and was not evidence for R2. The final distinct-document repro and same-document positive control remove that ambiguity.

## Contract/source review coverage and remaining limits

Reviewed original F0 §6 and epic §3.4/F5, governing accepted F1–F4 contracts and compatibility/resource constraints, full F5 production changes, new F5/core tests, implementation report and host contract. Followed retained typed push/replace/back/forward/select/close, branch truncation, bounded inbox/pending/outcome ownership, operation/decision exhaustion and atomic refusal, exact close coalescing, conflict queues, callback/destructor reentry, all-target cancellation/validation, save observations vs actual owned work/results, round/revision/lease rejection and failure paths. Followed actual source and structured commit/history/reload/IME admission and core compatibility, presentation/world freezes, shared/hidden documents, unleased manager documents, out-of-manager pending IME, registry admission and unavoidable loss, manager ownership and native Ready/reconciliation.

Important positive/negative distinctions:

- Tickets are observations; dropping actual SaveWork/SavedWork fails closed. Retry creates new rounds. Failure/panic do not mean discard. Existing saves can progress while retained/native close waits; freezes remain held after success and through Failed/Ready in the ordinary covered paths.
- Native Ready alone does **not** call Coordinator disposal. Actual runner WorldOwner loss drives disposal/reconciliation. R1 is different: incorrect Ready enables that real destructive boundary too early for an outside-cohort presentation.
- Existing gates preserve F1–F4 panic precedence, terminal receipt retention, actual producer exit and bounded resource accounting. No independent evidence here invalidates those unchanged proofs. Budgeted cleanup is not a total wall-clock latency guarantee; retained-token ancestry/final destruction caveats still apply.
- Reasonable host duties include complete native/document registration, correct manager ownership, all participating Worlds, exact byte/revision persistence and real durability before reporting success, independent save progress, using authoritative writer paths rather than replacing protected resources, guarding attachment/model reassignment, and reconciling actual terminal loss. None grants permission to treat a merely phase-Ready but unconsumable World as already closed, install a dead incoming entry, or drop an admitted freeze on a typed refusal.

### Native integration seam is not certified

The report/host contract requires all-World `sync_native` validation **immediately before** unchanged CloseDrain destructive consumption and after callback/topology batches. Headless tests manually provide `&mut [HostWorld]` and call the private runner close implementation directly. In the actual renderer, `Session` (`lib.rs:342–369`) owns private apps, `Handler` is private (`:511`), `reconcile_close` is private (`:2700`), and slots are removed at `:2732`; there are reconciliation call sites at `:2929` and `:2949`. No public all-World pre-consumption host callback was identified. A single App update is not evidence that arbitrary other session Worlds and both destructive paths have been validated.

Thus the required scheduling/access seam is **not demonstrated by the delivered integration evidence**. Do not invent authority to mutate renderer/runtime here or claim a standard Session host can install a private hook merely because a unit test can include `close.rs`. A corrected F5 needs an explicit, usable integration argument or appropriately authorized seam work and its evidence. This is a static integration limitation, not a fourth runtime reproduction or a native/GUI certification claim. R1 already fails even granting the ideal documented seam.

**Nine inherited ignored cost/diagnostic tests remain unrun** (7 core, 2 consumer; exact names/reasons in the post-qualification audit). Headless adapter testing is **not native/GUI certification**, backend quiescence, deployment or application-migration evidence. Exhaustion is covered by deterministic local counter tests, not physically consuming all u64 IDs. Arbitrary malicious direct resource replacement or a dishonest persistence callback is outside adapter authority. No promise of rollback for already completed external saves is implied.

## Handback

Hold acceptance/integration. Sender should assess R1–R3 and the native integration limitation, commission narrowly scoped corrections, preserve all accepted F1–F4 contracts and receipts, and require fresh exact-source qualification plus unchanged saved safety repros passing. Review artifacts are evidence only; no approval or merge is authorized by this report.

Evidence hygiene: the raw consumer log contains two trailing-whitespace lines from concurrent test output; numbered source-trace blank lines also retain whitespace. They are deliberately preserved verbatim. The exact-candidate whitespace gate passed; review prose/repro/scripts pass whitespace checking separately. A whole evidence-diff whitespace check therefore reports those raw-text lines, not production defects; no receipt was silently sanitized to manufacture green output.
