# F3 implementation / F4 readiness handoff

This document describes the F3 candidate built from accepted F0/F1/F2 baseline
`9f36c97271a3d2e7ead3049afdec89753ef990c3`. Historical status paragraphs in the
F0/epic records do not undo those acceptances. Worker completion is not sender
acceptance. Candidate identity and qualification are in
[`../reports/rndr-flow-f3.md`](../reports/rndr-flow-f3.md).

**Readiness:** the metadata/ownership substrate is implemented and headlessly
tested. F3's incremental-retirement correction is awaiting fresh independent
review and sender acceptance; F4 is only a plan for separately scoped work, not
already delivered or authorized by this document.
There are no task/feed adapters, replacement lanes, completion publication gate,
close guards, retained navigation or application migration in this candidate.
None of these tests establish platform, Medousa, backend cancellation or
performance readiness. F1's whole-stream staging cost caveat and F2's inherited
raw-push split-CRLF limitation remain unchanged.

## Implemented API and ownership boundaries

All names in this table actually exist in `crates/rndr_flow/src/lib.rs`.
`src/transition.rs` is a private pure state/checked-generation transition core;
`src/tests.rs`, `src/budget_tests.rs` and `tests/reviewer_probes.rs` contain the
deterministic lifecycle, work-accounting and retained adversarial proofs. Package `rndr-flow` exports
Rust library `rndr_flow`; only direct runtime dependency is feature-off
`rndr_core`. No facade export, reverse core/UI dependency, mandatory executor,
Tokio, native renderer, browser or network is introduced.

| Implemented API | Actual contract |
| --- | --- |
| `Coordinator::new(Limits) -> Result<(Coordinator, ScopeOwner), Error>` | Process-wide checked authority allocation; application root owner; all configured budgets positive. Coordinator stays in host outside participating Worlds. It can be stored in a UI-thread local by a host that arranges callback-safe extraction, but it must outlive its Worlds for orderly shutdown. |
| `root`, `create_scope(parent)` | Checked never-reused scope serial, exactly one immutable parent. No transfer/reparent/detach. IDs are process-local metadata, not domain IDs or authorization. Recreated semantic keys get new scopes. Dead slots still occupy capacity until drained. |
| `register_world() -> WorldOwner`, `WorldOwner::{id,scope,bind}` | Each World gets a distinct typed `WorldId` and root child scope; host moves the token into that exact World's local slot, once. Registry stores no World or Entity pointers and cannot mutate a different World. No cross-world resource lookup. |
| `ScopeOwner::{id,liveness,bind}`, `Liveness::is_live` | Nonclone single owner; optional core Component binding. Drop signals atomic terminal loss only. Liveness includes all ancestors, so a lagging World sees logical death before local physical drain. Never capture an owner in its own cleanup; capture IDs/read-only liveness instead. External retention of read-only tokens is the client's responsibility. |
| `state`, `set_visibility` | Visible/Hidden both retain resources and activity identity; visibility cannot leave Suspended/Disposed. Hidden does not promise offscreen interval polling. |
| `activity`, `activity_is_current`, `suspend(Recovery::ReconcileFromDurableState)`, `resume` | **Scope activity incarnation only**, not an F4 per-request lane/gate. Host explicitly asserts recovery is possible; no unrecoverable-stream suspension API. Suspend checks generation exhaustion before mutation, invalidates current activity, schedules Activity cleanup. Resume refuses while Activity cleanup remains and exposes a fresh incarnation; Lifetime cleanup/resources remain. It does not replay deltas or start work by itself. |
| `create_document`, `DocumentId::scope`, `document_is_live`, `lease`, `close_document` | Coordinator/application root is the sole document-lifetime manager. Host retains actual domain document ID/revision/state elsewhere. Presentation leases have no disposal-parent authority. Last release never closes the document; explicit document close refuses any live held lease. Direct `dispose(document.scope())` refuses. Root/application disposal affects the entire lease set. This is not F5 guarded save/close or a revision/participant snapshot protocol. |
| `register_cleanup(scope, kind, callback) -> Result<CleanupId, Rejected<F>>` | Bounded admission returns the **untouched** callback on Stale/Suspended/Capacity/Exhausted. Caller must clean rejected captures. Callback is FnOnce, may be non-Send, receives only a bounded command sink, and is consumed before invocation. No callback runs in admission. |
| `dispose`, `pump_work(budget) -> PumpWork`, `pump` | Disposal immediately closes subtree admission and invalidates active leases; physical slots remain charged until reclaimed. Terminal disposal is idempotent. `pump_work` resumes child-first, creation-ordered traversal/reverse cleanup-entry scanning and independent lease ancestry sweeping, charging every metadata/lease visit, restart and FIFO command. Callback count is separately capped. Newly dead ancestors or newly suspended children cause a charged restart before further callbacks. Live creation/registration schedule a later pass, not repeated preemption. Callback unwind/error continues. `pump` is a compatibility wrapper: 65,536 retirement units **plus unbudgeted exact `counts()` observation**, not a total-time bound. No core cascade changes. |
| `Commands::dispose` | FIFO deferred request, revalidated after callback before further callbacks, one command per work unit. Tiny budgets can defer that revalidation to the next pump. Full queue returns Capacity. No recursive registry access, arbitrary World mutation or async callback. Authoritative owner drop/disposal never needs command queue capacity for logical or eventual physical progress. |
| `counts`, `diagnostics`, `dropped_diagnostics` | Counts cover retained registry/cleanup/lease slots and active leases, pending dead scopes/callbacks and queued commands. Payload-free diagnostic ring evicts oldest event only, with saturating **telemetry** overflow count (not an identity). No immortal tombstones. Counts at zero mean registry cleanup attempted/reclaimed, **not** worker IO acknowledged or external backend stopped. |

F3 defaults from F0: 4096 admitted scopes, 256 cleanups/scope, 1024 queued callback
commands, 256 callback invocations/pump, 512 diagnostics. A separate 4096-lease
budget bounds presentation metadata (one lease need not imply another scope).
F4's 32 lanes/scope and later history/transition budgets are **not implemented**
in F3 because those tables do not exist yet. Retirement metadata work is now
incremental: each work unit has a constant number of ordered-table operations
(O(log capacity)); a traversal pass is O(scopes + cleanup entries) units and a
lease sweep O(leases * (depth + 1)). Death-triggered restarts can repeat prefixes,
explicitly charged. Finite admitted work/ordering changes plus positive pumps and
returning/unwinding callbacks guarantee eventual reclamation; no fixed completion
bound is claimed under endless new ancestor deaths/suspensions. Lease progress alternates
with traversal and is never reset by traversal restarts. Callback duration is
unbounded. Exact counts, document-close checks and at-capacity lease admission
are explicit synchronous observations outside bounded retirement; see the
[README](../../crates/rndr_flow/README.md#incremental-retirement-versus-exact-observation)
and [correction report](../reports/rndr-flow-f3-correction.md). There are no
real-time latency/performance claims.

## Exactly one supported World reconciliation contract

1. Host retains coordinator outside participating Worlds; registers each World
   once and stores `WorldOwner` in that World's local slot.
2. A semantic entity lifetime may own a `ScopeOwner` Component. Component
   replacement, entity destruction and World destruction drop stored tokens.
   `World::remove` returns ownership: **drop the returned token immediately**.
   Retaining/moving a removed token is not a supported detach/reparent mode.
   Explicit flow disposal may precede destruction; it does not despawn UI nodes.
3. Token drop only signals the already-admitted atomic owner flag. Descendant
   admission is immediately dead without a pump, a new queue allocation, or
   reliance on core hierarchy destruction order. Stale binding refuses.
4. Host calls `pump_work` (or observation-bearing `pump`) before result consumers,
   after callback batches and at
   reconciliation end, until pending state is drained. The host must arrange the
   proper per-World effect drains when F4 adds concrete handles. No plugin is
   silently installed; F3 does not claim it intercepts every core producer.
5. For orderly host shutdown: dispose/drop application owner, drain coordinator,
   then drop coordinator. Coordinator Drop marks retained flags dead and releases
   captures but **does not invoke arbitrary cleanup callbacks**. A host that
   skips the drain cannot claim unsubscribe or save durability. `panic=abort`,
   process loss and arbitrary destructor/double-panic recovery are not promised.

Tests cover despawn then explicit dispose, removed-component drop, retained
World loss, explicit terminal binding refusal, recycled entity IDs and duplicate
cleanup. There is no rival entity polling/destruction hook or core generation
identity scheme. F3 does not change ECS generation wrapping behavior.

## Existing concrete mechanisms F4 must reuse

These paths/symbols are inspected source at the accepted baseline, unchanged by
F3. Items in the right column are future requirements, not implemented APIs.

| Existing source / API | F4 adapter consequence |
| --- | --- |
| `crates/rndr_core/src/runtime.rs::TaskHandle::{cancel,is_cancelled,detach,Drop}`, `spawn`, `spawn_local`, `spawn_detached` | Retain concrete handles in bounded owned slots. Cancel/drop is local task cancellation, not a backend cancel command. Do not use detach in an ownership adapter. Rejecting registration returns the untouched handle for caller cancel/drop. Signal/drop task/feed slots before that scope's custom cleanup, still child-first. Do not represent handles as ordinary F3 custom callbacks if that would invert this ordering. |
| Same file `Job::poll`, `poll_local`, `wrap_output`, `drain_inbox`, `run_until_parked` | Cancellation can suppress a queued inbox delivery; it cannot recall an Event already committed. Background poll panic marks handle cancelled; local poll panic drops future without necessarily setting cancellation. Neither is a uniform terminal-status/worker-join oracle. A quiet handle is not success or physical quiescence. |
| `crates/rndr_core/src/feed.rs::{Feed,Sender,Epoch,FeedConfig}`, `Feed::{advance,epoch,cancel}`, `Sender::{epoch,send_at,try_send_at,cancelled,wait_until_cancelled}` | Capture epoch when request begins, not at final send. Use bounded queue/latest according to consumer loss semantics. Feed cancellation closes queues and wakes cooperative producers. Epoch uses saturating u64 today: do not inherit this as flow identity. Adapter must refuse refresh at max epoch before advancing/cancelling, or replace the feed under a fresh checked flow lane before work starts. Test near-max through a deterministic seam; do not weaken core semantics casually. |
| Same file `spawn_thread`, `spawn_task`, feature-gated `spawn_tokio` | Thread producer is detached and not joined; blocking IO can outlive feed closure. Task adapter uses injected executor; optional Tokio path uses an already-running host runtime. No new mandatory runtime/feature is justified. Distinguish observation release, cooperative producer exit ack and unknown externally blocked work. |
| `runtime.rs::TimerHandle::{cancel,Drop}`, `after`, `every`, `sleep` | Own existing handles; absolute/core manual clock semantics stay authoritative. Already-published timer events still need gate. `every` has bounded catchup; do not implement a competing timer wheel. |
| `crates/rndr_liquid_ui/src/state.rs::{subscribe_liquid_interactions_owned,unsubscribe_liquid_interactions,LiquidInteractionSubscription}` | Token Drop signals; explicit unsubscribe takes `&mut World` and drains captures. Legacy `subscribe_liquid_interactions -> ()` stays World-owned. UI-specific adapter belongs to consumer, **not** a flow -> UI dependency. F3 cleanup callback lacks `&mut World`; concrete per-World effect storage/drain must be designed in F4, not smuggled through a captured mutable World. |
| `crates/rndr_liquid_ui/src/{host.rs,feed.rs,state.rs,lifecycle.rs}` contextual feed ownership/retirement, handle APIs | Preserve F1 pending external-ack capacity accounting and F2 surface/block/resource identity and detailed error contract. Flow does not replace Liquid stores or bless legacy positional writes for delayed completion. Explicit observation retirement is distinct from backend work termination. |
| `crates/rndr_core/src/app.rs::App::{set_executor,set_clock,add_system_front,update}`, `world.rs::{insert_local,insert_resource}`, `sim.rs::Sim::{with_seed,advance,update,run_until_parked}`, `clock.rs::ManualClock` | Keep executor injection and non-Send futures/local captures. App pumps runtime/timers **before Input/Update**. An Update-front flow drain cannot undo earlier event publication; admission must also run at every mutation consumer (including Input). The host controls schedule wiring and cross-World owned messages. No borrowed World survives await. |
| `editing.rs::SourceSession::{admit,commit}`, `structured.rs::StructuredDocument`, `source_projection.rs` | Native document/revision, readonly/composition, admission/history remain authoritative. Gate then synchronous owner commit with no await/reentrant host callback between. A source projection is not an alternate mutation authority. Source and structured documents retain distinct authorities. |

## Proposed F4 implementation slices (not shipped APIs)

Each slice needs fresh authorization/review; symbols below describe behavior, not
API declarations. Keep the F3 no-facade/no-runtime dependency direction.

1. **Checked request replacement lanes (pure metadata + tests).** Bounded typed
   application lane keys (32/scope by default), never user prose; token captures
   coordinator authority + scope serial + scope activity incarnation + lane key
   + checked lane generation. Separate history, mode, feed refresh and save
   protection lanes. Validate/refuse exhaustion/capacity **before** cancelling
   the old valid request; increment replacement generation before old cancel/drop.
   Do not stale unrelated lanes. Stream lane maintains one generation for ordered
   multi-result output plus native session/turn/increasing-sequence validation;
   not a new generation per chunk. Refuse unrecoverable suspension.
2. **Concrete owned handle slots and per-World effects.** Bound admitted and
   retired/unacknowledged slots; retain the old work charge independently of
   publication eligibility (the accepted F1 retirement fix is a precedent).
   Cancel/drop tasks and feeds before custom cleanup; subscription removal runs
   only in owning World's drain outside registry/World resource borrows. Dead
   slots and drop flags must keep disposal progressing when command capacity is
   full. Preserve F3's explicit work accounting: bounded handle/effect scanning
   must not hide global scans, recursive traversal or exact observation inside a
   nominally budgeted pump. Exercise one-unit budgets at admitted capacities.
   No unbounded per-scope result buffer. Explicitly classify app-owned
   jobs versus UI observation of backend work. No implicit SDK cancel.
3. **Mutation-boundary completion envelope/adapters.** Carry captured exact
   domain document/session/turn/operation IDs, native revision and request token.
   Consumer validates current owner/activity/lane + native target/revision and
   then performs the existing synchronous owner admission/commit. Never derive
   target from selected tab, source byte position or recycled Entity. Prevent
   duplicates using operation/stream sequence semantics. No await, callback or
   schedule gap between validation and commit. Queue rejection/backpressure must
   not silently lose an admitted cleanup/ack obligation.
4. **Terminal outcomes / drain accounting.** Report logical dead, cleanup
   attempted, local task cancellation signalled, future/producer exit acknowledged,
   failed/panicked and unknown externally blocked work separately. F3 counts alone
   are insufficient. Wrapping future polling and producer exit with pre-admitted
   bounded status slots can normalize many outcomes without core API changes,
   including non-Send local paths. Investigate destructor/panic paths carefully.
   Never infer completion from `is_cancelled`, silence or `run_until_parked`.
5. **Host/consumer seam and full race qualification.** Compose schedule/drains,
   unchanged F1/F2 compatibility, feature-off/facade graph and repetition counts;
   exact native/application adapters remain separate. No F5 guard/close policy
   or Medousa migration in this slice.

### Possible minimal core seam — unresolved, separately justify before editing

No core source change was needed for F3. First attempt F4 normalization using
owned adapter futures/producers and bounded shared terminal slots. Existing
`TaskHandle` exposes cancellation only, not a common terminal reason or explicit
future-dropped/worker-finished acknowledgement. If wrappers cannot distinguish
runtime-discarded local/background panic/cancel paths without leaks, propose a
small additive read-only terminal-status/ack seam owned by core runtime, tested
for both `Job::poll` and `poll_local`; preserve current APIs and executor
injection. Do **not** add a new runtime, executor, broad completion gate or
flow dependency to core. Obtain separate justification/scope before modifying
core. Likewise feed epoch exhaustion does not authorize an unreviewed change
to `Feed::advance`; guard/replace in adapter first. A native external backend
conditional-cancel API is an application/backend concern, not a core seam.

## Deterministic F4 race / negative matrix

Use actual `App`/`Sim::with_seed`, manual time, controllable futures/producers,
pre-admitted status slots and host model write/revision/history counters; no
network/sleeps/paid inference. Run several fixed seeds and explicit orderings,
not only a happy-path async test. Every stale refusal asserts **zero domain
writes, history/revision increments and cross-target delivery**, not just a
returned error.

| Ordering / negative case | Required proof |
| --- | --- |
| Cancel/drop before first poll; cancel during completion | Output never commits; handle/capture eventually drains or unknown is explicit. |
| Complete into runtime inbox then dispose before inbox drain | Cancel suppresses what it can; consumer gate still rejects anything published. |
| Publish Event, then dispose before consumer (including Input) | Zero writes even though cancellation cannot recall event. |
| Close/reopen same logical chat/key; recycled entity index; foreign coordinator/World | Old scope/lane/domain combination cannot target replacement or another World. |
| Replace same-live-owner request, old finishes last | Only new lane can write; independent lane still works. |
| Duplicate terminal completion / duplicate ordered chunk / retired turn | No second write/history revision; stream accepts only exact native turn/increasing sequence. |
| Feed starts with epoch N, advances to N+1, N result arrives | Captured N send is rejected; never read current epoch at send to bless old work. |
| Feed emits before cancellation/retirement; result already merged | Consumer still rejects; retired unacknowledged work stays charged until exact ack. |
| Handle/cleanup/lane/status-slot capacity full; callback reentry | Untouched refusal, no partial cancellation of old request; disposal continues from admitted flags. |
| Scope/lane/epoch counter near max; new work rejected | Old valid token/result remains usable, no wrap/alias; explicit disposal still progresses. |
| Hidden -> visible; recoverable suspended -> resume | Hidden retains resources; old suspended output rejects; fresh identity and host durable reconciliation. |
| Unrecoverable stream suspension requested | Refusal before event loss/cancellation; no silent new buffering promise. |
| Cooperative vs deliberately blocked producer | Closure signals both; only acknowledged exit is called drained. Blocking work remains charged/unknown. No backend cancel inferred. |
| Background panic, local non-Send panic, explicit error, cancelled future Drop | Distinct terminal observation or conservatively Unknown; independent cleanup continues; no fabricated save success. |
| Timer fires then owner disposes; timer replaced; repeated firing | Old event cannot mutate; use existing catchup/deadline semantics. |
| Non-Send local future owns Rc; custom injected executor | Existing supported paths preserved, no Send bound creep or mandatory Tokio. |
| Domain revision/readonly/composition changes after request starts | Existing owner boundary refuses; no gate-only permission and no projection edits. |
| Cross-World logical death before local retirement message drains | Atomic liveness rejects immediately in lagging World; effect drain never reaches another World's resource by type. |
| 100+ lifecycle/replacement/disposal cycles with limited pumps | All registry/handle/cleanup/retired-ack counts return to baseline; diagnostics stay bounded. |

F5's dirty-document save/close revision snapshots, native `CloseGuard`
composition and F6 real app workflow proofs remain separate work. In particular
this F3 shared-lease refusal is not a promise of durable save, dynamic quit
protection or platform certification.
