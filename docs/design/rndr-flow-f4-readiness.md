# rndr-flow F4 implementation and qualification

## Status and scope

F3 is **accepted and integrated** at `f16e5572d35450d586dd4f10e00591fa36fb2b04`.
This undertaking implements the five F4 slices authorized after that acceptance.
This is a committed worker candidate for independent sender review, **not F4
acceptance or integration**. See the [report](../reports/rndr-flow-f4.md),
[reproducible gates](../reports/rndr-flow-f4-gates.sh) and
[original raw evidence](../reports/rndr-flow-f4-evidence/summary.tsv).

**Correction status:** original candidate `9e09c575` failed independent review on
R1/R2; review-only descendant `acaa7ede` is preserved. The scoped correction
implements the original acknowledgment/panic contracts rather than weakening
them. See [correction report](../reports/rndr-flow-f4-correction.md),
[fresh gates](../reports/rndr-flow-f4-correction-gates.sh) and
[new evidence](../reports/rndr-flow-f4-correction-evidence/qualified/summary.tsv).
All old implementation and independent review evidence stays unchanged;
new green qualification is worker evidence, not independent acceptance.

The old F3 handoff's “F4 not authorized/implemented” wording is superseded by the
current authorization and implementation, not by reinterpretation of F0/F3.
F0 ownership/mutation contracts, F1 retirement/ack accounting, F2 native identity
and detailed errors, and F3 incremental retirement remain the governing substrate.
No core production source, runtime/executor, facade export, production UI
integration, backend SDK, close/save guard or app migration was added.

## Actual APIs, in the authorized implementation order

### 1. Checked independent request replacement lanes

`src/request.rs` adds:

- `LaneKey(u16)`: typed application keys, not user prose or target lookups.
  `WorkLimits::default()` admits **32 lane records per scope**. Distinct history,
  mode, feed and other application operations use distinct keys.
- `Identity<D> { target, operation, revision }`: `D` is the exact **native**
  domain incarnation. Hosts include document/session/turn identities in `D` as
  applicable; flow does not invent a parallel entity/domain identity scheme.
- `DeliveryMode::{Once, Ordered { recoverable }}`, `Request<D>`, opaque
  `RequestToken`, and `Completion<D,T>`. Tokens contain coordinator authority,
  scope serial, scope activity, lane key/generation and exact World binding.
- `replace_request`: metadata-only admission for already host-managed producers.
  Owned starters below preflight **both** lane and work-slot capacity. Every
  checked refusal happens before changing the old lane or cancelling old work.
  Lane generation is advanced before old handles are signalled by the drain.
  An existing key may replace at lane capacity; unrelated lanes remain valid.
- `cancel_request`: exact lane cancellation without allocating a generation or
  command entry; disposal/cancel can progress at maximum generation.
- `request_is_current`: synchronous ownership/activity/lane/World observation,
  not an executor join or a promise that a once result was not already consumed.
- `suspend_requests -> Result<(), RequestSuspendError>` explicitly returns
  `UnrecoverableStream` before activity invalidation or cancellation. The F3
  `suspend` compatibility API also refuses, using existing `Error::Invalid`.
  Hidden is retained, not suspended. Resume waits for activity cleanup and owned
  handle cancellation/removal attempts, not a fabricated external backend join.

A stream keeps **one lane generation**. Successful chunks require exact native
identity and strictly increasing sequence; duplicate, decreasing, wrong-turn,
wrong-operation/revision and wrapped-to-zero sequences cannot write. Sequence
maximum is a valid final sequence; subsequent non-increasing delivery refuses.

### 2. Bounded concrete handles and owning-World effects

`src/work.rs` adds separate `WorkLimits` without changing existing F3 `Limits`
literals: 32 lanes/scope, **4,096 charged work slots**, 256 effects/World. Configure
positive limits with `configure_work` before any request/work has been admitted.

- `replace_local`: existing `spawn_local`; non-Send futures and `Rc` captures are
  supported. Its single-output task protocol requires `DeliveryMode::Once`.
- `replace_background`: existing injected `spawn`/`Executor`, not a new executor.
- Both take `Future<Output = Result<T, TaskFailure>>`; checked refusal returns
  the untouched future in `Rejected<F>`. Status, charged slot and reserved output
  receipt exist **before** core spawn/poll, so there is no exit-to-publication
  accounting gap. No detach is used. `Completion::transpose` extracts a successful
  task payload before the native gate while preserving identity, sequence and
  the exact receipt; failure does not become a native write/save success.
- `replace_feed`: existing bounded `Feed`/`Sender` with supplied `FeedConfig`.
  Returns request, `CapturedSender`, `ProducerExit`, status. Capture epoch at
  **request start**, never at last send. A replacement creates a fresh feed and
  checked lane, not a generation per chunk. Core queue/latest loss semantics and
  `FeedError` remain authoritative.
- `ProducerExit::run` normalizes a producer closure on its **existing** thread or
  executor; `track` wraps an async producer without adding Send/runtime bounds.
  Keep the guard/wrapper with that exact producer, not with its UI observer.
  Dropping an unstarted/abandoned guard with no known outcome reports
  `DroppedUnknown`, not success. A previously known failure/panic is retained.
  `run` returns its closure's normalized result; it does not overwrite aggregate
  `WorkStatus` evidence from cleanup. `track`, like owned tasks, returns
  `Err(Panicked)` at Ready if aggregate panic is already known. Later cleanup can
  still change aggregate status after either output; neither output certifies
  native commit or backend success.
- `advance_feed(&mut Feed)` guards maximum epoch **before** core `advance` can
  clear queued values. Exclusive mutable feed ownership spans check/mutation.
  The owned replacement path does not need to advance an old feed at all.
- `replace_timer`: existing `after` and manual-clock semantics, with a concrete
  `TimerHandle` and event-lifetime observation. `TimerCompletion::completion`
  retains its delivery receipt. Destruction takes and destroys the actual
  completion under the owned catch boundary, then acknowledges returned/unwound
  destruction; a single payload destructor panic is contained as `Panicked`.
  Inside that destructor exit is still unacknowledged and its receipt is live.
  Caller-retained clones retain their receipt independently of event exit.
  No second timer wheel. Repeated/late native
  `every` ticks are separately qualified through the consumer gate and a bounded
  owning-World cancellation effect; no invented interval scheduling API.
- `own_world_effect` admits a `FnOnce(&mut World)` in the exact World's local
  table. `drain_world_effects(world, budget)` extracts each callback before
  invoking it, outside registry/local-table borrows. It is the consumer seam for
  explicit unsubscribe, **not** a flow-to-UI dependency or captured mutable World.
  A callback may register another bounded effect without losing it.

The existing child-first traversal visits owned handles **before** custom F3
cleanup. Task/feed/timer cancel/drop occurs there. World-specific removal waits
for the owning World's effect attempt/abandonment before custom cleanup. A
lagging World remains pending; another World cannot drain its table. World loss
releases captures and reports unknown/attempt-not-run rather than claiming the
unsubscribe callback executed.

Slots remain charged independently of publication eligibility. Retirement alone,
`is_cancelled`, a quiet runtime, and producer exit while an event remains queued
or published are **not** capacity release. Owned completion receipts retain the
charge until their final actual disposal, including retained event clones.
Reclamation requires both wrapped future/producer/event exit acknowledgment and
zero delivery receipts (including pre-reserved task outputs). Blocked producers/custom jobs remain charged and unknown.
Raw metadata requests and arbitrary external handles are not retroactively
joined; use the owned starter/wrapper protocol for bounded drain claims.

### 3. Exact synchronous mutation boundary

`commit_completion` calls an authoritative `CompletionTarget<D,T>` only after
current ownership/activity/lane/World and captured native identity/sequence
validation. `commit_world_completion` supplies the exact owning World to a
consumer-owned `WorldCompletionTarget`. Implement these only beside the native
writer: synchronous native admission/commit, **no await or reentrant callback**,
and Err must leave domain/history/revision untouched. They are integration
contracts, not permission for projection/selection helpers to become writers.

`commit_source_completion` is a concrete `SourceDocuments` adapter:

- exact native `DocumentId`, Host operation and native batch base revision;
- source-native readonly/composition/revision/history checks;
- conservatively rejects pending IME preedit/commit in earliest Input for that
  exact attached source, before Chrome has observed composition. Focus is read
  only for composition policy, **never** to choose the mutation target;
- ordered chunks supply exact native bases. The lane's native progress advances
  only from that operation's successful native `ChangeEvent::after`; no-op chunks
  consume sequence without manufacturing a revision. Outside edits cannot be
  blessed by supplying their newer base. **No implicit rebase or per-chunk lane
  replacement**. Source and structured document authorities stay separate.

`crates/rndr_liquid_ui/tests/flow_owned.rs` is the consumer-side concrete proof:
existing F2 `put_liquid_component_state_for` checks block/surface incarnation and
record revision in the same synchronous call; exact `LiquidF2Error` is preserved.
Explicit `unsubscribe_liquid_interactions` runs only in its owning World before
custom cleanup and physically drops the listener capture. The consumer adds only
an UI-to-flow **dev dependency**; no production dependency/API/facade was changed.

No plugin is silently installed. App publishes runtime/timer events **before
Input**; a front-of-Update drain alone cannot establish safety. Every mutating
consumer, including Input and already-published events, must use its boundary.
No selected-tab/entity/source-byte/projection target substitution is supported.

### 4. Honest terminal and incremental drain observations

`WorkStatus::observation()` separates `retired`, `cancellation_signalled`,
`cleanup_attempted`, `exit_acknowledged`, `pending_deliveries` (reserved task
outputs as well as queued/published receipts), and
`Terminal::{Running,Succeeded,Failed,Panicked,DroppedUnknown}`.

Logical death is immediate through F3 `Liveness`/`state` and
`request_is_current`; `retired` records the later physical drain attempt.
Aggregate terminal evidence is monotonic: known `Panicked` takes precedence over
`Failed`, then `Succeeded`, then `DroppedUnknown`, then `Running`. Actual-exit
acknowledgment is independent, never fabricated by cleanup, quiet pumps or a
known panic. Task/feed/timer cancellation marks `cleanup_attempted` before
invoking cancellation/destruction, so a caught panic cannot hide the real attempt. `DroppedUnknown` describes abandonment without a better known
outcome; it does not erase failure. `Succeeded` means the wrapped computation
returned with no aggregate failure yet known, **not** that native commit,
save, IO or backend work succeeded. A custom executor that retains a Job then
panics leaves a retired, charged `Panicked` observation without a fabricated
cancellation signal or exit acknowledgment; later polling cannot erase that
panic. Actual exit and final reserved/published receipt disposal release it. Timer/event disposal can be acknowledged with
an unknown firing/result outcome. World-effect acknowledgment is its local
callback/abandonment observation, not a producer/backend acknowledgment.

`work_counts` synchronously reports charged slots, retained handles,
retired-unacknowledged slots and pending deliveries. A retired slot awaiting
**either** producer exit or final receipt disposal is retained. WorkStatus holds
only bounded observation data, not an immortal result/tombstone log.

`pump_work` charges handle visits inside metadata traversal and separately exposes
`PumpWork::ack_visits`. Old F3 metadata/lease/command/restart accounting remains;
when no F4 work exists the F3 path is unchanged. One dead lane is removed per
metadata visit. Ack selection is resumable; no global scan or `Life::live` ancestry
walk is hidden in a budget unit. World retirement schedules another charged pass
when necessary for an earlier application scope targeting that World. Full
command queues do not prevent admitted flags/handles/scopes from draining.

**F3 caveats still apply:** exact `counts`, `work_counts`, activity/request
observations, admission and document-close checks are explicit synchronous work,
not covered by the retirement budget. `pump` includes unbudgeted `counts` after
65,536 units. Externally retained F3 liveness tokens can retain ancestor metadata
after registry counts reach zero; dropping the last token can cause
**depth-proportional destruction**. Callback, payload/future destruction and
executor duration are not bounded. Budget progress is **not a total-latency,
constant-stack token-drop, performance or real-time guarantee**.

### 5. Deterministic race / negative qualification

All paths below have committed automated assertions. `f4.rs` means
`crates/rndr_flow/tests/f4.rs`; other filenames share that tests directory unless
qualified. Many cases run seeds **1, 7, 42**; explicit deterministic orderings,
not random timing/sleeps, establish the others. Native refusal snapshots compare
both targets' text/state, history retention/events and revisions, or equivalent
host write/history/revision counters. Refusals do not merely assert Err.

| Required ordering/negative | Concrete proof |
| --- | --- |
| Before-first-poll cancel/drop; during completion | `f4.rs::local_rc_panic_failure_cancel_before_poll_and_future_drop_are_distinct`; `owner_drops_during_future_completion_and_world_loss_are_not_successful_ui_delivery` |
| Runtime inbox complete then dispose before drain | `controlled_old_finishes_last_custom_executor_and_inbox_cancel_orderings` uses real held core `Job`s/inbox |
| Already-published Event, including Input | `already_published_event_rejected_even_in_input_after_drop`; timer/feed published cases; zero writer/history/revision changes |
| Same logical close/reopen; recycled entity; foreign authority/World | `same_logical_key_reopen_foreign_coordinator_world_and_lag_reject`; `f4_timer_entity.rs::recycled_entity_never_substitutes_recreated_semantic_scope`; consumer wrong-World snapshots |
| Same-owner old result finishes last; independent lanes | controlled held-executor race and `same_owner_replacement_independent_lanes_and_duplicate_zero_writes` |
| Duplicate terminal/chunk; wrong domain/session/turn/op/revision; sequence max | `ordered_native_session_turn_and_revision_identity_never_substitutes_target`; private `f4_tests::default_lane_capacity_exact_native_identity_and_terminal_sequence_refusals_are_untouched` |
| Owned runtime result to native writer | `f4_native_task.rs`: result transpose preserves the pre-poll reservation/identity; native commit once, duplicate rejection, failure without writes, final receipt drain |
| Actual ordered native revision progress, no per-chunk replacement | `f4_source.rs::ordered_source_chunks_advance_only_owned_native_receipts_without_replacement_or_rebase` (two writes, native no-op, duplicates, outside revision and attempted rebase refusal) |
| Feed epoch captured at start; N after N+1 | `captured_epoch_rejects_old_sender_before_mutation`; owned `CapturedSender` never queries a new epoch at send |
| Epoch max before mutation | private production guard seam `work::tests::epoch_max_refuses_without_advancing`; no core counter mutation/2^64-loop claim |
| Queued/merged feed then cancellation; blocked producer | `cooperative_and_blocked_feed_stay_charged_independently_of_eligibility` retains charge across 1,000 quiet pumps and across published receipt retention |
| Producer exit while queued/published payload lives | `f4_receipts.rs::acknowledged_future_cannot_evade_capacity_through_queued_or_published_payloads`: capacity refusal preserves new valid lane; quiet runtime cannot release it |
| Scope/lane/work counter max, capacity, untouched refusal | private lane/serial MAX tests plus unchanged F3 counter/lease-refusal suite; future returned untouched; default 32-lane and 4,096-slot limits exercised |
| Full commands, one-unit progress, callback reentry | private 4,096-slot/1,024-command proof; `f4_timer_entity.rs::extracted_world_effect_allows_bounded_callback_reentry_without_losing_new_effect`; unchanged nine F3 budget proofs |
| Hidden, recoverable suspend/resume; unrecoverable refusal | `hidden_and_recoverable_suspend_restart_activity_not_stream_buffering`; ordered-stream and private untouched suspension tests |
| Cooperative versus blocked custom executor/producer | `f4_producers.rs`: actual external producer closures use channel handshakes (no sleeps), cooperative cancellation versus deliberately blocked work across seeds 1/7/42; held `Job` cancel test retains charge until actual future drop; no backend cancel inferred |
| Background/local panic, explicit failure, cancel/drop; destructor panic | local/background normalization suite; producer/effect panic test; private future-destructor-panic test; `f4_start_panic.rs` retained-Job startup failure; independent cleanup continues |
| **R1: actual timer destruction before acknowledgment; single unwind contained** | Promoted `f4_corrections.rs::timer_payload_destructor_must_be_normalized_before_exit_ack`; `f4_timer_drop.rs` covers 12 normal/panic cases across cancelled/published events, runtime-buffer/World destruction and clone-before/clone-after orders; exact inside-drop receipt/ack snapshots, same-World and independent-World progress, retained receipt charge until final disposal |
| **R2: cleanup panic versus real producer outcome, both orderings** | Promoted `f4_corrections.rs::producer_success_must_not_erase_payload_cleanup_panic`; `f4_terminal_order.rs` covers 20 channel-controlled actual producers: closure and tracked-future success/Failed/Panicked/actual panic/guard-or-future drop, cleanup before/after exit, blocked unacknowledged charge and independent progress; no sleeps or manufactured acknowledgment |
| Aggregate task/effect/timer outcome precedence | `work::tests::aggregate_terminal_evidence_never_downgrades_in_any_order` checks all terminal pairs both orders; tracked pending-drop preserves Failed/Panicked without early ack; existing local/background/start-panic and World-effect tests stay required |
| Timer fired/disposed/replaced/repeated | `timer_manual_deadline_replacement_and_published_disposal`; `f4_timer_entity.rs::native_every_repeated_and_already_published_ticks_are_gated_without_new_timer_wheel` |
| Non-Send Rc local future; injected executor | local Rc suite and actual core `Executor`/`Job` held-executor races (no Tokio/runtime) |
| Native revision/readonly/composition; wrong operation/target | `f4_source.rs` snapshots including pending Input IME; `flow_owned.rs` preserves exact F2 revision/stale-block errors |
| Cross-World lag; earlier app scope targets retiring World | world-local effect ordering plus `f4_receipts.rs::application_scope_before_target_world_is_revisited_after_world_retirement` |
| 100+ cycles/full accounting baseline | **120 cycles × 3 seeds**, complete F3 `Counts` and F4 `WorkCounts` equality after every cycle; **720** task/producer exit acknowledgments. Unchanged F1/F2 300-cycle 19-counter proof rerun separately. |

The max-epoch case injects the adapter's actual scalar preflight; it is not a test
that overwrites private core counters. Ordered application-native target writers
other than the concrete source/UI proofs remain host integration obligations.
The generic trait does not automatically intercept unrelated mutation paths.

## Deliberate remaining boundaries

No required headless matrix row is left as a plan. Qualification is not a proof
of every possible interleaving: fixed seeds plus controlled orderings are the
committed evidence. No OS/native GUI/platform/IME event-loop certification,
release performance measurement, external SDK/blocked-IO termination, arbitrary
destructor double-panic or `panic=abort` recovery is claimed. F1 whole-stream
staging cost, F2 raw-push split-CRLF caveat and F3 retained-token/depth caveat remain.
Structured documents keep their separate native writer; this adds no generic
source-to-structured projection mutation path. F5 close/save/quit protection,
F6 real application workflows, Medousa migration and F7 rollout remain separate.
Nothing is pushed, published or deployed. Sender retains independent review,
corrections and sealed native integration ownership.
