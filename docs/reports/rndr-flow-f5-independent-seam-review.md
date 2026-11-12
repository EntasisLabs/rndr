# Independent F5 authorized renderer-seam review

## Exact scope and verdict

Reviewed **`8c337328ea733cda18a0f7215bfe379b986ea646`**, qualified source parent
`da364e3acbf17192ab9a1041609453d0c8fc77f1`, descended from reviewed
`9721657f352bac8e22796a6e608ba2d293bbe498`. Initial checkout was clean.
`main` remains **`bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5`**. The reviewer
is separate from the implementation author. This handback adds only this report
and the distinct `rndr-flow-f5-independent-seam-review-evidence/` tree.

**QUALIFIED PASS for the authorized narrow, trusted-hook contract.** No new
in-contract destructive-safety or compatibility defect was reproduced. The
missing-public-seam blocker is resolved at the API/shared-helper/source-route
level, **not certified by executing native event-loop callbacks**. This is not
project acceptance or approval.

| Domain | Verdict | Meaning |
|---|---|---|
| Destructive safety | PASS within documented preconditions | Real slots supply all admitted live Worlds and authoritative participation; final validation follows callbacks; errors refuse consumption; original safety assertions pass. Trusted hook must map required Worlds and preserve owners/guards. |
| Compatibility | PASS for selected headless gates | No-hook policy, F1–F4 and retained R1–R3 regression gates pass. No automatic F5 protection for an unhooked app. |
| Normal lifecycle resources | PASS for measured proofs | Full-vector teardowns and repeated F4/F5 resource proofs pass. No constant-time, RSS, or universal allocator bound follows. |
| Exceptional panic resources | **NOT an unconditional bounded-resource PASS** | Independent test confirms permanent retention per opaque secondary payload. Repeated occurrences accumulate even with one window. This is a disclosed exception, not a newly hidden implementation defect. |
| Evidence | PASS for stated selected scope | Fresh 27 gates; 589 exact source fingerprints; unchanged 13/4/9 probes; seven new independent helper tests. Native GUI/platform gaps and one optional unverified extra probe remain explicit. |

A universal “all panic recovery is leak-free/bounded” claim would be **FAIL** on
the saved concrete leak reproduction. The candidate explicitly disclaims that
claim. Sender must decide whether that exclusion is acceptable; this review does
not broaden recovery/trust guarantees to make it acceptable silently.

## Inspection and trust boundary

Read the complete implementation report `rndr-flow-f5-session-seam.md`, detailed
`docs/design/rndr-flow-f5-session-close-validation.md`, original F0 contract and
relevant original epic/F5 requirements, existing host contract and historical
blocker assessment. Inspected the complete five-file code/test delta against
`9721657`: renderer `lib.rs`, `close.rs`, new renderer boundary tests, external API
test and native flow matrix. Followed both real event routes, hidden frame code,
native close inputs, public CloseGuard and WindowHub admission/commit semantics.
Oversized observations were followed with bounded source reads rather than
inferring omitted text. Fingerprint completeness does **not** mean all 589 files
were individually reviewed line-by-line.

The hook is trusted application code, not a sandbox. A hook returning `Ok` must
perform its final `sync_native` after its own mutations, map every required live
World (including unrelated primary/hidden presentations), preserve exact native
owners/guards and hold protection through actual terminal ownership loss. The
renderer does not prove arbitrary application mappings correct or repair a lying
hook. It does not roll back mutations made inside a failed hook. Removing a
CloseGuard/WorldOwner or replacing protected Worlds is outside the supported
in-flight protocol, not an opt-out. None of this is an app migration.

## Native ordering and destructive-boundary assessment

Both **`Running::window_event`** and **`Running::about_to_wait`** call
`Running::reconcile_close` (`lib.rs` around 2743, 2952–2993). The former dispatches
the actual native event first; the latter completes its bounded shell/hidden
batch first. `reconcile_close` admits one open batch, then calls the same
production-used `CloseDrain::reconcile` qualified by headless tests. It does not
use the private callback-free test-only `step` shortcut.

The shared helper performs prepare -> required close frames -> prepare again ->
deferral if new callbacks are necessary -> participation preflight -> all-live-
World validator -> final participation/readiness check -> consumption. Old
secondary `closed` decisions are cleared during prepare. Production `parts()`
only borrows Handler fields; it does not dispatch user code. `CloseWorld`'s ID,
primary and closing fields are private runner-produced metadata, not inferred
from a public guard request. Manually Pending/Ready guards on inactive slots
refuse the pass before hook invocation and again before consumption. They do not
turn an unrelated window into a dependency cohort member.

The slice includes **every admitted Handler World**, including unrelated,
unguarded, hidden and primary slots, not just active guarded closes. Queued Apps
not yet built are not fictional live Worlds: process exit must pass the atomic
hub fence. Newly built windows join the next full validation. An unguarded
primary with guarded siblings waits for those guards under process exit.

A hook Err or caught unwinding panic returns Continue without consuming a
secondary or process exit. If the hook changed cancellation/new quit intent,
final consumption refuses the whole pass; the next prepare settles fresh
membership/requests. Secondary readiness can legitimately remove independent
Ready secondaries while another is Pending: this is not all-or-nothing process
exit. ProtectionHost's dependency-closed greatest fixed point must prevent a
manager being one of those consumable secondaries while a holder survives.
The 90-coordinate real-host matrix and unchanged original probes exercise chains,
cycles, outside holders, withdrawal and all six orders. Public request identity
is not authoritative membership.

On secondary Continue, native code immediately executes `slots.retain` with a
trivial predicate. On process Exit, it immediately attempts `try_commit_exit`;
a queued/racing open refuses commitment and wakes another admission/validation
pass. Admission after commitment rejects opens. `about_to_wait`'s subsequent
`sync_flow` only observes scheduling state/requests redraws; no app update is
performed. Subsequent real application event entry points check `exiting()`.
The earlier `new_events` redraw requests do not execute an application callback.

**Destructor limit:** no separately scheduled app/build/hidden callback appears
between successful validation and consumption/removal. But resource/Handler
**destructors are arbitrary code during destruction itself**. Independent
characterization confirms secondary App resource destructors run during retain
and can queue opens; there is no rollback or revalidation between each destructor
and the next secondary destruction. All eligible flags have already been
committed. This is consistent with the explicit contract that destruction itself
is not a separate callback phase; it is not a guarantee that destruction is
callback-free or that malicious/panicking resource destructors are transactional.
No unsupported mutation of another World/manager during that commit is certified.

No deadlock/liveness defect was reproduced. Independent retries demonstrate that
technical errors do not repeat close frames endlessly; resolving guards permits
progress. Fail-closed persistent hook errors intentionally keep windows alive.
Continuous new intent/recursive opens can postpone exit; no starvation-free or
hard latency promise is established.

## Independent adversarial evidence

Canonical `adversarial.rs` imports **the actual renderer `close.rs`**. The
runner temporarily copies it into the existing flow test directory, verifies
bytes and source cleanliness, executes serial/offline/locked in the shared
target, then removes it. The candidate implementation and permanent tests are
unchanged. This is not just rerunning author assertions or a manual adapter.

**14 passed / 0 failed / 0 ignored**: seven new review-owned tests plus seven
unchanged in-module runner tests. Full `adversarial.txt` preserves caught panic
output and exit 0; `adversarial-first7.txt` is the original identical receipt.

1. **128 coordinates:** three simultaneous secondaries × all Ready/Failed-or-
   Pending subsets × hook success/error. Every pass includes an unrelated
   unguarded primary; hook error closes none, ordinary independent readiness
   closes only eligible secondaries.
2. A **hook-local destructor** queues an open after the callback's final check.
   The atomic process fence refuses exit, the real build closure executes once,
   the new guarded World joins the next all-World pass, and postcommit opens
   cannot execute their build closure.
3. Twelve alternating hook Err/panic retries on secondary and process cohorts
   preserve exact native request IDs without rerunning close frames; resolution
   then permits the corresponding secondary removal or process exit.
4. Whole-process cancellation followed by a new secondary quit allocates fresh
   requests; completing old requests cannot satisfy the new cohort.
5. A late primary quit inside validation prevents consumption of otherwise Ready
   secondaries, then revalidates the complete process cohort.
6. Repeated opaque-secondary-panic-payload resource leak characterization below.
7. Secondary destruction characterization: no resource destruction before
   validation/commit; both resource destructors run during removal, can queue
   opens, and do not deadlock the hub. No rollback claim is inferred.

Two reviewer-only development compilations failed (non-`'static` callback captures,
then wrong optional-extra enum import); their source/logs are saved, not safety
failures in candidate code. The first was corrected and the canonical seven-test
source passed. An **optional eighth** real-native-discard/error/panic probe was
written later. Its import was corrected but a below-floor preflight refused the
next run before Cargo started. `unverified-real-native-extra.rs` is **unverified**,
excluded from all pass counts. Actual native freeze/decision retention remains
covered by the freshly rerun permanent flow tests, not claimed as this extra
independent result. A reviewer-check script's mistaken expectation of a persisted
space-monitor file is also preserved/corrected; the baseline monitor emits only
a breach receipt, not continuous samples.

### Concrete exceptional resource finding

`CloseDrain::finish` catches the first validator panic, drops that payload under
a second fence and frees ordinary string secondary payloads. For an **opaque
secondary payload it deliberately calls `forget`**, avoiding arbitrary recursive
payload destruction. The independent test makes a first payload destructor
panic with a second custom payload holding an Arc. Four repeated invocations in
one still-live closing window leave **four extra Arc owners, zero secondary
payload destructor calls**, while all close refusals preserve the window.
This is a real leak, not merely delayed release until guard retry. The payload
can own an arbitrarily large graph; neither bytes per fault nor cumulative faults
are bounded by the window count or flow Limits. Process reclamation at exit is
not successful in-process cleanup.

The current report/API contract discloses the exception. Recommend keeping it
prominent in acceptance/release statements; do not summarize this as all-resource
panic recovery or unconditional bounded F5 teardown. If universal bounded fault
resources are required, sender must request a separately assessed correction or
narrow the supported panic-payload policy; this review does not implement one.
`panic=abort`, panic-during-unwind, arbitrary app/system/destructor panics, hostile
hook code and forced process/OS loss remain outside recoverable guarantees.

## Fresh exact-source qualification

Ran the unchanged `rndr-flow-f5-seam-gates.sh` into the distinct review evidence
path **before adding temporary independent probes**. All **27 unique gates exit
0**. Commands/full logs and before/after disk values are retained per gate.

| Suite | Passed | Ignored |
|---|---:|---:|
| Flow feature-off | 144 | 0 |
| Explicit native adapter subset (additional rerun) | 14 | 0 |
| Renderer library feature-off | 60 | 0 |
| External public Session/API consumer | 1 | 0 |
| Core feature-off | 834 | 7 |
| Liquid parser | 16 | 0 |
| UI consumer | 115 | 2 |
| Editing compatibility feature-off / liquid | 2 / 2 | 0 |

The **original unchanged 13/4/9 reviewer probes** pass, including both formerly
failing seam assertions. Original failed receipts and source bytes are unchanged.
Accepted F4 reviewer probes also pass. Fresh lifecycle proofs include 300 full
19-counter F1/F2 teardowns; nine F3 budget tests; 360 F4 cycles / 720 actual exit
acks; 20 producer / 12 timer / 11 tracked / 45 task panic coordinates; 360 F5
full-resource baseline plus 360 terminal-window cycles; 90 real-host seam
coordinates. Selected strict lint, default renderer library check, feature-off/
facade, dependency, formatting and whitespace gates pass.

The implementation auditor is only a baseline. Review-owned
`independent-check.py` separately compares the full **589-file** fingerprint set
with the base Git tree, candidate blobs, current HEAD blobs and working bytes;
checks the exact five-file code/test allowlist, original evidence immutability,
27 expected gate names/exits, raw 300 full-vector coordinates and selected
proof/independent-probe receipts. It passes. The actual tests assert full data;
printed counters alone are not a substitute for compiled assertions. Static
route tracing is not promoted into native runtime evidence.

## Unrun gaps and resource stop

The nine inherited ignored tests remain **unrun**:
`typing::storage_tests::snapshot_storage_accounting`, `editor_scale_costs`,
`scroll_and_fold_stability`, `unique_line_editor_costs`, `markdown_virtual_cost`,
`markdown_idle_shape_cost`, `model_costs`, `liquid_stream_cost`,
`liquid_density_cost`. Exact reasons remain in raw logs/audit.
Renderer GPU/pixel/integration suites beyond the selected API test, its separate
ignored shaped-editor release-cost probe, release-performance/RSS tests, real
winit/native GUI/event-loop/platform/IME/dialog/surface teardown, filesystem
crash durability and backend quiescence are **not certified**. Full application
composition/F6 is not implemented or tested here.

Builds reused the existing shared target, jobs=1, incremental off, low debug,
offline/locked; no alternate broad target or unrelated cleanup. Qualification
recorded minimum **4,845,932 KiB (~4.62 GiB)** at gate boundaries, above
**4,456,448 KiB (4.25 GiB)**. Its one-second monitor recorded no breach;
continuous samples were not persisted, so this is not an instantaneous minimum.
The successful seven-probe run also stayed above the floor in its receipts.
Later disk availability fell below the floor **between completed runs**. The
optional eighth probe's preflight refused at **4,208,564 KiB (~4.01 GiB)**;
no next build started and compilation stopped. `resource-final.txt` records the
later observation. No restart, floor override, alternate target or cleanup was
attempted. Further builds require space safely above the floor; do not infer
extra unrun tests passed. Completed full qualification is unaffected.

## Handback and reproducibility

- Complete candidate changed-file list (49 paths against `9721657`):
  `candidate-changed-files.txt` under the distinct evidence root, appended below.
- Complete review-only changed-file list: `review-changed-files.txt`; no candidate
  production/test/contracts/author evidence files were edited by this reviewer.
- Full logs/commands: `qualification/*.txt`, `summary.tsv`, driver log,
  `adversarial.txt`, development receipts and `disk-preflight-blocker.txt`.
- Full source/harness digests: qualification `source-digests.txt`,
  `source-fingerprints.txt`, `harness-digests.txt`.
- Review evidence integrity: `evidence-sha256.txt` (every evidence file except
  that manifest itself); report hash is in the manifest.
- Actual route excerpts: `source-route-trace.txt`; review-owned checks and
  `independent-check.txt` retained separately from author auditor.

When space permits, reproduce into a new output directory with the unchanged
seam gate script; run saved adversarial runner separately and serially. The
runner deliberately refuses a dirty code tree or floor violation. Do not count
the saved optional extra as executed.

No implementation correction is requested by this narrow-contract review.
Sender retains assessment of the resource exclusion and outstanding native
qualification, any corrections and the exact sealed native acceptance/
integration lifecycle. **No fixes, F6, app migration, backend/runtime authority
expansion, approval, integration, push, publication or deployment occurred.**

## Appendix: complete candidate changed files against `9721657`

```text
crates/rndr_flow/tests/f5_session_boundary.rs
crates/rndr_render/src/close.rs
crates/rndr_render/src/close_boundary_tests.rs
crates/rndr_render/src/lib.rs
crates/rndr_render/tests/close_validator_api.rs
docs/design/rndr-flow-f5-host-contract.md
docs/design/rndr-flow-f5-session-close-validation.md
docs/design/rndr-flow-f5-session-seam-blocker.md
docs/reports/rndr-flow-f5-seam-audit.py
docs/reports/rndr-flow-f5-seam-evidence/changed-files.txt
docs/reports/rndr-flow-f5-seam-evidence/complete-audit.txt
docs/reports/rndr-flow-f5-seam-evidence/development-initial-bridge-check.txt
docs/reports/rndr-flow-f5-seam-evidence/development-initial-render-check.txt
docs/reports/rndr-flow-f5-seam-evidence/evidence-digests.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification-command.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/audit.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/compatibility-liquid.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/compatibility-off.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/consumer.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/core.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/environment.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/facade-liquid.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/facade-off.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/feature-off.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/flow-dependencies.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/flow.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/fmt.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/harness-digests.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/host-adapter.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/lint-consumer.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/lint-core.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/lint-flow.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/lint-renderer.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/liquid.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/preserved-f5-probes.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/renderer-api.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/renderer-default.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/renderer.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/reverse-dependencies.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/reviewer-probes.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/source-clean.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/source-digests.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/source-fingerprints.txt
docs/reports/rndr-flow-f5-seam-evidence/qualification/summary.tsv
docs/reports/rndr-flow-f5-seam-evidence/qualification/whitespace.txt
docs/reports/rndr-flow-f5-seam-evidence/resource-final.txt
docs/reports/rndr-flow-f5-seam-evidence/run-preserved-probes.sh
docs/reports/rndr-flow-f5-seam-gates.sh
docs/reports/rndr-flow-f5-session-seam.md

```
