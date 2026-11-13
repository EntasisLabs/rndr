# F5 authorized Session close-boundary integration

## Outcome and provenance

**Implementation and headless exact-source qualification PASS.** Both previously
failing integration assertions now pass unchanged. No known in-scope blocker
remains in this implementation handback; sender independent re-review and sealed
native acceptance/integration are still required. This is not project approval,
application migration or native GUI certification.

- Starting clean reviewed candidate: `9721657f352bac8e22796a6e608ba2d293bbe498`.
- Qualified source commit: **`da364e3acbf17192ab9a1041609453d0c8fc77f1`**, direct child
  of that candidate. Only production renderer `lib.rs` and `close.rs` changed.
- The final handback HEAD adds only this report, contract clarification and raw
  evidence/manifests. All 589 source fingerprints are identical to the qualified
  commit and checked against both working bytes and exact HEAD blobs.
- `main` remains accepted F4 **`bf56c67fd5b67ddf993ec5c02e5af4ef7ef507d5`**.
  No approval, merge, integration, push, publication or deployment occurred.

## Implemented seam and failure behavior

`Session::close_validator` is an opt-in owned higher-ranked callback receiving
all actual admitted live Worlds. `CloseWorld` exposes stable slot Desktop ID,
primary flag, **runner-owned actual native closing membership**, and borrowed
World access. Hidden, unrelated, unguarded and dynamically admitted Worlds are
not omitted. No flow/backend/policy dependency or new runtime/executor is added
to the renderer; existing facade reexports continue to expose Session/API.

Both real event paths (`Running::window_event`, `Running::about_to_wait`) use the
same production `CloseDrain::reconcile` helper. Its exact ordering is:

1. Finish the ordinary bounded update/build batch; admit one bounded open batch.
2. Prepare intent/guards only, clearing all old secondary destruction decisions.
3. Run needed hidden close frames (visible slots request redraws).
4. Prepare again; a newly needed callback/cohort defers to another bounded pass.
5. Check authoritative participation and invoke the hook with **all live Worlds**.
6. On hook Err/unwinding panic, refuse the entire destructive pass, preserving
   exact native requests, decisions, freezes and in-memory documents.
7. With no intervening app/hidden callback or mutable World/window topology
   change, recheck final guard readiness/participation and consume decisions.
8. Immediately remove eligible secondaries, or attempt the unchanged atomic
   process-exit queue fence. A queued/racing open refuses process commitment,
   wakes the owner and forces another all-World pass after admission.

Manually requested guards on inactive slots are rejected **before** invoking the
hook and again before consumption. A late cancel/new quit cannot let other
secondary flags escape from the old cohort. Secondary `closed` flags are always
recomputed, so later guard invalidation cannot leave a sticky destruction flag.
The private test-only `step` shortcut shares prepare/consume, but production
uses the callback-ordering helper, which is tested directly.

The hook is trusted application code, not a sandbox or persistence adapter. It
must map every required live World, preserve guards/owners, and run final native
protection validation **after** its own UI/save/choice mutations. Unknown or
missing mappings fail closed. Existing dependency-closed fixed-point/native
manager lifetime rules remain: a nonclosing World or application lease holding a
closing manager's document blocks destruction, without implicit model transfer.
No-hook ordinary Apps retain primary/secondary/quit and guarded failure/retry/
discard/cancel behavior. The two safety corrections also apply without a hook;
no-hook Apps do not automatically acquire F5 document protection.

### Panic normalization limits

Ordinary hook errors/panics retain all windows. A custom first panic payload is
dropped under a second unwind fence, so a sequential payload destructor panic
cannot escape the close boundary. Ordinary string secondary payloads are freed;
an **opaque secondary panic payload is intentionally forgotten** instead of
recursively invoking arbitrary destructor code. This pathological path can leak
that payload per occurrence; it is not a universal no-leak/hostile-code claim.
`panic=abort`, panic-during-unwind/double panic, arbitrary application/hidden-
system panics, malicious guard/World replacement and process/OS failure are not
made recoverable. No rollback of arbitrary side effects inside trusted callback
code is promised. The contract and API document these trust boundaries.

## Fresh exact-source qualification

**27/27 gates returned exit 0**, Rust/Cargo 1.98.0, `aarch64-apple-darwin`.
Reproduce into a new output directory (the runner refuses overwrite):

```sh
sh docs/reports/rndr-flow-f5-seam-gates.sh /path/to/new-evidence-directory
python3 docs/reports/rndr-flow-f5-seam-audit.py /path/to/new-evidence-directory
```

The renderer additions are headless and exercise the **actual production-used
ordering/helper and real public Session builder**, not just a manual flow
adapter. The public API is also compiled/tested from an external test consumer.
Both real event routes are independently source-traced by the auditor to the
qualified shared helper. They are **not executed as native winit callbacks**.

| Suite | Passed | Ignored / unrun |
|---|---:|---:|
| Flow, all selected feature-off tests | 144 | 0 |
| Explicit existing native-adapter subset rerun | 14 | 0 |
| Renderer library, no default features | 60 | 0 |
| External public Session/API consumer | 1 | 0 |
| Core, no default features | 834 | 7 |
| Liquid parser | 16 | 0 |
| UI consumer | 115 | 2 |
| Editing compatibility, feature-off / liquid | 2 / 2 | 0 |

Additional required gates: unchanged accepted F4 adversarial probes (3+3 plus
six-case paused-destructor probe twice); unchanged original F5 repro **13/13**;
unchanged independent additions **4/4**; unchanged original seam suite **9/9**,
including both formerly failing safety assertions. New source-fenced probe
runner copies the original bytes, checks equality and preserves original source
fences/failed receipts rather than editing them to bless a new candidate.

The 27 gates also include formatting, whitespace, strict renderer/flow/core/UI
lint, renderer **default-feature library check**, feature-off/facade checks,
dependency checks, exact-source cleanliness/fingerprints and the auditor. The
renderer lint compiles all targets but does not execute the unselected GPU/pixel
integration tests.

### Fresh regression and lifecycle proofs

- **90-coordinate native ProtectionHost matrix through the renderer helper:**
  all six World orders × five dependency/withdrawal cases × secondary-close,
  primary-close and secondary-quit paths. Cases include free/blocked chains,
  free/blocked cycles and hidden-callback participant withdrawal.
- Nine new renderer library tests cover complete cohort metadata, hidden
  mutation/invalidation before final validation, no callbacks after consumption,
  pre/post fabricated participation, hook Err/panic retry without text/guard
  loss, late cancellation/quit, no-hook compatibility, nonsticky secondaries,
  dynamically admitted Worlds/atomic queue fencing and sequential panic-payload
  destructor containment. Flow error/panic tests additionally retain a **real
  native readonly freeze and exact owned decision**.
- **300 F1/F2 teardowns**, all 19 counters equal each seed's baseline; external
  pending acknowledgments zero. Nine F3 budget proofs.
- **360 F4 lifecycle cycles / 720 actual exit acknowledgments**, full accounting;
  preserved 20 producer / 12 timer and 11 tracked / 45 task panic coordinates.
- **360 F5 full-resource baseline cycles**, full Counts/WorkCounts/HistoryCounts
  equality after bounded warm-up, final root/World teardown assertions; **360
  terminal-window admission/reclamation cycles**.
- **589/589 source fingerprints** match exact source and HEAD blobs. F0–F4
  protected contracts/evidence, manifests, runtime/SDK sources are unchanged.
  Against `9721657`, crate changes exactly match the five-file seam/test allowlist.

### Explicitly unrun inherited tests

The following **nine inherited ignored tests remain unrun**, not silently passed:

1. Core `typing::storage_tests::snapshot_storage_accounting`.
2. Core `editor_scale_costs`.
3. Core `scroll_and_fold_stability`.
4. Core `unique_line_editor_costs`.
5. Core `markdown_virtual_cost`.
6. Core `markdown_idle_shape_cost`.
7. Core `model_costs`.
8. UI consumer `liquid_stream_cost`.
9. UI consumer `liquid_density_cost`.

Their exact ignore reasons are retained in `qualification/audit.txt` and raw
suite logs. Separately, renderer GPU/pixel/integration suites beyond the selected
public API test were not run, including its ignored shaped-editor release-cost
probe. No release/performance, OS/IME/dialog/native surface teardown, filesystem
durability or backend-quiescence certification is implied.

## Disk/build resource evidence

One existing shared/local `target` tree was reused. Every build was serial
(`CARGO_BUILD_JOBS=1`), incremental-disabled, low-debug, offline and locked.
No alternate target tree, broad cleanup or deletion of unrelated work occurred.

- Source qualification start: **5,420,692 KiB free**, target **2,584,736 KiB**.
- Lowest recorded gate-boundary free space: **4,887,596 KiB (~4.66 GiB)**.
- Every gate was admission-checked against **4,456,448 KiB (4.25 GiB)** and a
  one-second monitor interrupted the active gate if it observed less. No disk
  blocker receipt was emitted. Continuous samples are not persisted; the
  reported 4.66 GiB is the recorded before/after minimum, not an instantaneous
  high-frequency minimum or a resource/performance guarantee.
- Final resource receipt records about **4.66 GiB free** and target
  **2,923,844 KiB (~2.79 GiB)**. Raw `df`/`du` and exact before/after observations
  are in `resource-final.txt`, `qualification/environment.txt`, `summary.tsv`.

## Report, manifests, full changed-file list

- Detailed contract: `docs/design/rndr-flow-f5-session-close-validation.md`.
- Updated existing host contract: `docs/design/rndr-flow-f5-host-contract.md`.
- Historical blocker assessment retains its pre-authorization text under an
  explicit supersession banner; original reviewer report/probes/receipts remain
  byte-unchanged.
- Gate runner/auditor: `docs/reports/rndr-flow-f5-seam-gates.sh` and
  `docs/reports/rndr-flow-f5-seam-audit.py`.
- Evidence root: `docs/reports/rndr-flow-f5-seam-evidence/`.
- Exact commands, exits and disk observations: `qualification/summary.tsv` and
  every full raw `<gate>.txt`; `complete-audit.txt` reruns the final 27-row audit.
- Source/harness fingerprints: `qualification/source-digests.txt`,
  `source-fingerprints.txt`, `harness-digests.txt`.
- Full evidence manifest: `evidence-digests.txt` (all evidence files except itself).
- **Full changed-file list against `9721657`: `changed-files.txt`**, checked to
  equal `git diff --name-only 9721657 HEAD`; no files are omitted. Its full list
  is also appended below. Five crate paths are the complete code/test delta;
  everything else is contract/report/harness/raw evidence.

## Remaining blockers and handoff

No further in-scope integration blocker is reproduced by the required unchanged
or new adversarial tests. Native GUI/platform behavior and actual application
composition remain unqualified; F6/application migration is expressly excluded.
The host must retain the actual existing application-owned shared manager and
protection lifetimes. The exceptional opaque-secondary-panic-payload leak and
trusted-callback limits above remain explicit. Sender must independently review
this exact clean candidate, request corrections if needed, and alone owns
sealed native acceptance/integration. Worker completion does not approve the
project. No backend authority, executor/framework, F6 or unrelated source change
is included, and nothing was pushed/published/deployed.

## Complete changed-file list against `9721657`

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
