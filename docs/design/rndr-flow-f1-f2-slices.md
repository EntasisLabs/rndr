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

## L. P1 correction after independent review of b4dfe507

This is the **same F1 undertaking**, continuing from clean admitted
`b4dfe507d432df29477fd1e51e7faaa48fbf6d93`. `main` was verified at
`f2919a1692d0bb7e0fba2e26976a1631cf4c637f` and is not changed. The sender
accepted the **review deliverable only**, requested changes, and retains
implementation review and native integration. Green §K gates were not
implementation acceptance: independent review found two additional P1 defects.
Sections H–K, including their failed and intermediate receipts, remain history.

### L.1 Reproduction and correction boundaries

The external file was accessible and read directly:
`/tmp/f1-independent-fixture.TlASfN/crates/rndr_liquid_ui/tests/adversarial_review.rs`.
Its unchanged 13 public tests were copied into the admitted checkout's regression
target **before implementation edits**. With b4dfe507 implementation they gave
**11 passed, 2 failed, exit 101**, no ignored/filtered tests:

- `mixed_physical_semantic_child_relocation_does_not_duplicate`: A's registered
  body child C was physically moved under independent B, itself attached under
  A's accordion label. Collapse/reopen produced **two** live body views instead
  of one reused handle. Physical traversal stopped at B and never parked C.
- `loader_reentrant_prepend_cannot_publish_feed_state_into_new_callout`: the
  loader prepended a callout while retaining the news feed. The old poll then
  published a Feeds record with the captured **block 0**, now a Callout, and used
  the old source address for its refresh envelope.

`lifecycle::park_under` now selects descendants using **registered semantic
parent keys**, across the live view registry, not physical ancestry. It parks
in deepest-first, reverse-registration tie order and reclaims each parked
surface's exact renderer-created/native projection sets. This works for children
behind foreign surfaces or moved outside the owner presentation tree. B and
B's own semantic descendants are neither parked nor retired by A's rebuild;
application attachments are detached from dying projections, not cascaded.
Reattachment reuses the original registered child lifetime. No title-key policy,
F2 identity scheme, generic World/despawn change or new public API is introduced.

`feed::poll` captures the request context and revision, then validates after
host return: live key, non-retired **exact active observation**, current logical
location, still-matching root location, and unchanged revision. A relocated or
superseded attempt is **refused without state, revision, mark or envelope
publication**; the current mounted watch can retry a retained live request.
Thus no stale positional address is used as publication authority. Reentrant
refresh while the host is borrowed stays Pending/retryable, rather than being
misclassified as a synchronous feed by the unavailable-host default Defer.

The public context-result path also reserves an envelope permit before commit,
emits the matching FeedRefresh/revision, and selects rebuild roots by the exact
logical lease after event callbacks (which can themselves mutate the source).
This prevents a newer reentrant result from changing state without its admitted
event, and the suspended old poll cannot overwrite that newer revision. The
existing World-free permit Drop releases capacity on refusal and panic. Shared
callback/envelope budgets and retirement behavior are retained, not bypassed.

### L.2 Permanent public regression evidence

The original nine §K tests are unchanged. Four external probes are now permanent:

- both failures above (the feed probe additionally asserts eventual Ready body,
  exactly Pending/current-Ready publication, revision 2 and current source);
- `public_feed_reservation_releases_after_loader_panic_and_stale_owner`, both
  modes with **envelopes=1, callbacks=1**, proving a later feed can publish;
- `mixed_ownership_terminal_controls_keep_independent_and_retire_owned_child`,
  all three raw-owner-despawn/component-removal/source-removal modes, preserving
  B's handle/content while retiring A-owned C.

Two more permanent tests exercise nested independent B/child placement and
outside-owner placement over three collapse/reopen cycles, and loader reentry
through source removal, feed replacement, a newer context result, and refresh
observation supersession. They check handles, live-view counts, independent
state/content, eventual correct body/revision/event, and rejection of stale
context puts with unchanged records/events. No hidden test cleanup was added.

Exact final permanent test command:

```sh
cargo test --locked --offline -p rndr_liquid_ui --test review_regressions -- --nocapture
```

| Implementation under byte-identical final 15 tests | Result | Exit |
| --- | --- | ---: |
| archived b4dfe507 | **11 passed, 4 failed**, 0 ignored/filtered | 101 |
| corrected candidate | **15 passed, 0 failed**, 0 ignored/filtered | 0 |

The two additional baseline failures are the nested/outside-placement test
(5 live views instead of 4) and the reentrant newer-result test (obsolete loader
outcome overwrote the newer result). The original nine and both pertinent
external passing controls remain green. The unchanged external 13 also passed
13/13 after the initial implementation correction, before the additional
qualification assertions were added.

The final comparison uses `git archive b4dfe507` at
`/tmp/f1-p1-baseline.IqzrMZ`, with only `tests/review_regressions.rs` replaced.
NUL-safe tracked-file byte comparison against b4dfe507 confirmed that this was
the **only changed archive path**; the source checkout was not reset or
modified for the baseline run. Final test bytes match between archive and
candidate (SHA-256
`adc54520c18321602abb77cacf121a7c41c9b3b78d34bbfe0183269f10c12320`).
`CARGO_TARGET_DIR` pointed at the admitted target directory for dependency reuse;
logs identify the archive's package source paths, not the corrected source.

Intermediate evidence is retained: `/tmp/f1-p1-expanded.log` had **14 passed,
1 failed, exit 101** because reentrant refresh left a body-less sync-marked
watch. Keeping borrowed-host refresh Pending fixed that retry failure; expanded
15 tests then passed before the final exact-lease rebuild guard was added.
Caught deliberate panic diagnostics are expected in the public unwind control,
not ignored/failed tests.

### L.3 Final exact gates, logs and resource bounds

All Cargo invocations use:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
```

| Exact command | Final result | Full workshop log |
| --- | --- | --- |
| `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture` | **85 passed, 0 failed, 2 ignored, 0 filtered; exit 0**, including docs | `/tmp/f1-p1-final-liquid.log` |
| `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown` | **179 passed, 0 failed, 1 ignored, 0 filtered; exit 0** | `/tmp/f1-p1-final-core.log` |
| `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings` | **exit 0**, no lint suppression | `/tmp/f1-p1-final-clippy.log` |
| `cargo fmt --all -- --check` | **exit 0** | `/tmp/f1-p1-final-fmt.log` |
| `git diff --check` | **exit 0**, repeated after this record | `/tmp/f1-p1-final-diff.log` |

The Liquid ignored probes remain `liquid_stream_cost` and `liquid_density_cost`;
the core ignored probe remains `typing::storage_tests::snapshot_storage_accounting`.
Core totals remain 138 unit + 11 feed + 15 Markdown + 15 runtime. The UI unit
target has **18 passed**, including the unchanged full oracle, in **90.61 s**.

The unfiltered command completed **100 same-World cycles each for seeds 1, 7,
42**, all **19 counters at baseline after every teardown**, without extra
cleanup. §J.3 defines the unchanged vector order and host-local capture/drop
checks. All three baseline vectors are:

```text
B = [0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0]
H = [6,1,6,8,5,1,0,8,1,1,1,1,1,0,1,1,0,91,206]
```

`H` is each seed's sampled high-water, not an instruction-level allocator peak.
The 91 tagged projections and 206 actual Layout entities return to zero at
teardown. The retained baseline 1 is the intentional World-owned listener,
whose capture releases on World drop. Cooperative host captures release and
retirement acknowledgement is checked. Seed elapsed times are **30.43613325 s**,
**30.1699005 s**, **30.0039725 s**, respectively. The legacy `attempted-prefix`
console label is retained honestly; all 100 iterations of each run passed.

Additional full logs (workshop-local `/tmp`, not durable published artifacts):

- `/tmp/f1-p1-before.log`: unchanged external 13 on b4dfe507, 11/2, exit 101;
- `/tmp/f1-p1-after-initial.log`: external 13 after initial fixes, 13/0, exit 0;
- `/tmp/f1-p1-expanded.log` and `/tmp/f1-p1-expanded2.log`: intermediate retry
  failure and correction;
- `/tmp/f1-p1-final-regressions-before.log` and
  `/tmp/f1-p1-final-regressions-after.log`: byte-identical final 15 before/after.

Admission limits are unchanged: defaults **4096 scopes**, **32 feed lanes per
block**, **256 listeners per owner**, **1024 envelopes per World**, **256 shared
listener/retirement callbacks per pump**, **512 diagnostic entries**. These bound
local logical work, not allocator/RSS or uncontrolled external legacy workers.
Parking now scans the live registry and semantic ancestor paths and sorts the
selected views; native reclamation scans live native ownership metadata. This
is not a constant-cost or throughput claim, and introduces no historical
identity cache. Whole-stream append staging remains **O(S_i) per update**, with
**potential aggregate quadratic cost** over many small appends, as in §J.4.

### L.4 Review, cascade and seal boundary

**Ready for fresh independent review, not implementation acceptance or native
integration.** The explicit caller subtree-despawn contract is unchanged: it
intentionally deletes still-attached descendants. Liquid does not resurrect
those nodes. Unrelated and reparented-out application nodes survive Liquid's
own exact cleanup. This distinction is not silently weakened into a survival
promise against caller-requested generic cascade.

F2, a flow crate, Medousa, parser/Cargo/SDK and generic core cascade code remain
untouched. No backend/durable-work cancellation, approval resolution,
self-approval, push, publication, deployment or merge occurred. No performance,
Medousa adapter readiness or native/visual/accessibility certification is
claimed. Native metadata/seals were not edited: the sender must obtain a fresh
seal of the exact eventual candidate before any approval; reopening old
d2edd1ad evidence for admission does not seal or accept this correction.
### L.5 Follow-up: exact-request Pending settlement through reentrant retirement

This follow-up addresses only the inherited P1 independently reproduced against
clean `5b2feb8a743bc9dbef5749caadddedb40ba3e587`. The admitted HEAD and clean
status were checked before edits; `main` remained
`f2919a1692d0bb7e0fba2e26976a1631cf4c637f`. Earlier §L results remain historical,
not acceptance of this follow-up.

#### L.5.1 Defect and settlement/publication separation

A fresh observation previously began with `pending = false`. A host loader
could retire/remove/replace its source and then return `Pending`; the correct
post-callback stale-publication refusal returned **before** updating that flag.
The retirement drain consequently forgot escaped, unacknowledged async work.
The public review probe admitted six distinct retired Pending contexts with
`scopes = 2`; the ordinary Pending retirement control stopped at two. The
available review logs were inspected, including
`/tmp/f1-review5-final-hostile_review5.log` (5 pass / 1 fail) and the parent
failure/control logs `/tmp/f1-review5-before-pending-{retirement,control}.log`.
These are corroborating historical observations, **not substitutes for reruns**.

The focused implementation in `src/feed.rs` separates two obligations:

- **Settlement is exact-request owned.** `LiquidFeedContext` shares a small
  settlement cell across its clones and all active/retiring/unacknowledged
  containers. No post-callback accounting lookup uses just `(Key, instance)`;
  settling an old request cannot charge or clear its replacement in that slot.
- **Actual host execution is reserved.** A World-free `LoadSettlement` guard
  marks an actual loader invocation in flight. Its normal return settles
  Pending/Ready/Failed/Defer before all publication checks. Its unwind retains
  potentially escaped work conservatively until acknowledgment, unless a newer
  contextual result has already settled it. The existing `HostSlot` borrow
  serializes actual loader invocations; no settlement lock crosses host code.
- **Retired retention tests obligation, not presentation.** A retained request
  stays charged while its loader executes, even after an early retirement
  acknowledgment; after return it remains charged only for unacknowledged
  Pending work. Synchronous settlement and acknowledged work are reaped on the
  ordinary admission/drain paths. No backend cancellation or worker completion
  is inferred from UI disposal.
- **Borrowed-host Pending is a retry presentation, not escaped async work.**
  Reentrant refresh still gets its existing Pending/watch retry behavior, but
  no actual loader invocation means no new asynchronous settlement obligation.
- **Notification callbacks keep their reservation.** A retirement notification
  remains in the charged queue while host code executes and reenters. It is
  removed only after return/unwind, then retained if an obligation remains.
  The borrowed host prevents recursive notification dispatch; callback budgets
  and postorder/reverse-registration notification ordering are unchanged.
- **Publication remains separately guarded.** Existing liveness, exact active
  context, root lease, location, revision and exhaustion guards still refuse
  obsolete state/events. Accepted contextual results replace a held identity
  token. The enclosing loader cannot overwrite that newer settlement **or
  publish its own outcome**, even if the newer result is identical and
  `store_snapshot` leaves the numeric component revision unchanged. Holding
  the original token also avoids identity-reuse/ABA and counter-exhaustion
  issues. No historical token cache is retained.

The unchanged envelope permit still drops on every stale return, capacity
refusal and unwind. The result path settles the exact context after its
callback-free state commit and before interaction delivery, so delivery
reentry cannot redirect accounting to a superseding request.

#### L.5.2 Permanent regressions and genuinely rebuilt comparison

`tests/review_regressions.rs` remains **byte-for-byte unchanged**, including all
15 earlier regressions (SHA-256
`adc54520c18321602abb77cacf121a7c41c9b3b78d34bbfe0183269f10c12320`). New,
formatted, permanent public tests live separately in `tests/pending_settlement.rs`:

1. Reentrant newly-started Pending retirement remains bounded; acknowledgment
   and normal drain restore admission.
2. The normal Pending retirement control also proves acknowledgment/recovery.
3. Reentrant Ready, Defer, Failed, early acknowledgment and borrowed-host refresh
   controls admit six successive cycles without phantom async charges.
4. An actual loader panic after retiring its source cannot escape accounting;
   acknowledgment restores admission, the host is restored, and an
   `envelopes = 1` permit is reusable after unwind.
5. A newer contextual Ready settles the request even when its enclosing loader
   later returns Pending and retires the owner.
6. A contextual result that does **not** advance the component revision still
   supersedes the enclosing loader's obsolete publication.

Three private tests additionally cover exact identity across active → retiring
→ drained/unacknowledged state and same-slot replacement; in-flight early ack,
Pending/Ready/Failed/Defer/unwind/newer-result settlement; and notification
reservation refusal during reentrant admission followed by successful admission
when the synchronous notification finishes.

The final public file's SHA-256 is
`080c5f1a77b25ed87283af842ad65000a033303aa090d5723532f148b9b48798`.
For the baseline, `git archive 5b2feb8a` was extracted to
`/tmp/f1-pending-before.UDyllP`; **every tracked file** was compared bytewise
against the Git tree (zero differences). Only the new public test file was
added. That file is identical to the candidate's, not an adjusted oracle.

The baseline and corrected disposable builds reuse dependency artifacts via
`CARGO_TARGET_DIR`, but **each source switch explicitly runs
`cargo clean -p rndr_liquid_ui` before testing**. Clean receipts and compilation
logs are retained. Archive timestamps/shared caches previously caused false
baseline passes during review; an old package path or short build time alone
is not evidence of recompilation. The comparison below relies on forced
package rebuilds, not those historical cache assumptions.

| Command/comparison | Observed result | Workshop-local log |
| --- | --- | --- |
| Final six public tests on forced-rebuilt admitted baseline | **3 passed, 3 failed, 0 ignored/filtered; exit 101** | `/tmp/f1-pending-final-before.log` |
| Same final six public tests on corrected disposable source | **6 passed, 0 failed/ignored/filtered; exit 0** | `/tmp/f1-pending-review-pending_settlement.log` |
| Unchanged independent review 13 on corrected disposable source | **13 passed, 0 failed/ignored/filtered; exit 0** | `/tmp/f1-pending-review-adversarial_review.log` |
| Unchanged hostile review 6 on corrected disposable source | **6 passed, 0 failed/ignored/filtered; exit 0** | `/tmp/f1-pending-review-hostile_review5.log` |
| Unchanged earlier permanent 15 on corrected disposable source | **15 passed, 0 failed/ignored/filtered; exit 0** | `/tmp/f1-pending-review-review_regressions.log` |

Commands use `cargo test --locked --offline -p rndr_liquid_ui --test NAME --
--nocapture`, where NAME is respectively `pending_settlement`,
`adversarial_review`, `hostile_review5`, or `review_regressions`. The corrected
disposable fixture is `/tmp/f1-pending-review.LCWBlV`, with only the corrected
feed source, permanent new test, and the two unchanged external probe files
added to the admitted archive. The 13/6 external files retain SHA-256
`8e77e826c99ea903be683eb361c2cb8eb683a685887b2b3f290cf548c027a6f2` /
`f13a5f02b0907d4ada79acd81f5e8e7d3144263e5cee41933396cbb73d4319fa`.
External probes remain disposable, not new tracked unrelated tests. Their
pre-existing unused-constant warning is not a warning suppression in production
or permanent tests.

The three baseline failures are the reported six-context Pending escape, the
same accounting escape on loader unwind, and same-revision enclosing-result
publication. The normal backpressure, synchronous/no-phantom and newer-Ready
controls pass the baseline. The corrected original hostile probe observes
**one** retained unacknowledged context at `scopes = 2`, not six; the other slot
is occupied by the current active observation, so a fresh manual replacement
correctly refuses. The normal control still observes two. Recovery is asserted,
not merely inferred from those counts.

Intermediate failures are retained honestly: `/tmp/f1-pending-after.log` used
an overstrict added assertion that exactly two retired contexts must accumulate
(the actual one plus an active lane correctly fills capacity); that assertion
was corrected to the original nonempty bounded contract. The first deliberate
unwind harness left its panic arm set after a refused refresh, causing an
unrelated later frame to panic; `/tmp/f1-pending-controls{,2}.log` and
`/tmp/f1-pending-unwind-debug.log` retain that harness failure. The final harness
clears an unconsumed arm and checks real unwind accounting/admission recovery.
The same-revision test initially failed the intermediate settlement-only fix
(`/tmp/f1-pending-same-revision-before.log`); exact result-token publication
refusal resolves it (`/tmp/f1-pending-token-after.log`). An intermediate private
control needed a match-statement semicolon after `finish` began returning
publication eligibility; compiler/lint exit-101 evidence is retained at
`/tmp/f1-pending-token-{unit,clippy}-compile-error.log`, not counted as a green
gate. Earlier §I–§L failures and qualifications are not deleted.

#### L.5.3 Final unfiltered qualification, resources and handback boundary

Environment: `PATH="$HOME/.cargo/bin:$PATH"`, `CARGO_BUILD_JOBS=2`,
`CARGO_INCREMENTAL=0`. After fixture builds, the admitted package was explicitly
cleaned and rebuilt before full qualification; no fixture artifact is used as
candidate evidence.

| Exact command | Final result | Workshop-local log |
| --- | --- | --- |
| `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture` | **94 passed, 2 pre-existing opt-in probes ignored, 0 failed/filtered; exit 0**, including docs | `/tmp/f1-pending-final-liquid.log` |
| `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown` | **179 passed, 1 pre-existing ignored, 0 failed/filtered; exit 0** | `/tmp/f1-pending-final-core.log` |
| `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings` | **exit 0** | `/tmp/f1-pending-final-clippy.log` |
| `cargo fmt --all -- --check` | **exit 0** | `/tmp/f1-pending-final-fmt.log` |
| `git diff --check` | **exit 0** after final documentation | `/tmp/f1-pending-final-diff.log` |

The Liquid total increases from 85 by **six new public tests and three private
controls**, not by filtering any earlier failure or adding ignored tests. The
UI unit suite took **89.34 s**. All 300 same-World cycles ran in order: 100 each
for seeds **1, 7, 42**. Every teardown passed the unchanged full 19-counter
baseline check, weak-capture/drop and retired-context publication checks, with
no new test-side cleanup. The actual Layout and tagged projection high-water
remain **206 / 91**, respectively; both return to zero. Caller-owned shell
removal still occurs only after renderer drain and never cascades hidden
renderer residue. The separate host-registration oracle completed 100 cycles
per seed with **retained = 0, high_water = 0**. Existing host unwind/drop,
callback order, append/title and legacy compatibility tests remain green.

For all three seeds, in the existing §J/§L counter order:

```text
baseline   = [0,0,0,0,0,0,0,0,0,0,0,0,1,0,0,0,0,0,0]
high_water = [6,1,6,8,5,1,0,8,1,1,1,1,1,0,1,1,0,91,206]
seed 1 elapsed  = 29.267170625 s
seed 7 elapsed  = 30.089586917 s
seed 42 elapsed = 29.982754417 s
```

The final documentation briefly had a trailing blank line; its exit-2 diff
receipt is preserved in `/tmp/f1-pending-doc-whitespace-before.log` and is
superseded by the final clean diff/staged/committed-range checks. Package clean
receipts are `/tmp/f1-pending-final-before-clean.log`,
`/tmp/f1-pending-review-clean.log`, and
`/tmp/f1-pending-final-restore-clean.log`. The final source/private-test bytes
were also recopied to the disposable fixture, its public suites rebuilt and
rerun, then the admitted source rebuilt again for the permanent public suite;
that restoration does not replace the full unfiltered qualification above.

Limits and policy remain unchanged: **4096 scopes, 32 feed lanes/block,
256 listeners/owner, 1024 envelopes/World, 256 shared callbacks/pump, 512
diagnostics**. Async work escaped by a panicking loader is conservatively
retained until the host acknowledges retirement; an unacknowledging host
continues to exert bounded backpressure, not silent cancellation or unlimited
admission. No in-flight settlement state is dropped just because publication
became ineligible. Synchronously settled work and acknowledged/drained work
restore capacity without a phantom Pending obligation.

This adds request-local settlement bookkeeping, not a performance claim.
Parking still scans ownership metadata, and whole-stream append staging remains
**O(S_i) per update**, potentially quadratic in aggregate. Generic caller
subtree despawn still intentionally deletes attached descendants; there is no
resurrection or generic cascade change. Liquid's own cleanup preserves
unrelated/reparented-out application nodes. F2, parser, SDK/Cargo and Medousa
remain untouched; no Medousa build, adapter or readiness claim is made.

**Ready only for fresh independent review.** No implementation acceptance,
approval, integration, push, publication, deployment, durable/backend
cancellation or metadata edit was performed. The stale native `d2edd1ad` seal
was not repaired or edited. The sender retains review/integration and must
obtain a **fresh exact corrected candidate seal before any approval**; reopening
old evidence for admission did not seal or accept this correction.

## M. F2 initial proof-only qualification — public error-contract conflict

**F2 is not implemented or qualified by this record.** This is a blocked,
proof-only handback from the isolated F2 undertaking. Historical F1 receipts
above are unchanged and are not fresh F2 test evidence.

### M.1 Verified starting point and bounded failing tests

Before edits, branch
`worktree/liquid-f2-semantic-identity-and-stale-handle-correctness` and HEAD
were checked: HEAD and local `main` both equaled accepted F1
`d18c55414d30c4c563148b30fd7e096f0bfd863d`, with no tracked or untracked changes.
Initial filesystem availability was about 10 GiB. Builds use only the isolated
worktree's `target/f2`, initially absent; after the focused build it occupied
about 427 MiB, with about 11 GiB filesystem availability. No shared target,
network download, dependency/Cargo/lockfile edit or production edit was made.

Permanent `crates/rndr_liquid_ui/tests/identity.rs` was added before production
changes. It reproduces ambiguous equal-block record inheritance after a
nonappend prefix insertion independently when mounted and when offscreen.
The offscreen fixture checks that no Liquid blocks are physically mounted
before the replacement. A control proves no-op/true-append preservation for
duplicates. A separate exhaustive-match fixture proves the existing public
error shape compiles unchanged.

All focused Cargo commands use:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$PWD/target/f2"
cargo test --locked --offline -p rndr_liquid_ui --test identity -- --nocapture
```

The final baseline is **2 passed, 2 failed, 0 ignored/filtered; exit 101**,
recorded at `target/f2/baseline-identity-final.log` (worktree-local, not a
published artifact). Both failures are the desired-behavior ambiguous-group
assertion: an old record survives when it must reset. The no-op/append and
legacy exhaustive-match controls pass. There is **no after-implementation
result**. An initial harness incorrectly assumed projected source position
zero and failed both tests during setup; that run at
`target/f2/baseline-identity.log` is not defect evidence. Correcting setup to
activate a real control produced the initial one-pass/one-fail reproduction
at `target/f2/baseline-identity-corrected.log`, before separating mounted and
truly offscreen cases into the final four-test file.

### M.2 Decision required before implementing the public F2 API

Accepted F1 `src/lifecycle.rs:24–33` already defines and publicly exposes the
exhaustive `LiquidUpdateError` enum with exactly `StaleOwner`, `Capacity` and
`Exhausted`; `src/lib.rs:48` re-exports it. The additive API paragraph in §A
requires the **same named return type** to distinguish eight conditions,
adding `StaleBlock`, `StaleRequest`, `Revision`, `Kind` and `InvalidKeys`.
The F2 assignment also explicitly prohibits exhaustive-breaking changes to
legacy public enums. These requirements cannot both be met literally:
adding variants or retroactively adding `#[non_exhaustive]` breaks the
permanent downstream exhaustive-match fixture. Associated-constant aliases
would preserve compilation but would not distinguish the required errors.

The sender was asked to settle one bounded correction, with no decision
inferred from worker initiative:

1. Preferably keep F1's enum unchanged and authorize a separately named,
   detailed F2 error type for the new APIs, updating their design signatures;
   or
2. Explicitly exempt `LiquidUpdateError` from exhaustive-match compatibility,
   authorizing the additional variants and documenting the break.

No production implementation, error-shape change or reinterpretation of the
acceptance requirements was made pending that decision. This is not a reason
to weaken the resolver, full identity table or remaining acceptance gates.

### M.3 Remaining work and handback boundary

All F2 implementation and combined qualification remain outstanding, including
keyed/handle/reset APIs, recursive semantic correspondence, child/resource
compatibility, actual-mutation stale admission, complete table tests,
F1 regression/settlement and all 300 interleaved resource-oracle cycles,
facade/feature-off compatibility and final full tests/core/Clippy evidence.
Those suites were not rerun for this proof-only result. The focused failing
tests must remain meaningful; they are not ignored or changed to expect the
baseline defect.

This record is **not** an implementation candidate for acceptance or
integration. No seal, approval, integration, push, publication, deployment or
Medousa migration was performed. The sender retains contract decisions,
review, continuation and eventual native integration. The existing
whole-stream staging performance caveat remains unchanged.

## N. F2 continuation: additive error-contract correction and implementation

### N.1 Explicit contract correction (supersedes the same-error-type sketch)

The sender assessed proof-only `54f3a15cc3221980a344c7cd4c2dcf24a47fba61`
as **changes requested**, not an acceptable F2 implementation. The compatibility
conflict in M is resolved within the original additive scope: keep the public
`LiquidUpdateError` **exactly** as its existing three exhaustive variants
`StaleOwner`, `Capacity`, `Exhausted`. Do not add variants, mark it
`non_exhaustive`, change existing signatures, or change legacy envelope,
component-record, timer-record or feed-slot struct-literal layouts. Historical
receipts and the original API sketch above are retained; their proposal that
new detailed F2 failures use that *same* legacy error type is **superseded by
this additive correction**. M remains accurate evidence about the earlier
proof-only result, not a current blocker or an implementation qualification.

New F2 APIs use the separately named public `LiquidF2Error`:

- `StaleOwner`: foreign, dead, poisoned or otherwise inadmissible owner.
- `StaleBlock`: retired/missing block incarnation in a still-live owner.
- `StaleRequest`: superseded observation/request lane.
- `Revision { expected, actual }`: source/state compare-and-set failure.
- `Kind`: incompatible state variant or child-index domain.
- `InvalidKeys`: incomplete/duplicate/empty/oversized/control-containing key
  assignments or malformed, cyclic, unresolved internal references.
- `Capacity`: bounded admission refusal.
- `Exhausted`: checked nonwrapping counter/deadline admission refusal where the
  particular API has a fallible return. Existing infallible timer actions
  refuse without state/event/deadline mutation, retaining their signatures.

The corrected additive public API is implemented in `identity.rs` and exported
by `rndr_liquid_ui`, without parser, core, facade production or Cargo changes:

```rust,ignore
pub struct LiquidKeyMap { pub keys: Vec<Option<String>> }
// Opaque, process-local; no host constructor, serialization or reuse contract.
pub struct LiquidBlockHandle(/* private */);
pub struct LiquidSourceRevision(/* private */); // includes public ZERO

pub fn inspect_liquid_occurrences(source: &str)
    -> Result<Vec<LiquidOccurrenceDescription>, LiquidF2Error>;
pub fn liquid_source_revision(world: &World, surface: LiquidSurfaceHandle)
    -> Result<LiquidSourceRevision, LiquidF2Error>;
pub fn liquid_component_handle(world: &World, surface: LiquidSurfaceHandle,
    occurrence: usize) -> Result<LiquidBlockHandle, LiquidF2Error>;
pub fn liquid_component_state_for(world: &World, block: LiquidBlockHandle)
    -> Result<Option<LiquidComponentRecord>, LiquidF2Error>;
pub fn put_liquid_component_state_for(world: &mut World, block: LiquidBlockHandle,
    expected_state_revision: u64, state: LiquidComponentState)
    -> Result<LiquidComponentRecord, LiquidF2Error>;
pub fn set_liquid_source_keyed(world: &mut World, surface: LiquidSurfaceHandle,
    expected_source_revision: LiquidSourceRevision, source: &str,
    keys: &LiquidKeyMap) -> Result<LiquidSourceRevision, LiquidF2Error>;
pub fn reset_liquid_block(world: &mut World, block: LiquidBlockHandle,
    expected_state_revision: u64) -> Result<LiquidBlockHandle, LiquidF2Error>;
pub fn reset_liquid_surface(world: &mut World, surface: LiquidSurfaceHandle,
    expected_source_revision: LiquidSourceRevision)
    -> Result<LiquidSurfaceHandle, LiquidF2Error>;
pub fn put_liquid_feed_result_for(world: &mut World, context: &LiquidFeedContext,
    expected_state_revision: u64, outcome: FeedOutcome)
    -> Result<(), LiquidF2Error>;
```

`LiquidOccurrenceDescription { ordinal, kind }` describes settled top-level
reading order, **not** original-source byte offsets, global parse-table indices,
keys inferred from props, or stable incarnation IDs. `LiquidKeyMap.keys` is the
complete sidecar for exactly that sequence, including `None` entries. Key
validation uses nonempty UTF-8 labels of at most 256 **bytes**, rejects Unicode
controls and duplicate `Some` labels, never trims/normalizes labels, and never
puts key/source/body text in refusal diagnostics. Nested parse entries and
unfinished fences do not acquire a top-level key slot.

`LiquidInteractionEnvelopeFor { envelope, surface, block }` wraps the unchanged
legacy envelope. `LiquidInteractionHandlerFor` and
`subscribe_liquid_interactions_owned_for` are additive owned observation APIs.
Its `LiquidSubscribeErrorFor<H>` returns the untouched handler with a
`LiquidF2Error` reason; the legacy `LiquidSubscribeError<H>` remains unchanged.
Dispatch captures handles from the committed delivery key, not a fresh lookup of historical source metadata after
another listener runs. `LiquidFeedContext::surface_handle()` and
`block_handle()` expose immutable observation ownership. The existing
`put_liquid_feed_result` retains its exact legacy signature/error type; its new
`_for` counterpart exposes distinguishable F2 refusals. Feed bodies retain the
existing self-describing datatype behavior; request identity is the actual
instance/feed-id/requested-datatype/field binding, not a guess from a last-good
body's display datatype.

### N.2 Implementation semantics and scope

- One correspondence plan drives logical block membership, component records,
  timer/feed ownership and mounted/saved/carry presentation state. It uses live
  host keys plus kind, or conservative uniqueness among unkeyed complete
  semantic values. Nonappend replacements never greedily pair ambiguous equal
  groups. Exact no-ops and true settled-prefix appends preserve duplicates.
- A private typed visitor walks every current payload's strings/containers,
  including chart values, replacing parse-local nested references with shared
  full-value canonical children before typed equality. No payload hash is an
  identity. Invalid reference graphs fail before publication. Parent/child
  paths compare resolved semantics rather than transient token indices.
- Legacy exact no-ops leave existing assignments alone; legacy true append keeps
  settled-prefix assignments and gives new occurrences `None`; legacy
  nonappend replacements supply unkeyed fallback. Explicit key mode changes
  reset the affected occurrence instead of searching historical state.
- Checked process-wide serials make surface/block incarnations independent of
  reusable ECS indices and source bytes. Whole serial ranges are reserved with
  one checked atomic operation, never a partially consumed prefix on refusal.
  Source/identity revisions are separate from reader-state revisions. Source
  movement/no-op does not bump reader revisions; a source/key change or block
  reset advances the source revision. Surface reset creates a fresh handle at
  `LiquidSourceRevision::ZERO` and retires all old descendant authority.
- Complete ordered child values, including existing child IDs, fence indexed
  state and recipe timers. Parent cosmetic edits retain compatible state and
  absolute deadlines. A retained-parent schema reset advances its state fence
  once, including a zero/missing pre-reset state barrier, so an old expected-zero
  write cannot recreate pre-reset indexed state. A **fresh block incarnation**
  after explicit reset/remove/reinsert still begins missing/revision zero.
- Feed/dashboard rebinding retires only incompatible request lanes and clears
  their last-good slots; independent compatible tiles retain their contexts and
  data. Pending and in-flight settlement ownership remains in F1 accounting,
  including cooperative external acknowledgments and finite callback pumps.
  Remounting a settled feed does not manufacture a fresh unprobed request.
- Parked nested surfaces keep logical state, handles, deadlines and observation
  authority even without a physical view. Reattachment stages membership and
  source-index remapping under a live transaction fence before presentation is
  restored. Logical handle state writes/resets do not depend on an Entity being
  mounted. Late feed results and timer completion use captured logical keys;
  contextual envelopes retain those handles while legacy Entity/source fields
  remain historical metadata, not mutation authority.
- Actual interaction roots must still carry the current block lease. Timer due
  batches recheck their actual stored deadline/revision after **each** prior
  callback. Contextual results check live owner/block/request and state CAS at
  final mutation. Reentrant source/state/reset attempts and poisoned owners
  fail closed; no refusal falls back to a coincident source byte or record.
- Only Liquid UI production/tests and this relevant design document change.
  There is no new flow crate, parser/core production change, dependency/lock
  edit, SDK/Medousa migration, durable-turn cancellation, push, publication,
  deployment, seal, approval or integration. The sender retains independent
  assessment/review/fixes and the native lifecycle.

Verification receipts for the completed candidate are appended separately
below. This contract clarification alone is not acceptance or a broad readiness
claim. The existing whole-stream staging performance caveat remains; full-value
correspondence and its worst-case repeated comparisons are not a new performance
qualification, and existing opt-in performance probes remain separately named.

### N.3 Permanent qualification and honest before/after chronology

The continuation began with a **clean** reusable worktree at proof commit
`54f3a15cc3221980a344c7cd4c2dcf24a47fba61`, not an integrated implementation.
`main` remained accepted F1 `d18c55414d30c4c563148b30fd7e096f0bfd863d`.
About 11 GiB was available initially. M's proof assertions and historical
receipts remain intact.

Initial desired-behavior assertions precede production changes:

1. M's exact mounted/offscreen duplicate-group baseline was **2 passed,
   2 failed**, exit 101, with genuine record transfer after nonappend prefix
   insertion. The exhaustive three-variant legacy error fixture passed.
   `target/f2/baseline-identity-final.log` is the retained baseline log.
2. New `tests/keyed_identity.rs` contract assertions were added before production
   implementation. `cargo test --locked --offline -p rndr_liquid_ui --test
   keyed_identity` failed to compile with **47 missing-symbol errors**, exit
   101 (`target/f2/baseline-keyed.log`). This is an absent-API compile baseline,
   not an invented runtime failure count.
3. Permanent expanded cases were added while following affected contracts and
   callers. Only the explicitly recorded runs are baseline evidence; the whole
   final matrix is not retroactively claimed to have executed against F1.
   The initial invalid-vector harness used an empty vector with a zero-occurrence
   plain source, which is valid; that fixture was corrected to the intended
   two-occurrence source. Required payload fields, disabled scroll following for
   offscreen setup, and distinct-context versus repeated-poll counting were
   likewise fixture corrections, not production-defect proof.

`tests/identity.rs` retains the proof and adds two data-driven tables:

- **18 replacement rows × mounted/offscreen = 36 cases**: exact duplicate
  no-op, true duplicate append, nonappend ambiguous prefix insertion,
  contraction/expansion, unique reorder/prefix, semantic edit, mixed unique and
  duplicate groups, keyed duplicate reorder, child-schema versus cosmetic title
  edit, both key-mode switches, key replacement, keyed kind change, pending
  fence append and CRLF-normalized no-op.
- **6 explicit operations × keyed/unkeyed × mounted/offscreen = 24 cases**:
  empty append, completion of a pending fence, block reset, surface reset,
  remove/pump/reinsert and destruction/immediate recreation. These check exact
  handle continuity or freshness, reader record/state/revision, source revision,
  missing state on a new incarnation, stale-token rejection, remount and final
  teardown. They do not substitute a shared saved-state heuristic for offscreen
  rules.

`tests/keyed_identity.rs` checks complete reading-order key vectors, duplicate,
empty/control/257-byte labels, invalid count, atomic key and revision refusal,
kind/domain refusal, schema/CAS reset, foreign worlds, remove/reinsert and
surface/block resets. It also checks split CRLF chunks followed by an exact
normalized full-source no-op for both keyed and duplicate-unkeyed occurrences.

`tests/identity_resources.rs` exercises actual nested controls and resource
owners: parse-index-only shifts versus same-index semantic child changes,
cosmetic parent preservation, absolute recipe deadlines versus duration/schema
reset, compatible feed data/context retention, per-tile partial rebinding,
retained-parent expected-zero ABA fences, parked handle reads/writes/block and
surface reset, late parked feed publication with captured contextual envelopes,
and colliding generated tile IDs with independent async result publication.
Private unit regressions cover whole-range allocation failure, source/state/
request/deadline counter admission, capacity, reference validity, reentry,
unwind poisoning and invalidation of a captured multi-timer due batch after the
first callback resets its schema. Existing F1 queue/fairness/ownership/settlement
regressions remain unweakened.

Additional observed failures were fixed, not hidden by passing-only assertions:

- Nested parse-index reattachment initially retained a handle but lost its
  record while the physical view was extracted. Rebinding now holds its logical
  transaction lease through insertion. Settled feed remount also initially
  retired the old context by treating known data as unprobed; its logical probe
  status now survives projection recycling.
- `target/f2/qualified-crlf.log`: **4 passed, 1 failed**, exit 101. Splitting
  `\r`/`\n` across stream writes then supplying the identical LF-normalized
  source incorrectly advanced source revision **2 → 3** (and could reset an
  ambiguous group). The UI setter now normalizes its saved stream comparison
  and append-prefix admission, without changing parser production. The exact
  same test is **5/5 passing**, exit 0, in `qualified-crlf-after.log`.
- `target/f2/qualified-instance-before.log`: the new focused desired regression
  **failed**, exit 101 (one failure, seven unrelated tests filtered only for
  this reproduction). The unchanged parser can generate colliding child IDs for
  labels `A`, `A`, `A-1`: three distinct request bindings had only **two**
  observation contexts. A private collision-safe instance plan now preserves
  ordinary unique names and disambiguates collisions against all raw/assigned
  names using the current occurrence. Loading, refresh actions and binding
  compatibility consume that same plan. These are **not host block keys or
  per-child reader-state remapping**; parser IDs, payload schemas and parser
  production remain unchanged. `qualified-instance-after.log` passes the same
  focused test, including three independent results, selective retirement and
  zero-effect stale result rejection.

After these last two fixes, every final gate is rerun, including the **entire**
300-cycle oracle rather than relying on an earlier passing suite. Its F2
interleave runs in the original F1 resource World on **every** teardown cycle:
key rebinding, cosmetic preservation, child-schema reset, stale CAS refusal,
block reset and periodic whole-surface reset occur alongside F1 timers, nested
controls, charts, observations, listener captures and external acknowledgments.
Final per-command receipts and limitations follow below.

### N.4 Final frozen-candidate receipts and handback limits

All rows below were rerun **after** the CRLF comparison and collision-lane
corrections. Each exited **0**. No new F2 regression is ignored; the final full
Liquid and selected core commands have no test-name filters. Complete logs are
worktree-local under `target/f2/`; `final-gates-post-fixes.txt` records every exit
status. Build environment for all Cargo commands:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$PWD/target/f2"
```

| Exact command | Observed final result | Log under `target/f2/` |
| --- | --- | --- |
| `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings` | Passed, no warnings | `final-clippy.log` |
| `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture` | Liquid UI **100 passed, 0 failed, 2 existing opt-in ignored**, plus **1 passing UI doctest**; parser **11 passed**, plus **5 passing parser doctests**; no filtering | `final-liquid.log` |
| `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown` | **179 passed, 0 failed, 1 pre-existing ignored**, no filtering | `final-core.log` |
| `cargo check --locked --offline -p rndr --no-default-features --features liquid` | Passed facade with Liquid enabled | `final-facade-liquid.log` |
| `cargo test --locked --offline -p rndr --no-default-features --features liquid --test editing_compatibility` | **2 passed**, no failed/ignored/filtered | `final-facade-compat.log` |
| `cargo test --locked --offline -p rndr --no-default-features --test editing_compatibility` | **2 passed**, no failed/ignored/filtered; facade without Liquid/UI feature | `final-feature-off.log` |
| `cargo fmt --all -- --check` | Passed | `final-fmt.log` |
| `git diff --check` | Passed; repeated against staged/committed range at handback | `final-diff.log` |

A separate audit of the actual final oracle log asserted all of the following,
not just its test-function pass count:

- Exactly **100 unique teardown cycles, numbered 0–99, for each Sim seed
  1, 7 and 42**: **all 300 cycles ran**.
- Every printed 19-counter resource vector equals its per-World baseline after
  **every** teardown. The exact retained baseline is
  `[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]`;
  it is not replaced with an incorrectly assumed all-zero target.
- `external_ack_pending=0` on **all 300** teardown records. F1's active,
  awaiting-host-ack and in-flight settlement accounting and physical ownership
  assertions remain active, with F2 identity operations interleaved every cycle.

Compatibility audits compared the legacy `LiquidUpdateError` and field
layouts of `LiquidInteractionEnvelope`, `LiquidComponentRecord`, `FeedSlot`,
`LiquidTimerRecord` **byte-for-byte** to accepted F1. They are unchanged. Every
pre-existing public function signature in the changed production files remains
present (whitespace-normalized comparison), and all original proof assertions
and the entire historical document prefix from `54f3a15` are unchanged.

The candidate's exact changed-file scope is:

- `crates/rndr_liquid_ui/src/{identity,feed,lib,lifecycle,render,state,timer,view}.rs`
- `crates/rndr_liquid_ui/tests/{identity,keyed_identity,identity_resources}.rs`
- `docs/design/rndr-flow-f1-f2-slices.md`

No parser/core production, facade production, Cargo/dependency/lockfile,
SDK/Medousa or flow-crate changes are included. All Cargo runs used locked,
offline builds and the worktree-local target, which occupied about **2.3 GiB**
at final audit, with about **9.9 GiB** available on the volume. No downloads or
shared build-target commands were used.

**Limits:** this is author-run functional/resource/compatibility evidence, not
independent acceptance or a new production-readiness/performance claim. The
unchanged opt-in `liquid_stream_cost` and `liquid_density_cost` release
characterizations were not run; the prior whole-stream staging caveat remains,
and no new scaling claim is made for full-value correspondence or private
collision disambiguation. The selected core suite retains its one existing
ignored test. Logs are local build evidence, not newly published artifacts.

The complete implementation is committed on the isolated undertaking branch,
retaining proof commit `54f3a15` in its history; the exact candidate OID is in
the terminal handback. `main` remains
`d18c55414d30c4c563148b30fd7e096f0bfd863d`. A local candidate commit is not
acceptance, approval, sealing, integration, push, publication or deployment.
The sender retains independent assessment/review/fixes and the native lifecycle.

## O. F2 correction after independent P2 review of c289747

### O.1 Reproduction before correction and permanent public coverage

The sender accepted the **review deliverable only**, not the implementation.
This correction continues the same undertaking from clean candidate
`c289747aff85d6cb538b61359c28681e0272eac0`; accepted F1/main remains
`d18c55414d30c4c563148b30fd7e096f0bfd863d`. The complete original review,
probe source, commands and logs remain unchanged in
`target/f2/independent-review/`. Fresh correction evidence is separate under
`target/f2/p2-correction/`, including a before/after hash inventory of that
independent evidence. Historical sections above are preserved, not rewritten
as though the old candidate already passed this review.

Before any production edit, permanent public-path tests were added in
`crates/rndr_liquid_ui/tests/source_normalization.rs` and run with:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$PWD/target/f2"
cargo test --locked --offline -p rndr_liquid_ui --test source_normalization -- --nocapture
```

- `before.log`: the exact opening-CRLF repro and initial CRLF-boundary matrix
  both fail, **0 passed / 2 failed, exit 101**. The repro uses the review's
  exact multiline accordion, CRLF wire split at byte **13**, public creation,
  public push of the remainder and public normalized setter. Expected one
  accordion from `LiquidStream::new(a).blocks()`, actual empty.
- `before-all.log`: a separately runnable normalized-prefix-extension test was
  then added, still before any production edit. **All three tests fail,
  exit 101**; the prefix test demonstrates the second unsafe path, not just
  the early return. These are genuine runtime failures, not setup failures.
- After correction, the matrix was expanded to **all 556 combinations** of
  every byte split (therefore including before/inside/after each opening,
  body and closing CRLF), complete/pending fence, keyed/unkeyed setter and
  normalized equality/prefix extension. It checks actual Liquid blocks and
  Markdown source, complete occurrence handles, preservation of two equal
  completed blocks' handles/state/record revisions, new missing-state handles,
  absent pending identities, atomic invalid-key refusal, normalized no-op
  source revisions, and public push completion after a pending setter.
- The exact repro also checks that a real canonical projection repair advances
  the opaque source revision. Repeating a genuine normalized no-op does not.
  Existing `keyed_identity.rs`'s completed-duplicate CRLF no-op regression is
  retained and passes alongside these tests.
- Additional physical coverage exercises keyed and unkeyed repair with two
  mounted controls, or with both controls genuinely released offscreen and
  then remounted. Distinct open/closed duplicate records must survive both
  branches. This exposed a necessary correction in the common replacement
  path: a released native binding could outlive the cleared saved entry and
  remount an empty slot, blocking the renderer's fresh slot. The new test
  failed before that correction (`after-mounted.log`,
  `mounted-diagnostic.log`) and passes afterward (`after-saved-slot-fix.log`).
  The fix is limited to exact Liquid-owned slot handles, described below.

`after-focused.log` records the first three tests and existing five keyed
identity tests passing. `after-expanded.log` is a **fixture-only compilation
failure** from trying to inspect the private revision tuple field; the fixture
was corrected to use public opaque equality/inequality, not an API expansion.
It is not counted as defect evidence. `after-expanded-fixed-fixture.log`
records all 556 cases passing. `after-saved-slot-fix.log` records all four final
normalization tests plus the five existing keyed tests passing. Initial full
gates are retained in `initial-gates/`; the final full rerun after the saved-slot
correction is the qualification that applies to the final production tree.

### O.2 Canonical staging and ownership decision

This is a renderer-side complete-source setter repair, consistent with F0 §7
and slices B/C/D/F/N; it does **not** change raw parser or core production.

1. Both legacy and keyed setters continue to normalize CRLF, build a complete
   canonical `LiquidStream`, validate the complete sidecar and check authority
   before publication. Every successful setter path now installs that staged
   stream. None clones/pushes into the old raw stream to derive setter output.
2. Normalized byte equality alone is not a no-op proof. Actual public block
   projection, prior stream blocks, normalized Markdown output and pending
   language must agree with staging, and the current keys must agree. Only
   when the physical Markdown bytes also agree can presentation be skipped.
   Even then, replace the private stream with canonical staging so latent
   chunk-dependent parser cursors cannot affect the next public push. Handles,
   records, state revisions and source revision remain unchanged.
3. If only physical Markdown CRLF bytes differ, publish normalized presentation
   through the validated common mapping without spuriously advancing the source
   revision or resetting duplicates. If canonical blocks/Markdown/pending state
   actually differ, it is a real projection repair: the checked source revision
   advances, even when the normalized raw source bytes were equal. No new block
   inherits an invalid old pending index or a missing occurrence's state.
4. Normalized true source-prefix extension remains the **identity** append
   exception in the shared resolver, preserving equal completed duplicates and
   inherited keys. The legacy **physical** append fast path is stricter: both
   the old block vector (indices included) and exact old Markdown output must
   be prefixes of canonical staging. Derive its delta from canonical Markdown,
   never from a push into the prior parser. Otherwise use the full canonical
   replacement/remap path with its existing checked admission and rollback.
5. Full replacement transfers saved/mounted/carry state through that same
   logical mapping. Private saved state now retains its exact `MarkdownSlot`
   handle. Once the logical plan succeeds and saved state has moved to carry,
   detach those released bindings as well as mounted bindings; otherwise core
   could later remount a stale empty native slot. Do not clear all host slots
   or sweep application entities. Refusals occur before this detach and leave
   saved bindings, source, records and revisions untouched. Existing F1 parking,
   independent ownership, feed settlement and callback bounds remain in force.

Only `view.rs`, the focused new normalization test and this append-only design
ledger change in this correction. Original three-variant `LiquidUpdateError`,
additive `LiquidF2Error`, legacy signatures and public record/envelope/request
layouts are untouched; there are no parser/core production, Cargo/dependency/
lockfile, SDK/Medousa, facade, new-crate or migration changes.

**Residual boundary:** raw `LiquidStream` and an uninterrupted series of public
raw pushes can still exhibit inherited chunk-sensitive CRLF behavior before a
complete-source setter repairs it. This correction does not claim a general
parser streaming fix. A setter canonically stages its complete normalized
input and installs safe starting state; a later newly split raw CRLF can again
require that boundary. Complete-source parsing, projection comparison and
full-value correspondence retain their whole-stream/worst-case comparison
costs. No release performance probes have been run and no new scaling,
physical-quiescence-under-uncontrolled-producers, or broad readiness claim is
made. Sender assessment and fresh independent re-review remain required before
native approval/integration; this correction performs no seal, approval,
integration, push, publication or deployment.

### O.3 Final correction qualification receipts

The full gate batch was rerun **after the final canonical/saved-slot production
correction and all four final public normalization tests**. Commands ran
serially with the O.1 environment, locked/offline Cargo, worktree-local target,
no downloads/shared-target commands and no lockfile edits. The following exact
commands each exited **0**; complete command files and logs are in
`target/f2/p2-correction/`, with statuses in `results` and the rerunnable batch
in `run-gates.sh`:

| Exact command | Final result | Local log |
| --- | --- | --- |
| `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture` | UI **104 passed, 0 failed, 2 existing opt-in ignored**, plus **1 UI doctest**; parser **11 passed**, plus **5 parser doctests**; **121 total passing**, no test-name filters | `liquid.log` |
| `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown` | **179 passed, 0 failed, 1 pre-existing ignored**, no test-name filters | `core.log` |
| `cargo check --locked --offline -p rndr --no-default-features --features liquid` | Passed Liquid facade compilation | `facade.log` |
| `cargo test --locked --offline -p rndr --no-default-features --features liquid --test editing_compatibility` | **2 passed**, none failed/ignored/filtered | `compat.log` |
| `cargo test --locked --offline -p rndr --no-default-features --test editing_compatibility` | **2 passed**, none failed/ignored/filtered; Liquid/UI feature disabled | `feature-off.log` |
| `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings` | Passed with no warnings | `clippy.log` |
| `cargo fmt --all -- --check` | Passed | `fmt.log` |
| `git diff --check` | Passed; repeated for staged/committed correction range at handback | `diff.log` |

A fresh `python3 target/f2/p2-correction/verify-vectors.py` audit of the **final**
`liquid.log` independently checks the actual logged resource vectors, not just
a test summary. `vector-verification.log` records:

- Exactly **100 cycles numbered 0–99 for each seed 1/7/42**: **all 300 cycles**
  ran on the final production correction, not a shortened prefix.
- Every vector has **19 counters**, and **every counter equals its per-World
  baseline after every teardown**. The retained baseline is
  `[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0]`, not an
  assumed all-zero baseline.
- **`external_ack_pending=0` on all 300 teardown records**, with the permanent
  F2 identity operations still interleaved on every cycle and all F1 physical
  ownership/settlement assertions active.
- The final log also records 556 normalization boundary cases and passing
  mounted/offscreen restoration, legacy exhaustive-match, and all existing
  independent-review/F1 regression tests. No new regression is ignored.

`independent-evidence-before.sha256` and `independent-evidence-after.sha256`
compare **identically**, preserving all independent-review files. Historical
candidate/document prefix and original proof/test files remain intact. The
focused correction contains only `crates/rndr_liquid_ui/src/view.rs`,
`crates/rndr_liquid_ui/tests/source_normalization.rs`, and this appended
ledger. Worktree-local target size is about **2.3 GiB**, with about **9.8 GiB**
available on the volume at final audit. The exact correction commit and clean
status are returned in the durable terminal handback; `main` remains accepted
F1 `d18c55414d30c4c563148b30fd7e096f0bfd863d`.

These are author-run correction receipts, **not independent acceptance**. The
unchanged `liquid_stream_cost` and `liquid_density_cost` opt-in release probes
remain unrun, and the whole-stream staging/comparison performance caveat remains.
The sender retains assessment and fresh independent re-review before any native
approval or integration. No seal, approval, merge/integration, push,
publication or deployment was performed.

## P. F2 explicit-reset remount correction after independent re-review

### P.1 Before/after public reproduction and narrow ownership repair

Continue candidate `ca8672f8581f60096b3b2acc869148ffccbd5c69`, with accepted
F1/main unchanged at `d18c55414d30c4c563148b30fd7e096f0bfd863d`.
The independent re-review deliverable was accepted as evidence only; F2 was
not accepted. Its complete `target/f2/independent-rereview-p2/REPORT.md`, exact
probe source and isolated failure log were read before editing. The newly
shown reset defect is in code unchanged from original F2 `c289747`; it is not
attributed to the canonical setter correction in O. Original F0 section 7 and
slices B/C/D/F/N/O remain the contract. All fresh evidence is separate under
`target/f2/reset-remount-correction/`; prior evidence is not overwritten.

Permanent public tests in `tests/reset_remount.rs` cover eight independently
runnable cases: block/surface reset x keyed/unkeyed x mounted/released-offscreen.
They boot real UI/Markdown/Liquid plugins, open both accordions, verify actual
mounted controls, then (in offscreen cases) scroll and assert actual release.
They use the current snapshot CAS, reset, return onscreen and pump 12 frames.
Assertions cover both actual mounted occurrences and rendered open/closed
labels/defaults, retired-handle query/write rejection, new authority writing
the rendered control, untouched block handle/state/record revision, fresh
surface revision, and application Layout/independent Liquid surface component,
handle and state survival. Control counting excludes separately owned Liquid
surfaces rather than counting their attached descendants as parent controls.
Wrong state CAS refuses in all cases; wrong source CAS refuses in keyed cases;
source bytes, records, mounted set and source revision remain unchanged.

Before any production change:

- `before.log`: 3 pass/5 fail. Four are real offscreen remount failures; the
  extra mounted-unkeyed failure is a fixture counter including an independently
  attached surface's rendered control. It is not product-defect evidence.
- After correcting only that ancestry counter, `before-corrected-fixture.log`:
  **4 pass/4 fail**, exit 101. Each offscreen block/surface x keyed/unkeyed case
  fails because mounted is `[1]` or `[]` instead of `[0,1]` after 12 frames.
- Unchanged exact reviewer source copied temporarily into its original test
  target: `reviewer-before.log` **4 pass/4 fail**, exit 101, reproducing all
  four isolated reviewer failures without production edits.

The production correction is limited to `view.rs::discard_presentation`.
After successful logical reset planning, selected saved entries detach their
**exact released MarkdownSlot** before being dropped. Unselected entries and
bindings remain. Offscreen release already parks semantic children and clears
owned projections; no second traversal or broad native-host sweep is added.
Existing mounted detach/parking, carry invalidation and parked logical reset
handling remain unchanged. Core detach validates the exact view/id/root binding
and supports released slots; no core production change is needed. Refusals
occur before this presentation phase. Separately app-owned surfaces/entities
remain outside logical child retirement and projection reclamation.

After that correction, `after.log` reports **8 pass/0 fail**, exit 0.
`reviewer-after.log` reports **8 pass/0 fail**, exit 0, including all four
remount cases, six mixed-key permutations/18 malformed-map refusals, exact
normalized repair, independent attachment repair, and **2,008 canonical cursor
push comparisons**. The exact unchanged probe source is preserved locally as
`reviewer-probes.rs`; its disposable test file was removed before full gates.
Exact command files accompany the full logs. Both focused commands used:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_TARGET_DIR="$PWD/target/f2"
cargo test --locked --offline -p rndr_liquid_ui --test reset_remount -- --nocapture
cargo test --locked --offline -p rndr_liquid_ui --test independent_rereview_p2 -- --nocapture
```

No parser/core production, facade production, Cargo/lock/dependency, SDK/Medousa
or new-crate changes. Original three-variant `LiquidUpdateError`, additive
`LiquidF2Error`, legacy signatures/layouts and canonical setter/cursor code
are unchanged. The original proof and historical design prefix are retained.

### P.2 Full fresh qualification and handback boundary

The final permanent source/test tree was qualified serially, locked/offline,
with the environment above. `run-gates.sh`, `results`, each exact `*.command`
and complete `*.log` are in `target/f2/reset-remount-correction/`. All eight
commands exited 0:

| Exact command | Fresh result |
| --- | --- |
| `cargo test --locked --offline -p rndr_liquid -p rndr_liquid_ui -- --nocapture` | **129 passed**, 0 failed: parser 11 + 5 doctests, UI 112 + 1 doctest; two existing opt-in performance probes ignored; no test-name filters |
| `cargo test --locked --offline -p rndr_core --lib --test runtime --test feed --test markdown` | **179 passed**, 0 failed, 1 pre-existing ignored |
| `cargo check --locked --offline -p rndr --no-default-features --features liquid` | Passed |
| `cargo test --locked --offline -p rndr --no-default-features --features liquid --test editing_compatibility` | **2 passed** |
| `cargo test --locked --offline -p rndr --no-default-features --test editing_compatibility` | **2 passed**, Liquid/UI disabled |
| `cargo clippy --locked --offline -p rndr_liquid_ui --lib --tests -- -D warnings` | Passed |
| `cargo fmt --all -- --check` | Passed |
| `git diff --check` | Passed; staged and committed ranges also checked at handback |

`verify-vectors.py` audits the fresh `liquid.log`, not historical summaries.
`vector-verification.log` confirms **exactly 300 teardowns**, cycles 0–99 for
each seed **1/7/42**; **all 19 counters equal the per-World baseline and
external_ack_pending=0 after EVERY teardown**. All existing F1 tests and F2
identity interleaving remain active. The full suite also re-runs all **556**
canonical source-normalization boundary cases. The exact reviewer's **2,008**
cursor comparisons passed in the separate after-probe run. Logical resource
settlement does not alone prove remount rendering; the eight new permanent
physical tests supply that separate evidence.

`prior-evidence.sha256` inventories prior author/independent evidence before
correction. The final audit verifies all listed hashes unchanged, the complete
prior design ledger remains an exact prefix, and saved reviewer probe source
is byte-identical to the independent source. Only `view.rs`, the new permanent
`tests/reset_remount.rs`, and this append-only ledger change in this correction.
The exact commit, scoped diff and clean status are returned in terminal
handback; main remains accepted F1 unchanged. No seal, approval, integration,
push, publication or deployment occurs.

These are author-run correction receipts, **not project acceptance or fresh
independent review**. Sender retains assessment, mandatory fresh independent
re-review, revisions and native integration. Inherited raw-push split-CRLF
behavior remains until canonical complete-source repair; this is not a parser
streaming fix. `liquid_stream_cost` and `liquid_density_cost` release probes
remain unrun. Whole-stream staging/projection comparison and worst-case
full-value correspondence costs remain; no broad readiness/performance or
uncontrolled-producer physical-quiescence claim is made.
