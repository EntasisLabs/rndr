# F1/F2 bounded implementation and proof plan

**Status: decided specification subject to sender review, not implemented.** Read the [F0 decision](rndr-flow-f0-contract.md) first. These are separately reviewable implementation assignments, not implementation authorization from this documentation turn. Land F0 through sender review/native integration first. Then implement **F1 -> F2 serially**, because both own `state.rs`/`view.rs`; no concurrent files may be overwritten. No flow crate, Medousa edit, migration, publish, push or deployment is in either slice. A new baseline must record the accepted F0 commit and refresh affected callers before editing.

## A. F1 — terminal-resource correctness in existing Liquid

### Owned outcome and exact boundary

Own logical resource lifetime and removable listener dispatch in `rndr_liquid_ui`; distinguish semantic retirement from offscreen realization. Finish P1 terminal cleanup and permanent regressions without requiring flow. Keep existing parser/source APIs compiling. F2 then replaces the provisional correspondence resolver with the exact key/fallback table. F1 alone is **not** a claim of full identity readiness or permission to release a partial Liquid contract.

Allowed production paths for the future assignment: `crates/rndr_liquid_ui/src/{state.rs,view.rs,timer.rs,feed.rs,host.rs,chart.rs,render.rs,lib.rs}` and a private `lifecycle.rs` module if it reduces shared-table knowledge. Allowed tests: existing `tests/{state.rs,recycle.rs,feeds.rs,timer.rs}` plus focused lifecycle unit tests. A documented narrow need in core `markdown/host.rs` must return to the coordinator for scope review; **do not start by changing core despawn, schedule semantics or MarkdownSlotHost public trait**. Parser/Cargo/SDK/Medousa edits and a new crate are not in F1. UI/theme/editor/native unrelated cleanup is excluded.

### Interface seams to implement (proposed names, no current-API claim)

| Seam | F1 contract | F2 consumer |
| --- | --- | --- |
| `SurfaceIncarnation` and `BlockIncarnation` (crate-private) | Checked never-reused serials; logical surface lease independent of physical Entity. Liveness also checks World registration/entity/component presence. Physical mounts carry references, not disposal authority. | Host-key/fallback resolver returns retained serials, never allocates by current source byte alone. |
| `ReconcilePlan { retained, retired, created, remapped_sources }` (private) | Build then validate then atomically publish all store changes. Current `match_states` parsed-equality correspondence supplies provisional matches, extended to saved/offscreen blocks too; record lookup cannot override that plan. New/unmatched old blocks retire, including changed content at same source byte. Retained duplicates follow characterized current greedy correspondence **only until F2**, not the accepted final policy. | F2 replaces only correspondence policy, keeps retirement/resource transaction. |
| `retire_block` / `retire_surface` / `drain_retirements` (private) | Logical dead marking and lane invalidation immediate; child-first record/listener/timer/feed/chart/nested cleanup, callbacks outside borrows, idempotent, bounded. No callback on Drop. | Used for kind/reset/remove/unmatched cases; source movement is remap, not retirement. |
| Removable subscription (new additive public function/type) | New owned API and explicit unsubscribe; Drop signals a cancelled flag, consumed at drain. Legacy void API remains app-global. ID-snapshot dispatch handles add/remove/self-remove/recursive emit safely. | New handle-bearing envelopes can coexist with old envelopes; no required legacy struct fields. |
| Feed observation context + retire notification (additive host methods) | Context = surface/block/request serial; default delegates to existing load method, retirement default no-op. Context-aware hosts explicitly release local observation handles. Legacy Pending may continue externally; invalidate delivery and report unacknowledged work. | F2 preserves/replaces context by exact feed binding. Never reuse old request on reset. |
| Internal resource snapshot / weak sentinels | Crate-private counters for records, timers, saved/carry, nested logical leases, watches, listeners/captures, chart slots, retirement backlog, local task/observation handles and pending external acknowledgments. Unit tests access it, not a new diagnostics product API. | 100-cycle identity oracle uses same counters and serials. |

F1 must account for every path: `hierarchy::despawn`, `World::despawn_entity`, removing `LiquidView`, removing `MarkdownView`, explicit future dispose, nested root rebuild and whole World destruction. Raw entity despawn does not cascade physical descendants; the new logical owner registry must still retire every descendant owned by that surface and release its resources. It must not recursively destroy unrelated entities attached by an application. Supported raw despawn/component-removal cleanup is recognized at the next drain; all state writes/delivery already validate owner liveness. Removing a component temporarily inside `set_liquid_source`/`push_liquid_str`/reconcile is an internal transaction, **not** terminal authority loss: use a stable separate logical lease/registry transaction marker rather than a `Drop` implementation on `LiquidView` that accidentally retires itself on every edit. Unwinding an incomplete transaction must fail closed and be observable on the next drain.

Logical nested surfaces are children of semantic block/child paths. Physical nested Entity loss during recycle/rebuild detaches a presentation, preserving its logical lease/state. Parent/child semantic retirement releases it. A fresh root at a different Entity attaches to the same retained logical lease only through the validated reconcile plan. Never search dead Entities for old state. For explicit raw top-level removal, retire its full logical subtree regardless of physical hierarchy order.

### F1 execution sequence and compatibility

1. Add failing permanent desired-behavior terminal tests before implementation; retain old recycle/append/timer/feed tests as characterizations. Include actual activation/dispatch, not only direct record writes. Capture the old P1 leak as expected baseline evidence without committing a test that celebrates a leak.
2. Add private serial/membership/reconciliation seam and route **all** mounted/saved/carry/ComponentStore/TimerStore/feed/chart paths through it. Terminal membership cannot depend on whether a reader previously interacted. `chart.rs::{release,reclaim}` stays the authority for chart slots but receives exact ownership retirement, not a separate guessed lifetime.
3. Sweep known terminal owners before timer/feed/result consumers (front of Update), drain pending removals after dispatch callbacks and after PostUpdate view reconciliation. Immediate query/put checks cannot wait for a sweep. Keep absolute timer deadlines and mounted-only feed interval behavior; no extra offscreen polling promise.
4. Add owned removable subscription API, leave legacy global subscriber untouched. Implement serial-ID snapshot dispatch: new handlers next envelope; removed-before-turn skipped; self-remove current call completes; callback disposal stops later surface deliveries; nested emits FIFO. Never restore a detached old vector over new registrations as current `state.rs::emit` does.
5. Add context-aware feed methods with default legacy delegation; exercise retire-before-complete and complete-before-retire. New adapter has explicit local cancellation acknowledgment; old adapter only proves no stale renderer write. This is not a daemon job cancel seam.
6. Update public docs with terminal cleanup, pump point and legacy limitations; compile unchanged legacy subscribe/record/request struct literals. Error-returning new functions expose typed reasons; old void source setters refuse capacity/exhaustion without partial mutation and record bounded diagnostics, not wrap/panic. Existing `put` returns its existing error without creating dead records. No ABI/signature replacement disguised as additive RAII.

**F1 acceptance:** no top-level or nested terminal state/resource/capture remains at defined drain point; dead owner writes rejected immediately; no timer completion or feed commit after logical retirement; all original append/recycle tests pass; no flow/core dependency additions. F1 validates retirement sets and the provisional equality resolver but labels duplicate identity limitations; final compatibility/release acceptance waits for F2.

### Additive public seam specification for F1/F2

The following is **proposed API contract notation, not compiling implementation or API already present**. Opaque handle fields stay private. F1 supplies surface handles and owned subscription/context seams; F2 supplies occurrence keys/source revisions/block handles. Names can receive mechanical Rust review, but changing these semantics returns to contract review.

```rust
// Opaque, Copy handles; no public constructor/deserialization or stored permission.
struct LiquidSurfaceHandle; // World registration + surface serial
struct LiquidBlockHandle;   // surface + block serial
struct LiquidSourceRevision; // checked u64, starts at zero on surface creation
struct LiquidFeedContext;   // block handle + binding/refresh lane generation
struct LiquidInteractionSubscription; // non-Clone, Send Drop-signal only

enum LiquidListenerOwner { World, Surface(LiquidSurfaceHandle) }
fn liquid_surface_handle(world: &World, view: Entity)
    -> Result<LiquidSurfaceHandle, LiquidUpdateError>;
fn subscribe_liquid_interactions_owned(world: &mut World,
    owner: LiquidListenerOwner, handler: impl LiquidInteractionHandler + 'static)
    -> Result<LiquidInteractionSubscription, LiquidSubscribeError>;
fn unsubscribe_liquid_interactions(world: &mut World,
    token: LiquidInteractionSubscription); // idempotent; consumes token

// Vector order is top-level Segment::Block reading order, NOT blocks() index
// (nested parsed blocks also occupy blocks()). None explicitly selects fallback.
struct LiquidKeyMap { keys: Vec<Option<String>> }
fn inspect_liquid_occurrences(source: &str)
    -> Result<Vec<LiquidOccurrenceDescription>, LiquidUpdateError>;
fn set_liquid_source_keyed(world: &mut World, surface: LiquidSurfaceHandle,
    expected: LiquidSourceRevision, source: &str, keys: &LiquidKeyMap)
    -> Result<LiquidSourceRevision, LiquidUpdateError>;
fn liquid_component_handle(world: &World, surface: LiquidSurfaceHandle,
    occurrence: usize) -> Result<LiquidBlockHandle, LiquidUpdateError>;
fn liquid_component_state_for(world: &World, block: LiquidBlockHandle)
    -> Result<Option<LiquidComponentRecord>, LiquidUpdateError>;
fn put_liquid_component_state_for(world: &mut World, block: LiquidBlockHandle,
    expected_state_revision: u64, state: LiquidComponentState)
    -> Result<LiquidComponentRecord, LiquidUpdateError>;
fn reset_liquid_block(world: &mut World, block: LiquidBlockHandle,
    expected_state_revision: u64) -> Result<LiquidBlockHandle, LiquidUpdateError>;
fn reset_liquid_surface(world: &mut World, surface: LiquidSurfaceHandle,
    expected: LiquidSourceRevision) -> Result<LiquidSurfaceHandle, LiquidUpdateError>;

// Additive LiquidHost methods; preserve load_feed(world, request) exactly.
fn load_feed_for(&mut self, world: &mut World,
    context: &LiquidFeedContext, request: &FeedRequest) -> FeedOutcome;
// Default implementation calls self.load_feed(world, request).
fn retire_feed_observation(&mut self, world: &mut World,
    context: &LiquidFeedContext); // default no-op; never backend cancellation
```

`LiquidOccurrenceDescription` reports ordinal/kind, not a stable identity or a guessed original-source offset. Key count must match inspected settled **top-level** occurrences exactly. Retained keyed source replacement (including append) supplies the whole new vector atomically; no "append now, assign identity later" window. Source revision increases once when source or key assignments actually change, not on no-op; block-only state updates do not change source revision. Empty appends are no-ops. `reset_liquid_block` replaces just that block serial/default state and advances source/identity revision once even if bytes are unchanged, preventing an outstanding sidecar update from silently undoing reset. Surface reset preserves current bytes/key labels but creates fresh serials/default state for the whole subtree and starts the new surface source revision at zero; old surface handles are stale. Replacing source authority is explicit reset followed by a new keyed update, not resurrection. Failed preflight changes none of these counters.

Legacy `push_liquid_str` on a keyed surface retains existing assignments and appends **unkeyed** new settled occurrences; callers needing stable keys for new blocks must use the atomic keyed setter. Legacy nonappend `set_liquid_source` explicitly chooses all-unkeyed mode (old keyed occurrences retire); exact no-op and true append preserve existing prefix assignments. Document this at both APIs. Legacy source fields remain current **projected Markdown** positions, distinct from occurrence ordinals and raw source bytes.

`LiquidUpdateError` distinguishes StaleOwner, StaleBlock, StaleRequest, Revision, Kind, InvalidKeys, Capacity and Exhausted (with bounded numeric metadata, no source/secret payload). A keyed creation helper can be expressed as `liquid_view` followed by an immediate keyed transaction before any pump; initial mode rebind is a documented reset, not state migration. `LiquidSubscribeError` returns both refusal and untouched handler for explicit caller cleanup. New deferred feed updates use a separate context-bearing update function that requires matching lane generation **and** state revision; no lane field is retrofitted into legacy put/FeedRequest. New interaction-envelope observation wraps a legacy envelope plus surface/block handle; old handlers still receive the old struct. No exhaustive legacy match or struct literal is broken.

Callback/event capacity must be reserved before an interaction state commit or timer completion is published. A full admitted-envelope queue defers that due timer/interaction or returns explicit Capacity before state mutation; it does not commit then silently lose the authoritative interaction. Drain at most 256 listener callback invocations per pump using persistent dispatch cursor/snapshot; resumed callbacks recheck cancellation/liveness. Bound queued envelopes at 1,024 per World; recursive emission cannot recurse the stack or monopolize a pump. Diagnostics may be dropped only as counted ring overflow, never in place of required retirement. These are design defaults, not measured throughput.

## B. F2 — semantic identity and stale-handle correctness

### Owned outcome and boundary

Depends on accepted F1 serial/retirement/plan seams **and landed F0**. Production paths: the same Liquid modules, principally `state.rs`, `view.rs`, `host.rs`, `render.rs`, `timer.rs`, `feed.rs`, `lib.rs`; private identity module is permissible inside the existing crate. Focused new `tests/identity.rs` and crate-private unit tests supplement existing suites. **No parser grammar extension, new crate, source-editor mutation policy or Medousa adapter**. `rndr_liquid` parsed values/streaming behavior are inputs, not a new document authority.

Implement the entire F0 §7 table as data-driven permanent tests, with and without physical mounts. Add keyed creation/replacement/reset and handle-bearing query/put APIs separately from legacy APIs. Use complete top-level reading-order occurrence/key vectors and an optional host sidecar: no new required field in `LiquidBlock`, public record/envelope, timer record or `FeedRequest`. In the first implementation, mixed keyed/unkeyed occurrences are permitted but never implicitly match across modes. Switching an occurrence's mode creates a new serial (explicit keyed update documents this reset). Keys attach only to settled occurrences; appending uses the current normalized source revision and the new complete occurrence vector (not source offsets). Keyed append must supply new settled occurrences' mapping atomically, not append first then race a second mapping update.

### Exact work order and release conditions

1. Replace greedy duplicate pairing with a single resolver: keyed same-live-key/same-kind, or unkeyed unique whole-value equality in both lists; no-op/append preserve established prefix. Identity comparison uses structural semantic equality, recursively resolving nested parser block-token references, not only raw parsed token indices, hashes/Arc pointers/source indices. Normalize CRLF before occurrence matching exactly as source parsing does. Validate key length/control-free/duplicate occurrence/key-vector length/source revision/counter budget before mutating source.
2. Compute all new serials, retire sets and source remaps in one plan. Preflight state/resource revision increments and queue/admission budgets. An invalid update leaves source, stream parse state, Markdown source, selection, mounted/saved/carry state, timers, feeds, listener delivery, records and revisions byte-for-byte/logically unchanged. Old void source APIs remain usable via unkeyed mode and diagnosed no-op on refusal; new APIs return typed errors.
3. Apply the F0 child-schema rule exactly: ordered full child values; differing child schemas reset index-based state/resources, no fabricated stable child ID or clamp to a different item. Test parent title-only edit separately from child content edit. Same-kind parent serial can survive a child-state reset; state revision and request lane generation distinguish pre-reset writes. Nested logical surfaces follow the same comparison. Future per-child reader-state remapping (including use of already parsed child IDs) requires a new contract, not an undocumented special case.
4. Migrate internal renderer interactions/timer/feed completion envelopes to current serial/revision/lane handles. Write admission checks happen once at the actual mutation owner, after all async work, with no callback between validate and write. Reject already queued old handle, remove/reinsert ABA, same-state-revision ABA, old timer complete and old feed refresh at the same byte. Assert **zero** record/event/revision/history/rebuild effects on rejection. Query/legacy source aliases are metadata only, not mutation capabilities.
5. Retain legacy current-source lookup/put and application-global listeners. Publish the explicit limitation: a delayed legacy positional put can name a different newly occupying block with matching revision; it cannot satisfy the new stale-handle oracle. New deferred callers must migrate to handles before claiming safety. Do not silently convert all consumers or immediately drop legacy subscriptions. Add fixtures that ignore legacy return value and construct old structs exhaustively. No automatic persistence/import of leaked baseline records.
6. Run F1 full regression/100-cycle oracle with F2 cases interleaved. Review release notes for intentional changed/ambiguous-state reset, dead/missing-owner put rejection and stronger no-resurrection behavior. Sender reviews combined result before release/integration; no framework token can compensate for a failed renderer identity test.

**F2 acceptance:** every table row states exact serial, state, revision, timer deadline, feed lane and listener effects; keyed validation is atomic; fallback never guesses ambiguous duplicates; source movement alone causes no state revision increment; retired handles never address new incarnation even at same byte/revision; legacy limitations and migration are explicit. Neither offscreen/appends nor public source compatibility regresses unexpectedly.

## C. Deterministic pump and 100-cycle baseline oracle

This is a future test specification, **not results from this F0 phase**. Reuse core `Sim::{with_seed,advance,run_until_parked,update}` and `rndr_paint::frame` patterns from current `tests/state.rs::boot`. Do not invent a `flush_everything()` API or claim `Sim` drains an OS/network producer. Core `App::update` flips event buffers and runs local executor before Input/Update/PostUpdate/layout. Liquid installs timer/feed in Update and view reconcile in PostUpdate; frame tests warm through layout as existing tests do. A retirement drain alone must not be confused with a full update/render pump.

Define the measurement helper in **test code**, with one documented implementation:

1. Inject due fake producer completions and Drop/remove operations in specified order. Run local executor until parked using manual time; record logical-dead counts immediately before pumping. Before the first update, attempt a new handle put for the dead owner and assert refusal.
2. Perform one normal `App::update` (or `rndr_paint::frame` for mount/layout tests), which includes the new front-Update retirement pass and PostUpdate final drain. Retire callbacks may queue commands/results; do not count this as fully quiet yet.
3. Perform a second normal update/frame with no time advance so event double buffers and callback-queued commands become visible, then executor-until-parked. Measure the complete counter vector and weak captures. If pending cleanup remains because of the explicit 256-callback budget, continue bounded pumps: at most `2 + ceil((admitted_cleanup_count + scheduled_listener_callback_count) / 256) + 2` total from the start (two extra rounds for callback effects). Compute that finite callback-work count from the test's fixed event trace, including known callback-enqueued events; do not omit listener work from a cleanup-only bound. A tiny test budget fixture can lower 256 to prove multi-pump behavior. Compare exact expected remaining count after each pump, not just eventual quiet.
4. At the bound, backlog must be zero and all cooperative tasks acknowledged, otherwise fail with owner/serial/lane diagnostics. No sleeps, network, fresh tasks admitted by an unbounded callback chain, or "retry until green" loop. A deliberately blocked/uncontrolled producer is a separate negative test: zero admitted UI writes is required, but physical quiescence remains `unacknowledged`, not falsely green. Drop fake producer handles and join them within a test timeout in teardown.

**Warm baseline B:** instantiate plugins/theme/viewport and one application-global legacy listener; complete two empty frames. Create/use/remove one temporary surface and drain to initialize retained allocators and caches. Snapshot logical counts after drain; retain a weak sentinel proving the deliberately global subscriber still lives. Reuse that same application for all 100 cycles; creating a fresh World each time would mask leaks. Allocation capacity/RSS is not a logical live count.

**Each of exactly 100 cycles:** create a fresh surface (capture serial); activate accordion/tabs/checklist/selection via actual action dispatch, start recipe timer, attach fake context-aware Pending feed with last-good value, owned listener/drop capture, nested Liquid control and chart; scroll offscreen and back, verify retained state/timer deadline and unchanged semantic serial; append settled and partial fences; alternate unique reorder/prefix, changed same-byte block, explicit key reset, ambiguous duplicate fallback, remove/reinsert, kind change. Assert the F0 expected identity effects. Then on even cycles remove **all** content and verify immediate logical retirement before teardown; on odd cycles keep nonempty content plus live timer/feed/listener/nested state through teardown so destruction is not tested only after an empty-source cleanup. Terminally destroy via `(cycle / 2) % 4` = cascade despawn/raw despawn/remove LiquidView/remove MarkdownView, crossing both even/odd strategies with all four terminal paths, and pump to bound. Drop any externally held token intentionally retained for ABA testing only after checking its dead state; it may retain inert token metadata, never listener/task capture. Try stale put/feed/timer event before and after pump and after recreation. Always assert no backend cancellation command.

**Vector oracle, every cycle not merely cycle 100:** semantic block/component records, timer records, logical surface/nested leases, saved/carry entries, live mounted-owned roots, chart slots, feed watches/observation handles, owned listeners/cleanup captures, pending retirements and cooperative local tasks each equal **B**. Separately count physical projection entities intentionally outside raw-despawn cascade; F1 must remove owned projection residue, not just registry entries. App-global listener count remains B and its callback still works. Each retired capture's Weak fails to upgrade; drop counter increments exactly once; parent cleanup observes dead/attempted child cleanup in specified order. High-water marks and elapsed test time are reported as measurements, not a performance guarantee. Monotonic diagnostic counters (stale rejects/cycles) can increase while bounded ring size/correctness counts stay bounded. Explicitly drain retained test logs before comparing memory. Feed/worker external pending acknowledgments are a separate vector and must be zero in cooperative baseline tests.

## D. Mandatory races, reentrancy and negative tests

Use fixed Sim seeds `1, 7, 42` where runnable work order matters, plus explicit barriers/fake queues for every ordering below. Manual time tests deadline-minus-one/exact deadline/after retirement; never schedule race assertions using wall-clock sleeps. Existing feed/runtime tests remain necessary but do not prove consumer admission.

| Boundary | Interleavings and exact assertions |
| --- | --- |
| Publication vs retirement | Complete before cancel; cancel before complete; event published before retirement but consumed after; dispose/recreate at same ECS index/source/revision; same-live-owner lane replace; duplicate completion. Old handles produce zero writes/events/rebuilds/revision increments. The correct independent lane remains deliverable. |
| Terminal paths | Explicit dispose then despawn and reverse; repeated remove; raw despawn and stale Entity; remove authority during callback; nested parent reset/recycle; World drop with pending work. Cleanup once, no dead-component insertions, no resurrection, no offscreen terminal misclassification. |
| Dispatch mutation | Add during callback survives for next envelope; remove not-yet-called handler skips it; self-remove finishes current callback only; callback disposes owner; recursive emit FIFO; add/remove same frame; token dropped from another thread signals only. Legacy subscriber stays until World drop. No overwritten vector, lock held across callback or double invocation. |
| Callback failure | Cleanup Err; explicit cleanup/listener panic under unwind; later siblings/parent cleanup still attempted, outcome records error; registration into dead owner returned to caller; all capacity failures atomic. For destructor double-panic/abort, document unsupported recovery rather than fake catch-unwind safety. |
| Guard safety (future F3–F5) | Cancel preserves bytes/revision/history/selection/resources; save result for old dirty revision or old transition rejects; edit/IME/reload/programmatic completion tries to commit under freeze; save needs its own live lane; stale/duplicate/foreign choices; all-world participant added while native Ready; failed/panicked/lost save stays protected, never silently closes. |
| Exhaustion/capacity | Inject max-1 counters, small scope/lane/listener/queue/history limits; refuse without partial source/revision mutation/cancel of valid prior work. Disposal must progress with saturated admission queue; budgeted cleanup completes at measured pump bound. Diagnostics overflow does not drop cleanup or bless decisions. |
| Identity | Every F0 table row mounted/offscreen; stable key + same kind title-only edit vs child schema edit; keyed/unkeyed mode switch; reordered duplicates; full remove then same-key reinsert; old token revision zero vs new zero; no-op all duplicates; incomplete fence append/CRLF normalization; wrong sidecar occurrence/source revision and duplicate keys reject entire transaction. |

Permanent tests should be named for expected behavior, not ephemeral P1/P2 probes. Do not add a runtime implementation under a test feature as a shortcut in this contract phase.

## E. Four real workflow proof oracles and prerequisites

All four are **future F6 evidence**, not already run. Reference exact Medousa commit/clean state in F0 §9. Use actual `apps/rndr-home/src/{main.rs,lib.rs,backend.rs}` model/writer and existing `backend_tests.rs::fixture` SDK loopback harness when authorized. A fake IO boundary can control receipts/timing but cannot stand in for a second chat/editor/approval model. Native live checks remain separate from headless proof. Preserve local credential loading and no remote/inference fallback; tests must not require real secrets or paid turns.

### 1. A history load completes after switching to B

**Prerequisites:** separately authorized Medousa work; actual retained multi-chat/tab model and per-entry lanes. Current preview is a single Chat and blocks navigation while busy/loading: the requested concurrent switch proof cannot be claimed by today’s UI. A current generation-filter unit test is only partial evidence.

Drive actual UI writer to open A, hold SDK history between active/history reads, activate B through real navigation, deliver B then A and inverse, and consume queued A after switch. Close/reopen A, replace A load in same live entry and repeat. Log `(authority, session, entry incarnation, lane generation, turn/seq)` at commit (no secret/prose payloads). **Oracle:** B's bytes/turn list/tool/approval projection/error/revision never changes from A; retained A updates only under declared live policy; closed A rejects, new A does not absorb old result. Assert exact zero wrong-target writes, not merely active label B. Keep bracketed non-atomic history retry/redaction behavior; no auto resubmission on unknown outcome.

### 2. Close a streaming entry; keep backend turn independent

**Prerequisites:** actual closeable retained tab, F1/F2 host context integration and ownership adapter. Preview currently only aborts followers on navigation; no semantic tab-dispose adapter exists. Existing SDK cancel test proves explicit endpoint wiring, not UI disposal behavior.

Start an SDK-admitted turn through real backend worker (loopback fixture), hold/queue stream text/tool/approval envelopes, close its UI entry, then release events and reconnect/terminal replay in all orders. Track follower handle Drop/abort acknowledgment, core feed lease, Liquid counters and request/stream sequence. **Oracle:** owned UI resources equal warmed baseline after pump; no late UI projection writes; unrelated tab/catalog observations survive; server fixture records **zero** cancellation/approval POSTs on close. Native active-turn record remains pending/completes by backend policy rather than being declared stopped by UI. In a separate explicit-cancel test, click cancel and assert exactly the session-targeted SDK cancellation POST and reconciled outcome, with unknown/race reported honestly (there is no exact-turn CAS in that wrapper). Approval request IDs remain pending until real host-authorized resolution; a close confirmation/flow token is not permission.

### 3. Close an unsaved shared document

**Prerequisites:** separately authorized real document model/editor/save adapter and host freeze admission; preview has no unsaved document manager or save/guard UI. Do not substitute a counter demo and call it Medousa proof. When the real host document surface is selected, record its source vs structured authority and all writer paths, document incarnation, dirty revision, selection and undo/history snapshot before testing.

Open two presentations of the same real model in two Worlds. Cancel one entry close; then close it successfully, keeping the other lease/document live. Request document/window quit, cancel, then retry with save/discard, delayed/failing save and external edit/IME/reload/programmatic mutation attempts. **Oracle:** cancel preserves exact bytes/revision/history/selections and resource set; freezing rejects all guard-relevant writes without changing them; save completion protects exactly the snapshotted revision; all affected leases validate; native Ready remains frozen until runner destruction; successful close releases UI once. New window/lease during quit invalidates/restarts or refuses admission. Pending approval/SDK operation remains independently governed. A multiwindow headless adapter pass does not certify OS quit/IME behavior.

### 4. Repeated real chat Liquid surfaces

**Prerequisites:** F1/F2 completed and separately authorized Medousa LiquidHost/stable-key/retirement adapters. Current preview renders Markdown transcript and has no Liquid dependency/host or authoritative host stable-key table; those hooks are not inferred from the existence of `rndr-home`.

Use actual chat segment/turn IDs as **inputs to a host-side identity decision**, scoped by actual session/surface; document how the host assigns a block key within editable/streamed segments. Do not use source index or segment prose as durable identity by accident. Drive 100 real chat navigation/mount/remove/reset cycles through actual UI/SDK worker with fake feed/timer IO. **Oracle:** run the complete C vector and F0 table, preserve offscreen/append state, reject retired segment/block updates, listener capture drops and cooperative observations return to baseline. Track any intentional global SDK observer separately. Release/persistence of backend requests is not inferred from zero Liquid records. Repeat a bounded native smoke run with exact OS/display/input details before claiming everyday workflow qualification.

## F. Verification commands and evidence ledger

### For future F1/F2 implementation (names only where tests already exist)

```sh
# Inspect free disk and current target/cache size first; use a bounded worktree-local target.
# CARGO_BUILD_JOBS=2 and CARGO_INCREMENTAL=0 bound build concurrency/cache growth.
cargo test --locked --offline -p rndr_liquid_ui --test state --test recycle --test feeds --test timer
cargo test --locked --offline -p rndr_core --test runtime --test feed
cargo test --locked --offline -p rndr_core --lib close::tests
# Then exact new lifecycle/identity unit+integration tests, and facade/feature-off/API
# compatibility checks selected from CONTRIBUTING.md and CI. No invented passing names.
git diff --check
```

Inspect resolved features/dependency changes with Cargo metadata/feature checks before calling optionality proved. Do not silently update locks, write a shared cache outside the admitted root, or run arbitrary broad builds. If locked/offline artifacts are unavailable, report the missing package/artifact rather than fetching or claiming tests passed. No Medousa build or lock update occurs under an rndr-only implementation slice.

### This F0 phase's actual evidence

- Read both complete baseline documents (analysis 208 lines, epic 388 lines), then source paths/symbols in F0 §9. The source maps are inspection evidence, not a native run or backend policy audit.
- `git rev-parse HEAD`, `git branch --show-current`, `git status --short` confirmed assigned isolated rndr baseline/branch/clean start. Read-only equivalents confirmed the exact clean Medousa reference HEAD/branch. Status is rechecked before handback; any concurrent reference change must be reported, not reset.
- `df -h .`: approximately **19 GiB available**, Data volume 91% used. No worktree-local target directory existed at inspection. No build/probe needed for documentation decisions; no shared target/cache was modified and no dependency download took place.
- No disposable runtime probe added or executed in F0. P1/P2 and the 41 test count remain **historical analysis receipts**, not re-run or new contract implementation evidence. No production changes or new framework package were made.
- Bounded Python read-only validation passed: **4 documents**, balanced code fences/final newlines, **19 local links**, **21 rndr + 10 Medousa reference files and 101 exact symbol/contract anchors**. It also confirmed that the complete changed-file set is exactly these four design documents. `git diff --check` passed. These are document/source-anchor checks, not runtime tests; final diff/status/reference checks and commit are reported in handback. The future 100-cycle, race and four application proofs above remain **unrun prerequisites**, not passed gates.

## G. Remaining gates, not unresolved design alternatives

1. Sender independent review and native integration of F0 docs (current worker commit is not either).
2. Separate F1 implementation grant, source refresh, complete resource/listener tests, then F2 grant/combined identity tests and compatibility release review. No Liquid code starts in this phase.
3. F3/F4 optional ownership implementation and F5 host-authoritative freeze/native-close adapters with affected writer tests; no claim those hooks are present today.
4. Separate Medousa authority to implement the missing retained-tab/document/Liquid/approval adapters and obtain exact locked/offline build evidence. Four proofs are blocked on these prerequisites, not on a missing reference checkout.
5. Host-authoritative cancellation/approval semantics remain native; a stricter exact-turn cancel API would require its own backend scope, not an invented client capability. Native keyboard/IME/accessibility/DPI/multiwindow and rich Live/canvas/artifact tracks stay open. No migration/full replacement readiness from F0 alone.

## H. F1 baseline investigation — incomplete, scope review required

This is a **new baseline receipt**, not a change to the historical F0 results
above and not an F1 implementation/acceptance claim. F0 is accepted/integrated
at the assigned baseline `f2919a1692d0bb7e0fba2e26976a1631cf4c637f`.
Investigation date: 2026-11-11 UTC; isolated undertaking
`work-95d0c986e14b6e17fb82f4b5950968b7`. No production source was changed.

Three permanent desired-behavior regressions in `tests/state.rs` fail against
that production baseline:

- `terminal_owner_rejects_writes_before_the_next_pump`: actual accordion
  activation, then raw owner despawn; the immediate positional put incorrectly
  succeeds before any pump.
- `unmatched_same_source_content_does_not_inherit_old_record`: changed parsed
  accordion content at the same projected source position incorrectly inherits
  the old open state. This tests F1's provisional parsed-equality retirement,
  **not** F2 keyed/final duplicate identity.
- `terminal_core_host_registration_releases_capture_at_cleanup_pump`: a focused
  host-registration sentinel, **not the full section C resource vector**.
  Each Sim seed `1, 7, 42` uses one World for 100 cycles, alternates empty and
  nonempty teardown, crosses cascade/raw/LiquidView/MarkdownView removal, and
  pumps four times per cycle. Each seed retains **100 captures**, with measured
  high-water **100**, instead of baseline zero. All captures release on World
  drop. The focused state suite took **31.30 s** with output enabled; this is a
  measurement, not a performance qualification or proof of cooperative task
  retirement. The sentinel replaces the per-view hook solely to observe core
  table retention; it does not claim Liquid's current stateless hook has a
  capture or that application-global listeners should be removed.

**Required scope review:** `rndr_core/src/markdown/host.rs::MarkdownSlotHosts`
contains a private per-view host map. `set_markdown_slot_host` only
inserts/replaces; `notify` temporarily removes a callback and reinserts it.
There is no scoped removal API or terminal-view sweep. Liquid installs one
entry per surface, so Liquid-only code cannot remove its terminal entries
without also discarding unrelated application hosts. The worker reported this
source evidence and failing sentinel to the sender **before any core edit**.
A narrow candidate is an additive removal method on the already-exported
`MarkdownSlotHosts`, returning the removed host so it can be dropped outside a
World borrow. Its callback-time semantics require review; the public
`MarkdownSlotHost` trait, schedules and core despawn behavior need not change.
No scope expansion or core implementation is claimed by this receipt.

Commands used `export PATH=$HOME/.cargo/bin:$PATH`; final verification used
`CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0`:

- `cargo test -p rndr_liquid_ui --test state --locked --offline -- --nocapture`:
  **2 passed, 3 failed**, exit 101, measurements above.
- `cargo test --locked --offline -p rndr_liquid_ui`: exit 101 on those same
  three desired-behavior failures. Cargo stops after that integration suite.
- To separately establish all unchanged existing coverage,
  `cargo test -p rndr_liquid_ui --locked --offline -- --skip terminal_owner_rejects_writes_before_the_next_pump --skip unmatched_same_source_content_does_not_inherit_old_record --skip terminal_core_host_registration_releases_capture_at_cleanup_pump`:
  **29 passed** including doc tests; **2 existing opt-in probes ignored**;
  exactly the three new regressions filtered. This is a baseline-only result,
  not a green F1 suite.
- `cargo test --locked --offline -p rndr_core --test runtime --test feed`:
  **26 passed**.
- `cargo test --locked --offline -p rndr_core --lib close::tests`:
  **3 passed**, 134 unrelated unit tests filtered.

All F1 production work, the complete resource-vector oracle, listener and feed
retirement races/capacity/compatibility acceptance remain **incomplete**.
F2 remains deferred; this branch is not ready for integration.

**Owner-supplied reference evidence (not this worker's repository inspection):**
Medousa reference HEAD `effb18c2836df4ce2e5e88cd46876c685b0bb318` was refreshed
clean on November 11. The preview has no Liquid/flow dependency or adapter.
Its retained core thread feed is not a Liquid feed. Follower abort/reconnect
and stream Drop retire observation only; explicit `Command::Cancel` separately
calls session-targeted `cancel_active_turn`, not exact-turn CAS. Closing UI
must not call that endpoint or resolve pending permission/secret/budget requests.
No Medousa checkout was accessed, changed, built or tested in this investigation.

## I. F1 implementation continuation — incomplete, blocked physical-ownership gate

November 11 continuation after proof-only `6238aa0`. **This is a substantive
implementation draft, not completed F1 and not ready for integration.** Section
H remains the original failing-baseline evidence; none of its results have been
rewritten as implementation acceptance. F2 keys/resolver remain deferred.

### Implemented draft boundaries

- Core changes remain exclusively in `crates/rndr_core/src/markdown/host.rs`:
  additive `MarkdownSlotHosts::remove(view)` returns the detached host for drop
  outside the resource borrow. Scoped active-dispatch flags suppress extracted
  callback restoration after removal, preserve callback-installed replacements
  (including nested dispatch), and remove bookkeeping on ordinary return and
  unwind. No trait, scheduler, generic despawn, export, or other core file change.
  Focused tests exercise 100 iterations per removal/replacement/raw/panic/
  Markdown-component-loss/cascade path, checking drop-once, unrelated host
  preservation and zero accumulated active-frame bookkeeping.
- Liquid's new private `lifecycle` module supplies process-local checked serials,
  logical surface/block occurrence membership, authority-component checks,
  guarded internal component extraction, provisional parsed-equality retirement,
  parked nested surfaces, and tagged physical projection bookkeeping. Component
  records and timers use logical membership rather than `(Entity, byte)` as their
  backing identity. Changed parsed content retires the old occurrence; unchanged
  saved/mounted occurrences share a plan. No durable keys or F2 resolver claim.
- Additive owned interaction subscriptions use removable atomic signal tokens,
  surface/World owners, ID snapshots, FIFO queued envelopes, persistent dispatch
  cursors, callback budgets, cancellation checks before each invocation, and
  panic isolation. The legacy global subscription and exhaustive envelope/record
  struct shapes remain unchanged. A retained dead token holds inert signal
  metadata, not its listener capture.
- Additive `LiquidFeedContext`, `load_feed_for`, retirement notification,
  acknowledgment and context-bound result publication distinguish current
  observations from retired requests. Deferred results recheck logical owner,
  exact request, state revision and emission capacity before publication; they
  rebuild the live projection. Notifications wait until an extracted host/source
  transaction returns and share the callback budget. Legacy hosts retain the
  old default loader behavior; unacknowledged Pending observations are counted
  separately and are **not** declared quiescent. Retirement never issues a
  durable backend cancel or resolves an application approval.
- `LiquidLimits` configures positive renderer budgets (4096 logical leases,
  32 observation lanes, 256 owned listeners per owner, 1024 envelopes, 256
  callbacks per pump, 512 diagnostic entries by default). Source replacement
  stages a cloned stream before publishing membership/source, so refusal does
  not cancel the previous valid source. This introduces an unqualified append
  cloning cost; no performance equivalence claim is made. Exhaustion/capacity
  primitives and listener/feed refusal ordering have focused tests, not a claim
  that the entire original F0/F1 capacity/exhaustion matrix is finished.

### Concrete blocking evidence and requested scope decision

The expanded oracle counts **actual live Layout entities**, in addition to
Liquid registry tags. On **each seed 1, 7, 42**, the first raw-owner removal
(zero-based **cycle 2**, even/empty-source strategy) leaves **109 Layout entities
versus warm baseline zero**. All other currently instrumented vector entries
return to baseline, including component/timer records, logical surfaces/blocks/
nested leases, saved/carry/mounted entries, chart slots, watches/observations,
owned listeners, queued delivery/retirement work, cooperative local handles,
external pending acknowledgments and tagged owned projection entities. The one
legacy global listener intentionally remains alive. Owned listener weak captures
are checked dead before reporting this failure.

This is **not a passing 100-cycle oracle**: each seed deliberately stops at its
first counter-vector failure. The three prefixes took approximately **0.890 s,
0.845 s, 0.839 s**; the failing test took **2.58 s**. The observed prefix
high-water vector was `[6,1,6,8,5,1,0,8,1,1,1,1,1,0,1,1,0,90,204]`, in the order
below. These are partial-prefix measurements, not throughput guarantees or a
completed run. An earlier tag-only attempt was interrupted and is not acceptance
evidence.

Vector order: component records; timers; logical surfaces; block leases; nested
leases; saved entries; carry entries; mounted roots; chart slots; feed watches;
active observations; owned listeners; global listeners; queued envelopes;
retirement notifications; cooperative local handles; pending external acks;
tagged physical entities; **all actual Layout entities**. Parked snapshots are
included in saved/carry accounting. The fixture removes any caller-owned shell
entity **raw**, never with a test-side child cascade that could hide residue.

Core `MarkdownView` exposes source/blocks/source-offset and editing/follow/
selection operations, but its realized native text/spacer ownership (`slots`,
`lead`, `trail_spacer`, private `Piece` implementation) is not publicly available.
`World::despawn_entity` intentionally does not cascade. Liquid can remove its
known block projections, but cannot safely distinguish those native Markdown
projections from arbitrary application-attached children by sweeping the view's
children. Such a sweep would violate the explicit unrelated-entity boundary.

The sender was asked for a **narrow read-only native Markdown projection
ownership accessor/helper**, implemented in `markdown/host.rs` on an existing
exported type if appropriate, without scheduler/despawn/trait changes or a new
export. That is beyond the exact registration-removal permission supplied for
this continuation; it has **not** been implemented without a decision.

Separately, the unmodified existing integration test
`reader_state_survives_appends_and_unchanged_blocks` changes an accordion title
from `FAQ` to `Questions` but expects its prior open state. That expectation
conflicts with F1's changed-parsed-content retirement gate. A narrow permission
request to adjust that assertion/comment in otherwise out-of-scope
`tests/render.rs` is pending. A temporary diagnostic assertion edit was reverted;
that file is **unchanged** in this draft. Unrelated carousel/slides/checklist/tabs
retention expectations were not weakened.

### Exact verification (locked/offline)

Environment: `PATH=$HOME/.cargo/bin:$PATH`, `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`.

- `cargo test --locked --offline -p rndr_liquid_ui`: **exit 101**, **9 unit tests
  passed, 1 full-vector oracle failed**; Cargo stops before integration suites.
- `cargo test --locked --offline -p rndr_liquid_ui --lib full_same_world -- --nocapture`:
  **exit 101**, the seeded prefix failures above. All three seeds were attempted;
  no claim of 100 completed cycles per seed.
- `cargo test --locked --offline -p rndr_liquid_ui -- --skip full_same_world_100_cycle_resource_oracle_seeds_1_7_42`:
  **exit 101** on the unchanged title-edit retention assertion in `tests/render.rs`.
- `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --skip full_same_world_100_cycle_resource_oracle_seeds_1_7_42 --skip reader_state_survives_appends_and_unchanged_blocks`:
  **55 passed**, including doc tests, **2 existing opt-in probes ignored**,
  exactly the two named tests filtered. This is explicitly filtered coverage,
  **not green F1 acceptance**. This includes rndr's checked-in grammar/golden
  fixtures; it does not build or test a Medousa checkout, worker or adapter.
- `cargo test --locked --offline -p rndr_core --lib markdown::host`:
  **3 passed**, 135 unrelated unit tests filtered.
- `cargo test --locked --offline -p rndr_core --test runtime --test feed`:
  **26 passed**.
- `cargo test --locked --offline -p rndr_core --lib close::tests`:
  **3 passed**, 135 unrelated unit tests filtered.
- `cargo fmt --all -- --check` and `git diff --check`: passed at handback.

Remaining work includes the ownership seam and full physical cleanup without
unrelated-child collateral, completion of every 100-cycle trace/identity case
and its finite pump/order assertions, exhaustive source/revision/timer capacity
and exhaustion/refusal fixtures, complete callback-time disposal/transaction
qualification and legacy exhaustive-construction fixtures. Current tagging and
rebuild/despawn behavior is **not certified against arbitrary application
attachments**. Only focused behavior supported by the receipts above is claimed.
No native interaction/visual qualification, performance probe, Medousa adapter,
Medousa build/test, push, publication, deployment or merge occurred. Sender
independent review and additional authorized work remain necessary.

## J. Resumed F1 candidate after explicit native-ownership permission

This section preserves and supersedes the **current failure measurements** in
§I; it does not erase the failed drafts, certify them retrospectively, or imply
sender acceptance. Work continues directly from
`e9b0ddce07d98b082ad0e72b87cec9b9f626dcd8`. No history was rewritten. The sender
had requested changes, not accepted the 2710-line draft. F2 remains excluded.
The implementation below is committed for full independent review, **not
self-accepted or integrated**. The literal caller-cascade attachment boundary
under J.2 still requires sender interpretation; do not silently count a test of
reparented-out survival as proof that attached descendants survive core cascade.

### J.1 Changes and ownership proof

- Added `MarkdownSlotHosts::native_projections(&World, Entity) -> Vec<Entity>`.
  Core tags exact native text/rule/quote/list/checkbox implementation nodes at
  callback-free construction, plus window spacers. The tag contains only the
  generational owner entity. It survives loss of `MarkdownView` and raw owner
  despawn, without retaining source, a dead-view table, or an `Arc` capture.
  It excludes the view, host-slot roots, and application content. Reparenting a
  native projection does not transfer its creation owner. Lookup is read-only,
  unordered, and O(all live native projection tags in the World), including its
  temporary enumeration allocation. Entity generations prevent ABA. Metadata is
  released with the exact projection; no scheduler or generic despawn changes.
- Terminal Liquid cleanup removes both Liquid and Markdown authority, including
  component-loss paths: leaving Markdown authority alive was found to recreate
  native nodes on the next sync. It enumerates and raw-removes only exact owned
  projections; native ownership therefore remains discoverable even if the
  owning component/entity is already gone. Caller-owned view shells remain
  caller-owned. Per-view core host removal remains callback-safe.
- Replaced the WIP's repeated arbitrary-subtree ownership inference. Pure
  renderer/UI constructor sites claim their newly created trees immediately,
  before host callbacks; later application attachments are not claimed. Nested
  owners are parked before recycle/rebuild. Presentation cleanup and terminal
  cleanup raw-remove the admitted exact set, detach surviving application
  children, and tidy hierarchy links, rather than cascade through attachments.
  Dead physical-reference IDs are pruned from live surface bookkeeping during
  drain; rebuilds must not become an Entity-ID history.
- The narrowly authorized `FAQ -> Questions` regression now asserts that the
  old body entity died, old open state did not carry, and the replacement starts
  closed. Existing append, carousel/slides/checklist/tabs assertions remain.
  Correspondence remains the F1 whole-parsed-value provisional rule, not F2 keys.
- Source publication now refuses nested extraction transactions before mutation.
  Commit cannot revive a poisoned owner. An abandoned/unwound transaction fails
  closed before the next pump; its next drain records a bounded payload-free
  panic diagnostic. Listener and retirement callback panics are likewise
  observable without suppressing later independent cleanup. Core host and
  Liquid host extraction/restoration paths retain their unwind behavior.
- Feed retirement paths preserve tree creation order (child before parent,
  siblings by creation) and reverse request-registration order within one
  surface. Depth-only sorting and ascending request order were insufficient.
  Contexts are invalidated as a batch before callbacks; the shared per-pump
  callback budget is not reset by reentrant drains. Unacknowledged local work is
  bounded/tracked, never silently reported as released.
- Corrected plugin construction to actually install the validated configured
  `LiquidLimits`. Scope admission failure no longer installs a useless per-view
  core host before discovering that it cannot register the Liquid owner.

### J.2 Application attachment safety and the core cascade boundary

New tests prove survival of application subtrees after raw view loss,
`LiquidView` removal and `MarkdownView` removal, including attachments under
native nodes of nested Liquid views, unrelated Markdown views attached beneath
owned nodes, reparenting, and application nodes attached by a synchronous host
load callback. The cleanup unparents surviving attachments; it does not destroy
or relabel them. A separate test proves survival of an attachment reparented
outside the requested cascade subtree and preservation of its new parent.
The focused core test also checks native-tag ownership after authority loss and
reparenting, exact release, unrelated-view isolation, and generational identity.

**Non-negotiable limitation/source evidence:** `rndr_core/src/hierarchy.rs`,
`despawn` (lines 56–77 at this revision), recursively despawns every still-listed
`Children` descendant before returning. If an application itself requests that
cascade while an application child remains attached, that child is already dead
before Liquid's drain can run. The new cascade test explicitly observes that
pre-drain deletion; it does not claim survival. `MarkdownSlot`'s existing public
contract likewise says descendants still attached to its root die at detach.
Liquid now detaches unrelated attachments before its own detach/recycle paths.
This work does not rewrite that generic core/caller contract or resurrect dead
entities. Such a literal all-attached-children-survive-caller-cascade requirement
cannot be implemented by the authorized read-only ownership seam while also
preserving the explicit prohibition on World/despawn/scheduler semantic changes.
The sender was notified with exact source evidence; no further permission or
clarification had arrived when this evidence was written. **Full F1 acceptance
must resolve that interpretation, not weaken the gate by omission.**

### J.3 Unfiltered physical oracle: all three complete 100-cycle runs

The same World is warmed once per seed and reused for all 100 cycles. The
existing 19-entry vector is unchanged, including **actual all-World `Layout`
entities**, not merely tagged Liquid nodes. Vector order:

1. component records; 2. timer records; 3. logical surfaces; 4. logical blocks;
5. nested logical links; 6. saved block maps; 7. carry entries;
8. mounted entries; 9. chart renderers; 10. feed watches; 11. feed observations;
12. owned listeners; 13. legacy/global listeners; 14. queued deliveries;
15. retirement notifications; 16. cooperative host-local captures/tasks;
17. unacknowledged observations; 18. tagged Liquid physical projections;
19. actual native/renderer `Layout` entities (including Markdown projections).

All three warm baselines are:

```text
B = [0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0]
```

All three **sampled trace high-water** vectors are:

```text
H = [6,1,6,8,5,1,0,8,1,1,1,1,1,0,1,1,0,90,204]
```

These are maxima of the test's explicit snapshots, not allocator-instrumented
peaks at every instruction. All completed cycles assert `A == B` after exactly
four teardown pumps. The test alternates empty/nonempty teardown and exercises
cascade/raw/component removal, append/prefix preservation, changed content,
nested content, viewport recycle, controls/timer/feed/chart, stale operations,
owned capture drops, local host retirement/ack and independent global lifetime.
No test-side cascade is used to sweep residual renderer descendants away before
counting. A caller-owned shell is raw-removed as the caller's explicit teardown;
that operation cannot sweep away descendant residue. Global capture is tested
through World drop separately. The console retains the historical
`attempted-prefix` label so a failed future run cannot falsely advertise 100
cycles; in this final run **all 100 iterations per seed completed and passed**.

| Seed | Completed cycles | Observed elapsed |
| --- | ---: | ---: |
| 1 | 100 | 28.738449334 s |
| 7 | 100 | 29.166971625 s |
| 42 | 100 | 29.136367167 s |

The 17-test UI unit suite containing the oracle completed in **87.04 s**. This
is headless seeded behavior, not a native interaction, visual, DPI or throughput
certification.

### J.4 Additional qualification and stream-clone cost

New/focused tests cover exact two-callback retirement budgeting (five
notifications complete in three pumps; repeated same-pump drains make no
progress), tree postorder/reverse-registration callbacks and idempotence,
transaction reentry/poison/unwind/observable fail-closed cleanup, source capacity
refusal preserving stream/block-Arc/source/key, diagnostic-ring overflow,
component/timer revision exhaustion with unchanged records and no new events,
and exhaustive v1 record/request/envelope construction plus a context-free
legacy host implementation. Existing unit tests continue to cover subscription
capacity/ID exhaustion/untouched-handler refusal, self/other token cancellation,
registration during dispatch, FIFO/reentrant queue/callback budgeting, listener
panic isolation, feed stale generation/revision/refusal and retirement panic
isolation/unacknowledged legacy-host tracking. No global serial allocator is
reset to fabricate an exhaustion test; isolated local counters are used where
applicable. Full render/recycle/feed/timer/stream/grammar/golden behavior remains
in the unfiltered test runs below.

**Cloning is intentionally qualified, not called free.** Source update stages
one cloned `LiquidStream` before membership admission so refusal cannot corrupt
the published stream. If `S_i` denotes the owned source, Markdown and parsed
block payload size at chunk `i`, this adds O(`S_i`) copying per candidate, in
addition to tail parsing, source reconciliation and block-array publication.
At most one source candidate per surface is extracted; reentrant source updates
are refused. The candidate is synchronously committed or dropped, not queued or
retained as history. Existing snapshots/carry/state and the old/new published
block arrays are additional live storage; neither the test nor this analysis
pretends that source bytes alone measure total heap use. Source/Markdown/block
payloads are application document data, not bounded by the scope-count limit.
There is no claim of a constant-byte or constant-time append, no source-size
cap introduced, and for many small chunks total added copying can be quadratic
in final source size. Chunk coalescing by the host can reduce that cost; it is
not required for correctness or presented as a measured adapter optimization.

The structural regression stages/drops one candidate per append for 32 appends
to a large document, proves the old stream stays unchanged while staged, and
compares each published Markdown/block result with a fresh parse. Recorded
measurements: final source **246,314 bytes**, **7,873,600 cumulative source bytes
copied**, maximum old-plus-candidate source bytes **492,612** (at most twice the
final source bytes). These numbers exclude other strings, parsed props, maps,
allocators and rendering. This supplies finite-document structural boundedness
and semantic evidence; it is **not** a latency/RSS benchmark or a constant-cost
performance promise. Optimizing this tradeoff further would require a separate
parser/checkpoint design review rather than silently weakening atomic refusal.

### J.5 Exact final commands/results and limitations

Environment for all Cargo commands below:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
```

- `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture`
  — **exit 0, 69 passed, 2 pre-existing opt-in performance probes ignored,
  0 filtered** across unit/integration/doc suites. Includes all 300 oracle
  cycles and the changed-title regression; no `--skip` flags.
- `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown`
  — **exit 0, 179 passed, 1 pre-existing ignored unit test**. Includes all core
  unit tests (close and host unwind/drop cases included), runtime, feed and the
  full applicable Markdown integration target.
- `cargo test --locked --offline -p rndr_core --lib markdown::host`
  — **exit 0, 4 passed**, 135 other core unit tests filtered by this focused
  command; the full core command above supplies the unfiltered unit coverage.
- `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings`
  — **exit 0**; relevant libraries and tests, no lint suppression added for the
  new ownership/transaction logic.
- `cargo fmt --all -- --check` and `git diff --check` — final whitespace and
  formatting checks both **exit 0** after this section was written.

Intermediate honest failures included fixture compilation/incorrect fixture
source offsets, an initially attempted nonexistent `markdown_host` core
target (exit 101; actual applicable integration target is `markdown`), two strict
Clippy findings, and the new aborted-transaction regression exposing an
incorrect `== 2` terminal test when the observable abort flag became `3`.
These were fixed and the unfiltered final suite rerun; the old failures in §H/§I
remain preserved. No failing test was deleted, ignored or filtered to manufacture
a green final run.

Remaining review limitation: sender independent full-diff review, acceptance and
resolution of J.2's literal caller-cascade requirement remain outstanding. Do
not infer Medousa readiness, native UI certification, or broad `rndr-flow`
implementation from these receipts. No Medousa checkout/edit/build/test, adapter,
durable work cancellation, approval resolution, backend task cancellation, F2
implementation, new dependency, core scheduler/World/despawn change, push,
publication, deployment or merge occurred.

## K. Corrections after independent review of d2edd1ad

This section supersedes **candidate qualification**, not the historical receipts
in §I/§J and not the F0/F1 acceptance contract. The sender accepted the independent
**review deliverable only** and requested project changes on the sealed
`d2edd1ad1a4ac353bc5a1cbb30346905eb0e3946` evidence. That implementation was not
accepted. Before editing, `git status --short` was empty, HEAD was exactly that
candidate, and local `main` was
`f2919a1692d0bb7e0fba2e26976a1631cf4c637f`. No main/shared-reference movement, native
approval, integration, push or publication is part of this correction.

### K.1 Independently reproduced defects and scoped implementation

1. **Independent surface authority lost on parent collapse (P1).** Physical
   nesting did not establish logical parentage, but `park_under` removed every
   physically nested `LiquidView`. Parking now follows registered parent keys
   back to the block being recycled. An unrelated live surface is a traversal
   boundary, including its entire renderer subtree; `clear_presentation` uses
   the same boundary. This preserves independent handles, state, native
   projections, owned listeners and feed observations while genuinely owned
   logical descendants still park/release normally. Permanent controls include
   collapse/reopen, source replacement, raw parent despawn, parent `LiquidView`
   removal, and parent viewport recycling, with unrelated wrapper/nested Liquid
   attachments. Physical block roots also retain the existing F1 lease that
   created them: membership indices can move before old projection cleanup.
   This is a cache of an already-issued lease, not a new F2 identity policy.
   The existing 300-cycle ownership oracle remains unfiltered.
2. **Native tab commits without envelope admission (P1).** `sync_tabs` now checks
   queue admission and revision exhaustion **before** mutating `BlockState` or
   its authoritative revision. A mounted native selection can remain an
   unadmitted presentation intent until capacity frees; a newer native selection
   or source replacement supersedes it. Recycling explicitly refuses an
   unadmitted selection, records bounded Capacity diagnostics, and does not
   copy it into saved authoritative state or advance its revision. An admitted
   envelope keeps its persistent recipient snapshot/cursor. Native tab lookup
   no longer mistakes a nested Markdown surface's controls for the owning block.
   During callback reentry the in-flight envelope still occupies capacity until
   its last recipient, preventing a partial delivery from exceeding the bound.
3. **Retirement starved by recursive listener traffic (P2).** The pump alternates
   first claim on the **same** listener/retirement callback budget. Retirement
   invalidation remains immediate; host notification/acknowledgment is bounded
   and FIFO within its existing retirement ordering. With callbacks=1, each
   continuously runnable class gets a first claim within two pumps; a finite
   retirement backlog of N entries progresses within at most 2N pumps while
   listeners also progress. No callback-budget reset/inflation, unbounded drain,
   durable-work cancellation or backend operation was introduced.

The analogous-path audit followed dispatch, timer action/completion, feed loading
and native-slot snapshots. Dispatch and timers already preflight capacity without
application code between mutation and envelope enqueue. Feed loading **does**
invoke application code in that gap. It now holds a synchronous RAII envelope
reservation across the host loader; reentrant interaction cannot steal that
slot. The reservation drops on refusal, supersession and unwinding without World
access. Initial unprobed autoloads refused capacity retry through existing
`FeedWatch` reconciliation. Manual refresh notification follows authoritative
enqueue, not vice versa. Reservation/in-flight occupancy is included in the
existing queued-envelope oracle counter (still a 19-counter vector), so it
cannot be hidden from teardown accounting. A unit test proves bounded admission
and lease release on both ordinary drop and unwind.

Only four existing Liquid implementation modules, one new public integration
test target and this append-only qualification record change from d2edd1ad. The
already-authorized core host/Markdown ownership dependency is unchanged; there
is no further core, World/cascade, Cargo, parser, SDK, F2 or flow-crate expansion.

The current requested scope explicitly resolves the earlier §J.2 ambiguity:
caller-requested subtree despawn intentionally deletes still-attached descendants
before Liquid drain. These fixes neither resurrect those descendants nor alter
generic core cascade semantics. Liquid's own detach/recycle/rebuild is different
and must preserve unrelated application ownership, as the regressions prove.

### K.2 Committed regression-before/after evidence

`crates/rndr_liquid_ui/tests/review_regressions.rs` uses public construction,
`set_parent`, real `try_activate`/action dispatch, frame pumps, state/handle
queries and host callbacks. No private fixtures or test-side cleanup mask the
behaviors under assertion. All nine final tests were run unchanged against the exact
original candidate's four implementation files, extracted with
`git show d2edd1ad1a4ac353bc5a1cbb30346905eb0e3946:<path>`. A `try/finally` restored all four
fixed source byte strings, with equality verified, before final qualification.
No additional worktree, branch or shared reference was changed.

With the environment in §K.3, the command was:

```sh
cargo test --locked --offline -p rndr_liquid_ui --test review_regressions -- --nocapture
```

Original implementation: **exit 101, 1 passed, 8 failed, 0 ignored, 0 filtered**.
Corrected implementation: **exit 0, 9 passed, 0 failed, 0 ignored, 0 filtered**.
The eight defect regressions fail the original candidate; the ninth is a
compatibility control, `retained_logical_child_parks_when_its_parent_occurrence_moves`,
which passes both original and corrected candidates. It catches duplication or
loss of genuinely owned child surfaces when a new block is prepended. The final
unfiltered command below runs all nine again.

| Permanent public regression | Original-candidate failure |
|---|---|
| `independent_surface_survives_parent_collapse_rebuild_and_cleanup` | Independent B loses `LiquidView` on collapse (cleanup=0) |
| `native_tabs_defer_atomically_until_envelope_capacity_is_available` | Full queue nevertheless commits a tab revision |
| `native_tab_activation_inside_partial_delivery_is_admitted_once` | Reentrant native activation commits before admission |
| `deferred_native_tab_intent_can_be_superseded_without_a_phantom_commit` | Phantom tab event survives supersession |
| `recycling_refuses_unadmitted_native_tab_without_changing_revision` | Unmount snapshots an unadmitted tab revision |
| `retirement_and_listeners_both_progress_under_sustained_reentrant_load` | Retirement calls remain 0 instead of 1 |
| `retirement_backlog_cannot_starve_reentrant_listener_traffic` | No retirement progress within two pumps |
| `feed_reserves_envelope_capacity_across_reentrant_loader` | Reentrant host interaction steals feed admission |

The primary retirement test continues recursive native accordion activation for
12 pumps, proves exactly-once retirement within four pumps, host capture drop,
cooperative acknowledgment and stale-result rejection. The reciprocal-load test
keeps eight retirements queued with callbacks=1, requires both classes to
progress within two pumps, completes all eight within sixteen pumps, and checks
that listener plus retirement invocations never exceed one per pump. Further
pumps prove no duplicate retirement. None of this cancels a durable/backend job.

Intermediate failures are retained honestly: the first test fixture incorrectly
expected a return value from the legacy unit-returning append function and used
ASCII `-` rather than the actual `−` label; both were corrected before baseline
reproduction. A diagnostic post-host capacity recheck showed that an initial
feed load needed real reservation/retry, not just dropping an outcome. An
expanded independent-surface fixture initially counted legitimate Pending feed
poll envelopes as accordion clicks (15 versus 1), causing one intermediate
unfiltered run to exit 101 after its oracle passed. That assertion now counts
specifically the independent accordion Collapse event. No production failure was
hidden by a skip, ignore, deleted test or weakened acceptance text. The final
unfiltered rerun below uses the corrected fixtures and final sources. A further
compatibility control found an intermediate ownership-fix regression: deriving
an old projection's parent from a newly reassigned block index duplicated a
retained logical body (2 versus 1). Caching the existing F1 projection lease
fixed it; the permanent ninth control passes the original and final candidates.
The entire unfiltered suite/oracle was rerun after that final source change.

### K.3 Exact final commands, counts and readiness

All Cargo invocations use:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
```

- `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture`
  — **exit 0, 79 passed, 0 failed, 2 pre-existing opt-in performance probes
  ignored, 0 filtered**, including all unit/integration/doc targets. The Liquid
  UI unit target has **18 passed** and took **88.10 s**. Deliberate caught-panic
  diagnostics in unwind tests are not failed tests.
- The same unfiltered command executes
  `full_same_world_100_cycle_resource_oracle_seeds_1_7_42`: **100 completed cycles
  per seed 1, 7, 42**, same World within each seeded run, all **19 counters** at
  baseline after every teardown. No extra test-side subtree sweep/cleanup was
  added. Seed timings: **28.921796208 s**, **29.431631666 s**, **29.743815875 s**.
  Each seed's baseline is
  `[0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0]`; the retained 1 is the intentional
  World-owned listener. Each seed's sampled high-water vector is
  `[6,1,6,8,5,1,0,8,1,1,1,1,1,0,1,1,0,91,206]` in §J.3 order. Actual Layout
  entities and tagged projections return to zero; this is not allocator/RSS
  certification or a throughput claim.
- `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown`
  — **exit 0, 179 passed, 0 failed, 1 pre-existing ignored, 0 filtered**:
  138 core unit tests, 11 feed, 15 Markdown and 15 runtime tests. Includes host
  unwind/drop, close and Markdown ownership controls without filtering.
- `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings`
  — **exit 0**, no new lint suppression.
- `cargo fmt --all -- --check` and `git diff --check` — **exit 0** after this
  record was appended.

**Recommendation: ready for fresh independent review of this corrected F1
candidate, not implementation acceptance or integration.** Sender review,
native acceptance and integration remain the sender's responsibility. §I/§J
receipts remain history, not evidence that the reviewed d2edd1ad was accepted.
F2 and Medousa remain untouched. Whole-stream append staging remains **O(S_i)**
per update and potentially quadratic in aggregate over many small appends; this
work neither fixes nor qualifies that performance characteristic. No Medousa
adapter readiness, native interaction/visual certification, backend cancellation,
approval resolution, push, publication, deployment or merge is claimed.
