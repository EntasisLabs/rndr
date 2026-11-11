# Epic: optional rndr-flow ownership and guarded transitions

**Status: proposed, not owner-accepted or implemented. Documentation/analysis only.** The package name `rndr-flow` is negotiable. This epic does not authorize its stories, create a crate, migrate Medousa, change application policy or certify a platform.

Companion: [architecture/readiness analysis](rndr-flow-analysis.md). Baseline: rndr **`82707632302c67a88bd84220b9ca262f63a25197`** (0.5.0), reviewed 2026-11-11 UTC. All source references below are pinned by that baseline, not by this document's eventual commit. Rake inspiration is pinned to **`e03e0c09bfcf5ad3b271b8185280284a22823e05`**; see the analysis for exact reference links, evidence classification, tests and limits.

## Read / continue here

1. Review the responsibility boundary and invariants (§1–3).
2. Accept or revise F0 decisions before authorizing implementation (§5).
3. Prioritize F1/F2 existing-owner correctness over public framework breadth.
4. Use F3–F5 for minimal ownership/transition infrastructure, F6 for the real existing application proof.
5. Keep migration-start and full-replacement gates separate (§8).

This follows the repository's `docs/design/` proposal/story/gate convention. Source/tests decide what exists; proposed names below are not public rndr APIs. No issue tracker, new demo application, dependency addition or new manifest accompanies the epic.

## 1. Outcome and scope

Applications can opt into a small coordination layer to own UI work, retain hidden state intentionally, reject obsolete completions, and asynchronously guard disposal. The first usable result is **four lifecycle proofs in the existing `apps/rndr-home` workflow**, not widget coverage or an independently impressive framework demo. That application is in Medousa, not this repository, and its checkout was not available in this admitted worktree.

### Goals

- App, window, tab, route-entry and document lifetimes have explicit identities and one disposal owner. The identity of a route destination is distinct from an instance/history entry.
- Task/feed/subscription ownership follows semantic lifetime, not paint realization or whichever tab is currently selected.
- Hidden, suspended, closing and disposed are distinct. Switching tabs does not accidentally dispose a stream or draft.
- Cancellation and stale-result validation compose with existing rndr runtime/feed mechanisms; already published events remain subject to owner checks.
- Save/discard/cancel guards are asynchronous, revision-checked, reentrancy-safe and nonblocking. Confirmed disposal consumes cleanup exactly once; cancelled close preserves state.
- Headless deterministic tests cover core logic, including race orderings, with no required native renderer, browser, network or particular executor.
- Diagnostics show ownership, outstanding work, pending transitions and cleanup reasons with bounded retention.

### Non-goals

- A second UI/component tree, competing application/domain state store, new editor authority or undo stack.
- A wholesale Rake clone, GPUI adoption, dependency on Rake, mandatory Tokio/executor, service locator or remote-control server.
- Implementing canvas caret/selection/IME, full Live Markdown + Liquid, renderer reconciliation via scope metadata, icons/effects/detail sheets, or artifact script security inside flow.
- Medousa approval/security policy, persistence/recovery, transport, credentials, backend job orchestration, business logic, OS packaging/signing/updaters or mobile/browser support promises.
- A disconnected demo, a big-bang port, publication/deployment/push, or changing rndr's primary-window/quit behavior to Rake's behavior.

### Definition of done for a future implementation

F0 decisions accepted; F1/F2 cleanup/identity behavior independently reviewed; F3/F4 deterministic ownership tests passing; F5 guarded transitions and native gate composition verified; F6 four proofs passing against an exact reproducible application baseline; F7 compatibility/diagnostic review complete. Renderer/editor/security/native tracks keep their own unresolved gates. No claim that this whole epic is done from unit tests or a worker handoff alone.

## 2. Ownership and dependency direction

```text
application + host adapters
  ├── existing rndr facade/UI/Liquid/browser as needed
  └── optional rndr-flow
        └── rndr_core (headless World/App/events/runtime/feed/Sim)
```

A UI-aware adapter may depend on the necessary UI crate, but core must never depend on flow. Begin with an internal pure transition module and existing concrete handle types; split additional crates only after demonstrated reuse. Default-feature-off builds must not pull in renderer/browser/native window services or an executor dependency. Consider extending `rndr_app` instead if F0 cannot justify an independently reusable ownership surface.

| Owner | Must do | Must not do |
| --- | --- | --- |
| rndr core/render/UI/Liquid | Entity/component lifetimes, reconciliation, input/IME/AT/DPI, task/feed semantics, Liquid state retirement and identity, native close gate | Delegate renderer identity or editing correctness to an unrelated scope registry |
| Proposed flow | Scope/operation metadata, owned handle groups, token validation, typed history/transition coordination, consumable cleanup, bounded observations | Mirror domain state, save documents, grant permissions, stop external durable work by UI teardown |
| Application | Actual document/chat model, dirty revisions, save durability, auth/approvals, SDK subscriptions, backend job policy, transport/persistence, choice presentation, packaging | Treat a flow token or navigation success as authorization, backend cancellation or persistence acknowledgment |

Use existing `World` resources/components and UI-thread locals, `Plugin`, `Schedule::Update`, `TaskHandle`, `Feed::advance`/captured `Epoch`, `CloseGuard`, public `UiId`-based components and editing admission. See the analysis §2 for exact paths/symbols. Private `rndr_ui::lifecycle` helpers are not extension APIs.

### Lifetime model to ratify in F0

- An application coordinator roots UI lifetimes; a window registration maps to its actual rndr World. Do not assume different windows share a World. Cross-window coordination is explicit host wiring.
- Each tab owns retained route entries and bounded typed history. A route value may occur in multiple entries with distinct identities.
- A document model may be shared by several presentations. Choose one document-lifetime owner (application/document manager), with borrowed entry references. Closing one presentation releases its lease, not every presentation or the backend document. Do not give several parents permission to dispose the same object.
- `Visible` and `Hidden` are presentation/activity policy; both retain a live scope. `Suspended` pauses selected activity explicitly, invalidates that activity's request generation, and defines buffering/resume behavior. It is not a synonym for hidden or dead.
- `Closing` retains state while guards run; a cancelled/failed close returns to an intact live state. Terminal disposal has a no-return logical invalidation point before effects are emitted.
- A Liquid surface owns semantic blocks; viewport mount/unmount is underneath that lifetime. Removing a block, replacing its semantic identity and recycling it have distinct renderer policies.
- No detached/reparented work in version one. Independently owned backend work must be classified at creation. Future ownership transfer would require its own atomic contract, not a magic `detach` escape hatch.

## 3. Correctness contracts before API breadth

### 3.1 Completion admission and cancellation races

A request captures exact scope incarnation, request generation and domain target at submission. Before applying its result on the UI thread, validate all three, then invoke the existing model's revision/admission boundary. This check occurs even if the runtime has already checked handle cancellation. Feed producers capture an epoch when work begins and call `send_at(epoch, ...)`; reading the latest epoch just before sending would incorrectly bless old work.

Required orderings include: cancellation before first poll; cancel while future completes; queue then cancel before inbox drain; publish event then close before consumption; close/reopen same logical chat; replace request without disposing scope; duplicate completion. Cancellation cannot roll back an already committed event or IO side effect. Generation/ID exhaustion fails admission rather than silently reusing an incarnation (policy to ratify; rndr entity generations alone wrap).

No task may capture a mutable World across await. Non-Send local futures stay UI-thread-local; transferable handles and immutable owned completion data cross worker boundaries. Async application work never gets an unchecked model pointer through flow.

### 3.2 Disposal and subscriptions

Recommended disposal sequence:

1. Commit logical disposal: reject new work and invalidate owner/request tokens.
2. Remove owned registrations from the registry before invoking any external cleanup code. Snapshot child order and owned cancellation handles without holding registry borrows across callbacks.
3. Dispose owned child scopes in a documented deterministic order (recommend child-first cleanup); signal/drop runtime handles and feeds; consume unsubscribe/cleanup registrations once.
4. Record cleanup errors and continue remaining independent cleanup. Enqueue any nested dispose request rather than recursively borrowing the coordinator. No resurrection of a disposed incarnation.
5. Publish a bounded disposal outcome. Reclaim tombstones only after stale-token rejection remains guaranteed. Track cooperative task drain separately from logical disposal.

Exactly-once means one invocation of each successfully registered cleanup in an orderly process, including repeat/reentrant disposal calls. It does not guarantee an external service performed its effect once, panic-abort safety, forced-exit cleanup, or destructor execution after a crash. For unwind-capable cleanup callbacks, catch at a reviewed boundary, record failure and continue; do not claim catching arbitrary future Drop panics makes the process safe.

Subscription admission racing disposal either installs under the live owner or returns ownership to the caller for immediate cleanup. No successful registration becomes orphaned. A drop token may only queue cleanup when World access is needed; explicit UI-thread draining gives the baseline measurement point. Decide this before exposing RAII semantics. Existing append-only Liquid listeners need F1 upstream changes or one app-owned dispatcher; a flow wrapper alone cannot remove them.

### 3.3 Typed navigation/history

Use an application enum such as `Route::Chat(ChatId)` / `Route::Document(DocumentId)`. Resolve contextual targets at submission to exact window/tab/entry IDs. Back/forward reuse retained entries; push creates a new entry; replace and forward-history truncation guard every entry they would destroy. Selection alone does not run disposal guards. Preserve each entry's local state; no second UI tree.

Choose explicit bounds for tabs, histories, pending operations and completed outcomes. Capacity should refuse or run an explicit guarded eviction policy, not silently dispose dirty history. URI encoding/deep-link parsing is a later host adapter; authorization remains host-owned. No derive macros are required for the first typed enum/state-machine API.

### 3.4 Deferred close protocol

Proposed phases:

```text
Requested(exact target, operation, revision)
  -> Guarding -> AwaitingDecision(save/discard/cancel)
  -> Preparing(save) -> Revalidating -> CommitDisposal -> Disposed
       |                   |
       +-> Failed/Blocked  +-> Guarding again if changed
Any pre-commit phase -> Cancelled (retains state)
```

A clean document may proceed directly after validation. Dirty state offers choices supplied by the host. The save handler must acknowledge actual host-defined durability before clearing the current dirty revision. Failure leaves state intact and offers retry/cancel/informed discard. Direct discard is an explicit informed choice, not fabricated save success; the native gate's existing `discard` method specifically addresses a current failed protection request, so do not alter that meaning.

Each choice is bound to operation, target incarnation, allowed choices and dirty revision. Editing while already dirty invalidates old decisions. Save started at revision N cannot authorize closing revision N+1. Choose either mutation freeze across all input/IME/programmatic writers or complete revision invalidation at their authoritative boundaries. Revalidate all affected targets together before any is disposed. Completed saves are external effects and are not rolled back on cancel.

Repeated close requests for the same target coalesce. Conflicting operations serialize per target; unrelated tabs progress. Awaiting a guard must not hold the scheduling lane needed by that guard's save command. Nested navigation/close from callbacks is queued and revalidated; a save job must not await the very quit operation that is waiting for it. Cancellation wins before the commit point; after commit it returns an explicit too-late outcome. A dropped observer/timeout is not a choice and does not cancel implicitly.

Compose window/quit protection through `rndr_core::CloseGuard`. Pending flow transitions do not call `complete(Ok(()))` early. Hidden entries and all participating windows remain covered by the native runner's close contract. Native close cancel preserves UI models; it does not pretend to cancel already committed saves. Forced process termination remains outside this protocol.

### 3.5 Durable jobs are not UI tasks

For each stream classify (a) UI-owned request, (b) UI observation of independently owned backend work, or (c) explicit application-owned finite work. Closing a tab cancels its UI request/observation/subscription. A backend job continues or receives a separately authorized cancel request according to application policy. Dropping `TaskHandle`, feed or view is not proof that the backend stopped. App-owned work still needs explicit app shutdown policy; flow must not infer backend cancellation from native quit.

### 3.6 Errors, panics and observability

Separate succeeded, application error, cancelled, panicked, stale-discarded and disposed outcomes. Existing background/local panic reporting differs (`runtime.rs`); inspect rather than map both to a false success or a permanent spinner. A lost result leaves a guard pending/failed, never ready. Timeout UI offers retry/cancel but does not auto-discard. Diagnostic event loss cannot change authoritative state.

Expose snapshots/counters and an optional bounded event ring: live scopes by kind/state, owned task handles versus actual known running/drained work, cleanup registrations, operation/decision phases, stale rejection reasons and disposal failures. Bound/redact labels; never retain payloads, prompts, tokens, URLs or document bodies by default. Completed outcomes/tombstones have explicit retention and overflow semantics. A diagnostics API is not a mutation permission surface or a control transport.

## 4. Minimal API sketches — explicitly proposed, not compiling contracts

The following is **illustrative Rust-like pseudocode**. None of these names exist by virtue of this document. F0/F3 must resolve ownership, borrowing, local/Send bounds, error types and feature placement before publication. Prefer fewer public types than the sketches if existing rndr APIs express the contract.

```rust,ignore
// Proposed metadata, not a parallel domain state store.
struct ScopeId { slot: u64, incarnation: u64 }
struct RequestToken { owner: ScopeId, generation: u64 }
enum ScopeKind { App, Window, Tab, Entry, Document, Surface }
enum Visibility { Visible, Hidden, Suspended }

// Proposed flow resource installed by an opt-in headless Plugin.
// Diagram of operations, not actual fn signatures:
flow.create_child(parent, kind) -> Result<ScopeId, FlowError>
flow.set_visibility(owner, visibility) -> Result<(), FlowError>
flow.begin_request(owner, request_key) -> Result<RequestToken, FlowError>
flow.own_task(owner, task: rndr_core::TaskHandle) -> Result<(), RejectedTask>
flow.register_cleanup(owner, cleanup) -> Result<CleanupId, RejectedCleanup>
flow.dispose(owner, reason) -> DisposeOutcome
flow.snapshot() -> BoundedSnapshot
```

`RejectedTask`/`RejectedCleanup` return ownership or deterministically cancel/clean it; choose one documented rule. No token is an authorization capability. Internal request keys identify replacement lanes (e.g. history load), not backend job IDs or network retry/idempotency keys.

```rust,ignore
// Proposed application use of existing runtime + guarded result consumption.
struct LoadedChat {
    request: RequestToken,
    chat: ChatId,
    expected_revision: ChatRevision,
    result: Result<ChatHistory, LoadError>,
}
impl rndr_core::Event for LoadedChat {}

// Capture A and its token now; never read current selected chat inside completion.
let request = flow.begin_request(chat_a_scope, HistoryLoad)?;
let task = rndr_core::spawn(world, async move {
    LoadedChat { request, chat: a, expected_revision, result: load_a().await }
});
flow.own_task(chat_a_scope, task)?;

// During Update, an adapter validates + commits synchronously without await:
adapter.apply_if_current(world, event.request, |world| {
    host_chat_model(world).apply_history_if_revision(
        event.chat, event.expected_revision, event.result
    )
});
```

`apply_if_current` represents one reviewed mutation-boundary operation, **not** an invitation to validate once and mutate after an async gap or reentrant callback. Implementation must avoid aliased mutable borrows of World/coordinator. Existing source/structured admission still runs within the host model operation.

```rust,ignore
// Proposed typed coordination only; host supplies views, saves and choices.
enum Route { Chat(ChatId), Document(DocumentId) }
enum Transition<R> {
    Push { tab: TabId, route: R },
    Replace { entry: EntryId, route: R },
    Back { tab: TabId }, Forward { tab: TabId }, Close { target: TargetId },
}
enum GuardReply { Allow, Block(Reason), Ask(DecisionDescriptor) }
// request -> operation handle; response names exact decision and revision;
// observe -> owned state; cancel -> explicit pre-commit request.
```

Do not add a `Route::mount` trait that constructs a rival component tree, encode UI views into the core, or make observer Drop cancel an operation. API sketches do not imply a route loader/cache framework in version one. The first proof can use host-created views with existing `UiId` identities.

## 5. Sequenced stories and acceptance criteria

Stories are bounded follow-on undertakings or review units, not an authorization to implement them now. Estimates are deliberately omitted until the owning contributor validates scope. Each implementation change needs focused before/after tests, a final diff and owner review. Existing unrelated work remains preserved.

```text
F0 accepted boundary + contracts
 ├─ F1 Liquid terminal cleanup + removable subscriptions
 ├─ F2 Liquid identity/replacement policy (coordinate with F1)
 └─ F3 headless scopes + consumable cleanup
       └─ F4 owned async work + stale completion adapter
             └─ F5 typed retained history + guarded transitions
F1 + F2 + F3 + F4 + F5
 └─ F6 existing rndr-home proof workflow (separate application authority)
       └─ F7 rollout/compatibility/diagnostic acceptance
```

F1/F2 correctness evidence is a release gate for the Liquid proof, not an excuse to wait for broad route macros. F3 design can proceed after F0, but do not release a framework that masks F1/F2. F1/F2 touch shared state/view semantics: implement serially or agree exact interfaces before concurrent edits. No parallel work is dispatched by this epic.

### F0 — contract and placement decision (P0)

**Owner:** rndr maintainer with application integration reviewer. **Input:** this epic and baseline refresh. **Output:** accepted decision record and exact implementation slices.

- Decide companion versus `rndr_app`, package name, feature surface and dependency direction. Confirm no core-to-flow edge or mandatory executor.
- Ratify scope topology/shared-document leases, IDs/exhaustion, request lanes, hidden/suspend policy, cleanup order/drain semantics, error/panic handling and capacity limits.
- Map actual current Medousa chat/session/cancellation/approval/SDK contracts read-only in a separately admitted application checkout. Record exact app commit, dirty state and `rndr-home` source paths. Lack of checkout is a blocker, not an inferred interface.
- Select mutation-freeze or revision invalidation for all guard-relevant writers and map native `CloseGuard` integration across worlds.
- **Acceptance:** reviewer can trace every state mutation, cleanup and policy to exactly one owner; four proofs have test oracles; source baseline differences are recorded; no implementation begins under documentation-only authority.

### F1 — terminal Liquid resource ownership (P0; existing renderer owner)

**Depends:** F0. **Likely paths:** `rndr_liquid_ui/src/state.rs`, `view.rs`, core `markdown/host.rs` only if a narrow lifecycle-reason seam is needed, focused tests. Refresh callers before editing.

- Promote P1 to a permanent failing desired-behavior regression: actual accordion interaction, replace/remove, cascade-despawn, repeated owner creation/destruction.
- Define terminal view/block cleanup distinct from offscreen recycling. Records, saved state, timer/feed resources and nested surfaces follow explicit semantic ownership. Ensure direct supported destruction paths cannot bypass cleanup; do not require every caller to remember an undocumented special destructor.
- Add a reviewed removable interaction-listener token/API or explicit dispatcher ownership. Preserve existing app-global subscribe behavior through a documented compatibility path; do not silently switch legacy void-return subscriptions to immediate drop-cancellation.
- Handle subscribe/unsubscribe during dispatch and reentrant disposal without lost registrations, duplicate callback or orphaned captures.
- **Acceptance:** after warm-up, 100 mount/remove/replace/dispose cycles return semantic record/listener/owned-resource counts to baseline after the defined pump; cleanup once; offscreen state/timers/feed last-good values and append tests still pass. Counts distinguish intentional app-global listeners. World teardown is not the only cleanup path. No flow dependency introduced to Liquid/core.

### F2 — Liquid identity and replacement contract (P0; existing renderer owner)

**Depends:** F0 and agreed F1 interfaces. **Likely paths:** Liquid parser identity representation if needed, `state.rs`, `view.rs`, focused reconciliation tests.

- Decide host-stable IDs versus parser-derived identity plus explicit reset, and compatibility for existing source-key APIs. Define semantic replacement when the new block remains at the old source byte.
- Use P2 as characterization, not an automatic mandate to reset all edits. Specify and test preserve/reset outcomes for same-ID edits, changed-ID replacements, duplicates, reorder, prefix insertion, kind changes, offscreen edits, remove/reinsert, streaming append and explicit reset.
- Reconcile resource records, mounted state and carried/saved state with the **same** semantic identity policy; source-position lookup must not silently override a different policy.
- Revision checks must reject updates for retired/replaced block incarnations. Avoid state transfer across duplicate unrelated blocks or reused view identities.
- **Acceptance:** a documented table determines every case; tests assert exact retained/reset state and retirement, not just rendered label existence. Append/offscreen guarantees remain; legacy source-key compatibility is explicit. Flow tokens are not used to conceal a renderer identity defect.

### F3 — minimal headless ownership core (P0)

**Depends:** F0. **Output:** optional crate or accepted `rndr_app` module, no UI breadth.

- Implement scope metadata/resource, single-parent ownership, shared-document borrowed references, visible/hidden/suspended/disposed state and no resurrection.
- Consumable cleanup registrations handle success, error, duplicate/reentrant dispose, registration-after-disposal and dropped owner. Existing World component destruction and explicit flow disposal have one supported reconciliation contract.
- Record bounded counters and logical-versus-physical completion status. Host callbacks run outside registry borrows; avoid capture cycles and expose pending cleanup/drain state where needed.
- **Acceptance:** deterministic headless tests cover parent/child disposal and shared presentation leases; 100 cycles return registry/cleanup counts to baseline; stale IDs/exhaustion refuse; hidden retains resources; suspended restarts get fresh request identity; no native renderer, network, browser or Tokio needed. Existing core/UI do not depend on this optional layer.

### F4 — existing-runtime ownership adapter and completion gates (P0)

**Depends:** F3. **Likely paths:** new flow module/tests and minimal core observability seam only if separately justified.

- Retain/group `TaskHandle`, feeds/timers and producer unsubscribe handles; make task replacement lanes explicit. Rejected ownership registration cleans up safely.
- Validate completions at the UI mutation boundary; use captured feed epochs and exact domain IDs. No selected-tab lookup on completion. Distinguish app-owned work and UI observations of backend work.
- Surface terminal error/panic/cancellation without assuming local/background paths already report the same thing. Keep executor injection and non-Send local futures supported.
- **Acceptance:** all race orderings in §3.1 pass using `Sim`/manual clock and seeded scheduler; include already-published event then owner disposal, same-live-owner request replacement, close/reopen, blocking/cooperative producer distinctions and duplicate completion. Rejected stale results cause zero domain writes, zero history/revision increments and zero cross-chat delivery. No executor/runtime replacement.

### F5 — typed retained transitions and close guards (P1, after correctness core)

**Depends:** F3/F4. **Output:** minimum route/history/decision engine and host adapter, not URI macros/service framework.

- Implement exact-target typed push/replace/back/forward/select/close with bounds and explicit retained entry ownership.
- Defer transitions through owned operation/decision state. Save/discard/cancel, dirty revision changes, multi-entry atomic validation and technical failure follow §3.4.
- Compose window/quit with current `CloseGuard`; preserve hidden/all-world participation and runner close policy. Revalidate dynamic window admission rather than caching an obsolete set of protected entries.
- Queue reentrant requests; serialize conflicting target operations; unrelated tabs continue. Observer drop and timeout do not act as implicit choices.
- **Acceptance:** cancelled unsaved close preserves exact document bytes, revision/history, selection and owner resources; confirmed close disposes once; stale/duplicate/foreign save or decision cannot close; save failure/panic remains safe; concurrent close coalesces; edit-after-save-start requires new protection; back/forward retain identities; capacity refusal leaves history unchanged; app quit cannot deadlock waiting for its own save task. Multi-window adapter tests do not claim native certification.

### F6 — four proofs in existing rndr-home (P1; application owner)

**Depends:** F1–F5 plus a separately authorized Medousa application scope. **No Medousa implementation is authorized by this rndr epic.** Keep SDK sessions, approval/security policy, durable backend jobs and transport authoritative. Do not build another demo in rndr as a substitute.

- Refresh exact app/rndr commits and lock/dependency/offline baseline without rewriting unrelated files or moving the reviewed rndr baseline silently.
- Wire existing views and adapters to flow incrementally behind an opt-in application switch. Exercise the proof matrix in §6 with the real chat/shell model; deterministic fakes are used at IO boundaries, not a second application state model.
- Show host-owned UI for loading/error/decisions and explicitly log backend policy on tab close.
- **Acceptance:** all four proofs pass headlessly against actual model adapters, and bounded live workflow evidence records OS/input/display context. Offline locked preview build is reproducible. No late delivery, lost approval or backend cancel inferred from a dropped view. F1/F2 baseline counters hold in the integrated loop. Native qualification remains separately tracked.

### F7 — compatibility, diagnostics and rollout review (P1)

**Depends:** F6; diagnostics incrementally implemented with F3/F4 rather than bolted on at the end.

- Review all new public names/feature flags; add examples to existing docs with honest proposed/implemented status. Ensure prior rndr consumers need no flow adoption.
- Bound snapshots, terminal outcomes, labels and stale/disposal events; test retention overflow and slow observers. Leak baseline counts are authoritative measurements, not inferred from process RSS.
- Document task ownership, Drop/detach differences and conversion of existing append-only listeners. Establish owner-reviewed release notes and rollback procedure.
- **Acceptance:** feature-off/core-only builds and existing compatibility suites pass; no default dependency expansion; real workflow proof evidence and residual risks are accepted by the owner. Integration/publishing remains a separate explicit action, never implied by worker completion.

### Deferred breadth (P2, not prerequisites)

Route derive/URI codec, remote inspection/control transport, advanced route loaders/caching/retry frameworks, persistence/restoration, activity factories, tab transfer, complex eviction and packaged tooling. Each needs demonstrated reuse and separate review. Icons/effects/detail sheets remain renderer stories; editing/IME/security work remains independent and may be higher priority for a particular surface than any P2 flow feature.

## 6. Four proof scenarios and exact oracles

| Scenario | Setup and forced interleavings | Required oracle |
| --- | --- | --- |
| A→B while A loads | Start A history load; switch to B; complete B then A and reverse order; queue A result before switch then consume after; close/reopen A and replace A request | B receives no A data, errors or tool/approval updates. A result either updates retained A under explicit policy or is rejected as stale. Record exact chat/session/incarnation/request identity and zero writes to wrong target. Selected-tab changes never retarget delivery. |
| Close streaming tab | Stream UI feed; attach host SDK observation; associate an independently owned backend job; close while feed queue/completion is ready | UI handles/feeds/subscriptions and Liquid surface state return to baseline after pump. Job is neither implicitly killed nor falsely declared stopped. Continue or explicit host-authorized cancel policy is asserted separately; late UI deliveries are rejected. Other tabs/app jobs survive. |
| Unsaved document close | Dirty document, request close, cancel; request again, async save/discard; edit during save; fail save; repeated/reentrant close/choice | Cancel keeps bytes, undo/history/revision, selection and scope resources. Save for stale revision cannot authorize disposal. Failed save retains model. Informed discard/successful current save commits once; all cleanup once; repeated/stale/foreign choices refuse. |
| Liquid mount/remove/replace | Warm up; repeatedly open/toggle/timer/feed, scroll offscreen/back, append, replace same source, reorder/duplicate, remove and terminally dispose | Offscreen/appends preserve intended identity/state. Explicit replacements follow F2 policy. After 100 terminal cycles semantic records, listener captures, owned tasks/feeds and cleanup registrations equal baseline at defined drain point. No state resurrection from dead view or old block identity. |

Count logical resources and drained/parked tasks separately. A worker blocked in uncontrolled IO is a visible residual, not an excuse to mark physical cleanup complete. Tests should retain weak/drop sentinels to expose capture cycles rather than only counting registry entries.

## 7. Test and evidence strategy

### First reuse current baseline evidence

The analysis §7 records **41 existing passing headless tests + 2 disposable characterizations**. Those tests prove current task/feed/close/recycling behavior, not this proposed API. P1/P2 are intentionally green assertions of current undesired/unspecified retention; implementation stories replace them with accepted desired behavior and permanent regression coverage.

### Verification layers for implementation stories

1. **Pure/headless:** transition tables, property/model-based operation sequences, deterministic ID/capacity failure, callbacks reentering, cleanup errors/unwind, request replacement and stale completion refusal. Use `App`, `Sim`, manual time and multiple fixed scheduler seeds; do not rely on sleeps to order races.
2. **Existing runtime:** keep `runtime`, `feed`, `close` tests green; add task queue-before-cancel and already-published result tests. Include pooled execution only where available, with bounded timeouts and no dependency on real transport.
3. **Renderer integration:** Liquid state/recycle/feed/timer tests plus terminal cleanup/identity matrix, nested surface teardown and app-global versus surface-owned listeners. Include direct supported destruction paths and unavailable/stale handles.
4. **Application adapter:** four proofs using actual `rndr-home` model/SDK boundary fakes, current authorization checks and explicit backend ownership. Lock and dependency evidence names both repositories/revisions.
5. **Native acceptance:** keyboard focus/shortcuts, IME preedit/commit/cancel, clipboard, screen-reader roles/actions, mixed DPI/window movement, resize, hidden windows, safe close and host browser intents. Record OS, hardware/display scales, input method, date and result; never infer certification from headless or compilation success.
6. **Soak/cost:** warmed counts and bounded queue/retention high-water marks; repeat 100 cycles deterministically before longer owner-approved soaks. Assert no monotonic semantic-resource growth. Report time/allocations if claiming performance; no unbounded build or soak by default.

Run focused checks first; future source changes use relevant `CONTRIBUTING.md`/CI gates, including feature-off and facade compatibility tests. Typical starting commands (future new test names are not asserted to exist):

```sh
cargo test --locked --offline -p rndr_core --test runtime --test feed
cargo test --locked --offline -p rndr_core --lib close::tests
cargo test --locked --offline -p rndr_liquid_ui --test state --test recycle --test feeds --test timer
# After an approved companion exists, select its exact package spelling:
# cargo test --locked --offline -p rndr-flow --no-default-features
# Then affected core/UI/render/facade checks from CONTRIBUTING.md and CI.
git diff --check
```

Do not silently refresh a consumer lockfile to turn a failing `--locked --offline` check into a passing claim. Diagnose in an isolated disposable copy under appropriate scope, report differences, then authorize a deliberate lock/dependency update. This documentation pass ran no Medousa build and makes no current lock-drift claim.

## 8. Rollout, backwards compatibility and migration gates

### Rollout

- Fix F1/F2 in their existing owners without requiring flow. Keep legacy subscriptions/source-key APIs usable or provide explicit deprecation/transition adapters with tests.
- Ship optional ownership core only after F0/F3/F4 review. Existing apps retain existing behavior; neither hidden-state policy nor native quit semantics changes without opt-in.
- Adopt first for one `rndr-home` chat load/observation and one dirty-document close. Observe counters/rejections; extend ownership after evidence. Avoid running old and new disposers simultaneously.
- Keep source editing fallback, current approval/security boundaries and Tauri available. An application feature switch can revert adapter use, but cannot resurrect already disposed UI or undo backend effects. Rollback stops new flow admission, drains/cancels observers explicitly and restores prior routing under application review.
- Public APIs get release notes and compile/feature compatibility checks; no version/publish action is part of this epic.

### Gate ledger (all are pending unless independently evidenced)

| Gate | Evidence required | Does not require / does not establish |
| --- | --- | --- |
| **G0: authorize foundation implementation** | Accepted F0 contract, exact fresh baseline, bounded scoped work units and owner review | This document alone is not implementation authorization. |
| **G1: ownership correctness ready** | F1/F2 renderer cleanup/identity regressions resolved; F3/F4 race/cleanup tests; explicit backend boundary | No need for route macros, remote control, icons or full canvas parity. Does not establish app integration. |
| **G2: start incremental chat/shell migration** | G1, F5 safe-close integration, reproducible exact `rndr-home` dependency/lock/offline baseline, four real-model proofs, preserved SDK/session/approval/attachment/link/copy/streaming behavior for the selected slice; owner accepts residual native risks | Does not require full Live/canvas/artifact parity. Prior review's “ready to start” is not newly recertified here. |
| **G3: expand a specific surface** | Selected surface has its own renderer/editing/security/native gates and usable source/fallback path where appropriate | A chat success does not qualify rich Live notes, canvas or interactive HTML. |
| **G4: full Tauri replacement** | Everyday workflow acceptance, native keyboard/IME/AT/DPI and multiwindow qualification, durable close/recovery, Live/canvas/artifact contract/security parity or explicit owner-approved exclusions, packaging/OS services and rollback evidence | Green unit tests, a flow crate or a preview build alone cannot retire Tauri. Packaging/security remain host-owned. |

Independent gaps retain explicit tracks: canvas caret/selection/IME and image import; full Live Markdown + embedded Liquid; restricted browser versus host artifact interaction/security; icons/effects/detail sheets; platform qualification. Flow readiness cannot close these cells by architecture alone.

## 9. Risks and decisions still requiring an owner

| Risk / open decision | Mitigation or decision gate |
| --- | --- |
| New crate duplicates rndr runtime or `rndr_app` | F0 placement review and dependency tests; favor concrete handle grouping and small pure coordinator. |
| Scope graph becomes a second domain state model | Store IDs/relations/operation metadata only; host retains document/chat/selection/edit authority. |
| ECS generation versus request/document identity confused | Distinct types/incarnations, exact mutation-boundary checks and exhaustion policy. |
| Shared document closed by one presentation | Single document owner + leases; explicit final-owner disposal guard. |
| Cancellation races or already-published events | Runtime cancellation plus consumer token/domain revision validation; test all publication boundaries. |
| Unsubscribe/cleanup reentrancy, panic, cycles | Consume registrations before callback, queue nested operations, weak/drop sentinels; document abort/forced-exit limits. |
| Legacy Liquid subscription source compatibility | Add explicit owned/removable alternative; preserve app-global old contract rather than returning a token old callers immediately drop. |
| Dirty revision missed by IME/programmatic edits | Map every authoritative writer; freeze or invalidate before native Ready is consumed. No safety claim from tokens alone. |
| Guard self-dependency / arbitrary callback navigation | No held scheduler lane across save/choice; exact-target queues, revalidation and explicit too-late cancellation boundary. |
| Hidden/suspended streams grow or stop unexpectedly | Explicit per-activity keep/pause/cancel policy and bounded/latest buffering; backend lifetime remains separate. |
| Pending operations/diagnostics retain everything | Admission/retention limits and clear overflow outcomes; pending pressure refuses new work, never auto-approves or silently drops guards. |
| New multi-window coordinator conflicts with per-World close gates | Narrow host adapter, dynamic participant validation, existing runner semantics retained. |
| Source-key identity migration loses interactive state | F2 preserve/reset matrix and compatibility adapter; do not promise implicit remapping of arbitrary edits. |
| Application checkout/build evidence unavailable | Concrete F0/F6/G2 blocker; obtain admitted source and exact dependency/lock evidence, not invented integration paths or re-used historical test counts. |

## 10. Review handback checklist

- [ ] Scope and placement accepted; API sketches still clearly separated from implemented API.
- [ ] Priority order keeps existing-owner correctness ahead of optional framework breadth.
- [ ] Every story has an owner, dependency, bounded output, negative cases and observable acceptance.
- [ ] Four proofs exercise the existing application workflow, not a substitute tree/model.
- [ ] Renderer versus flow versus host responsibilities stay explicit; native close and editing authorities are preserved.
- [ ] Baseline, actual commands/results and unverified native/application evidence are recorded.
- [ ] No implementation/migration/publish/integration is implied by accepting these documents.

**Current handback:** two review documents only. See the analysis for verified gaps and exact validation. The sender retains review and native integration; future implementation requires its own scoped authorization.
