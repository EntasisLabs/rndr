# rndr-flow F3 implementation and qualification

## Correction status — incremental metadata retirement

The original candidate `fa5a03b76975068e015cc5f373baadaff1b703fe` passed lifecycle
review but **did not satisfy F0 §5 incremental metadata scanning**. Sender
requested changes, not integration. The scoped correction replaces whole-tree
order construction, eager death reconciliation and repeated global lease sweeps
with resumable budgeted retirement. Exact counts remain a separate explicit
synchronous observation. See [the correction report](rndr-flow-f3-correction.md)
for changed APIs/invariants, deterministic work proofs and **new rerun evidence**.
The correction still requires fresh independent review and sender acceptance.

The original implementation report below is retained as historical evidence.
Its 24-gate results and non-incremental-scan caveat describe the old candidate,
not a claim that those broad gates were rerun on this correction. The exact
original text and evidence remain available in `fa5a03b`; original evidence files
have not been overwritten.

## Original candidate report (superseded scan contract)

Date: 2026-11-12 UTC. Status: implemented and locally qualified candidate;
**not sender-accepted or integrated**. Work `work-901f2ce85e86e30b96529a611e423516`,
isolated branch `worktree/rndr-flow-f3-headless-ownership-core-and-f4-readiness`.
Accepted F0/F1/F2 baseline: `9f36c97271a3d2e7ead3049afdec89753ef990c3`.
The candidate is the commit containing this report; resolve its exact OID with
`git log -1 --format=%H -- docs/reports/rndr-flow-f3.md` after commit.
No merge, push, publication, deployment or Medousa migration occurred.

## Delivered scope

Optional package `rndr-flow`, Rust import `rndr_flow`, with no default/optional
features and sole direct runtime dependency feature-off `rndr_core`. Includes a
private pure state transition module, public opaque checked identities, a host
coordinator, immutable single-parent scopes, World/entity signal-only owner
bindings, manager-owned document metadata and separate presentation leases.
Visibility retains resources; explicit recoverable suspension invalidates the
activity incarnation and consumes activity cleanup before resume.

Logical disposal is immediate through ancestor-aware atomic liveness. Physical
cleanup is explicit, bounded per pump, child-first / creation-ordered siblings /
reverse registration per scope. FnOnce callbacks are removed before invocation;
rejections return untouched ownership; error/unwind paths release captures and
continue independent cleanup. Callback commands are bounded FIFO disposal
requests revalidated after invocation. Diagnostics are a payload-free bounded
ring; stale rejection needs no permanent tombstones. Counts distinguish held
leases and logical pending cleanup from retained physical metadata, not external
worker acknowledgement.

The [crate README](../../crates/rndr_flow/README.md) documents host usage and the
supported removal/drain contract. The [F4 readiness source map](../design/rndr-flow-f4-readiness.md)
records concrete existing task/feed/timer/subscription/executor/document APIs,
ordering and epoch limitations, bounded proposed implementation slices, a
possible narrowly justified core status seam, and a deterministic race matrix.
It does **not** implement F4 adapters, replacement lanes or completion gates.
F5 guards/navigation/save-close and application adoption remain separate.

## Change audit

- Workspace manifest adds only the companion member and explicitly disables
  inherited core default features so the companion can request feature-off core.
  Core currently has `default = []`; no existing default behavior is removed.
- Cargo.lock adds only the `rndr-flow` package with dependency `rndr_core`.
- New source is confined to `crates/rndr_flow`; no core/UI/Liquid/runtime source
  changes, facade export, reverse dependency or executor are introduced.
- New documentation, runner and bounded text evidence are under `docs/`.
  Build artifacts remain untracked/ignored, not part of the candidate.
- Final source/contract review covered ownership, stale binding, scope/document
  authority, cleanup ordering/reentry, refusal ownership, counter exhaustion,
  physical storage bounds and affected feature/facade dependency contracts.
  Independent sender review is still required.

## Verification

The previous attempt stopped at disk exhaustion. That was not a pass. On resume,
Git was still at the accepted baseline with the intended uncommitted changes,
no cargo/gate process remained active, and approximately 16 GiB was available.
All checks below were rerun after disk cleanup. There was no lease workaround or
execution outside the admitted worktree.

Exact reproducible commands and individual exit codes are in
[`f3-gates.sh`](f3-gates.sh) and [`f3-evidence/gates.tsv`](f3-evidence/gates.tsv).
**All 24 recorded gates returned 0.** Read individual results rather than relying
only on the runner's final shell status (the runner prints all results).

| Gate family | Outcome / evidence |
| --- | --- |
| Focused feature-off flow tests | Initial recovery run: 14 passed. After adding the Sim test: **15 passed**, 0 failed, `flow-test-final.log`. |
| Lifecycle repetition | 100 cycles, 300 callbacks; after every cycle: scopes=1, cleanups=0, leases=0, active_leases=0, pending_scopes=0, pending_cleanups=0, queued_commands=0. Root disposal then returns every count to zero. `cycles.log`. |
| Existing Sim host | Seeds 0, 1, 42; actual World binding/despawn, manual time/update, immediate logical death, no automatic callback, explicit physical pump and World shutdown. Included in final focused suite. No async adapter claim. |
| Headless build and dependency graph | Feature-off build, normal/build dependency tree and inverse tree all exit 0; no adopter/reverse dependency introduced. |
| Format / focused strict Clippy | Gate-runner checks pass; repeated after final test addition in `fmt-final.log`, `flow-clippy-final.log`, both exit 0. |
| Accepted Liquid behavior | `cargo test --locked -p rndr_liquid -p rndr_liquid_ui -- --nocapture` passes. |
| Contributor core/UI/paint/render and source-session | Both test commands and strict contributor Clippy pass. Existing ignored tests remain ignored; no performance certification inferred. |
| Workspace and feature-off | `cargo check --locked --workspace` and core/UI/paint feature-off check pass. |
| Editing compatibility | Facade with no defaults, ui, and liquid configurations all pass. |
| Documentation | `RUSTDOCFLAGS=-Dwarnings` docs for flow/core/UI pass. |
| CI-style strict Clippy | Workspace/all-targets and render/facade/UI no-default/all-targets both pass without warning suppressions. |
| Facade lib/doctest compatibility | No-default lib, ui lib, ui doctests, and liquid+canvas lib all pass. |
| Whitespace | Gate-runner diff check passes; final staged diff checked separately before commit. |

The focused suite also covers subtree logical death, shared documents surviving
World/presentation loss, last-lease semantics, removed owner/stale bind refusal,
capture release on success/error/unwind/rejection, hidden/suspended behavior,
untouched refusal, duplicate/reentrant disposal, reentrant ancestor cleanup
ordering, entity recycling/destruction, callback-free coordinator Drop, every
identity counter near exhaustion, capacity/foreign identity and diagnostic bounds.
Initial compile-failure and earlier focused logs are retained as historical
engineering evidence; they are not the final candidate verification.

## Residual limits and next review

- This is macOS-local headless qualification, not the complete multi-platform,
  all-feature CI test matrix. Native IME/clipboard/AT, browser auth, GUI pixels,
  backend IO and Medousa integration were not tested or changed.
- A pump bounds callback count, not callback duration or total scan time. Scans
  are bounded by admitted storage but not incrementally time-sliced. No latency,
  throughput or performance equivalence is claimed.
- `World::remove` returns its component: host must immediately drop a removed
  ScopeOwner. Retaining/moving it is explicitly unsupported reparent/detach.
  World registrations must be bound to their corresponding World by the host.
- External retained owners/read-only liveness tokens and rejected captures are
  the client's responsibility. Never capture an owner in its own cleanup.
- Coordinator must remain outside participating Worlds and be explicitly drained
  before shutdown. Drop signals/releases captures but never invokes callbacks.
  Unwind catch is not recovery from abort/process loss/destructor double panic.
- Cleanup attempted and zero registry counts do not mean all IO stopped. F4 must
  separately account for local cancellation, acknowledged producer/future drain,
  failure/panic and unknown blocked work; no backend cancel is implied.
- F1's whole-stream staging cost and F2's inherited raw-push split-CRLF limitation
  are unchanged. No unrelated cleanup or compatibility debt suppression occurred.

Next: independent sender review of the exact committed candidate and native
integration decision. F4 requires a separately scoped assignment; this worker
has neither accepted nor integrated its own result.
