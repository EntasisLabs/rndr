# rndr-flow

Optional, headless ownership metadata for an application-owned rndr world.
Depend on package `rndr-flow` and import `rndr_flow`; there is no automatic
facade export or plugin. Its only runtime dependency is feature-off `rndr_core`.

```rust
use rndr_core::World;
use rndr_flow::{CleanupKind, Coordinator, Counts, Limits};

let (mut flow, application) = Coordinator::new(Limits::default()).unwrap();
let mut world = World::new();
let window = flow.register_world().unwrap();
let window_scope = window.scope();
window.bind(&mut world).unwrap();
let entry = flow.create_scope(window_scope).unwrap();
flow.register_cleanup(entry.id(), CleanupKind::Lifetime, |_| Ok(())).unwrap();

// Drop signals logical death only; the host explicitly pumps physical cleanup.
drop(entry);
flow.pump();
drop(world);
drop(application);
while flow.counts() != Counts::default() {
    flow.pump();
}
```

Keep the coordinator outside participating Worlds. Bind each World registration
only in its own World. Entity-bound owners are terminal on removal: immediately
drop the component returned by `World::remove`, rather than moving or retaining
it as an unsupported detach/reparent operation. A rejected registration/binding
returns the untouched submitted value; the caller must clean it.

Documents are application/manager-owned metadata, not document bytes or history.
Presentation leases neither close the last document nor add another disposal
parent. Explicit document close refuses active presentations. Hidden retains
resources. Suspension requires an explicit durable-recovery assertion and only
retires activity cleanup; resume waits until that cleanup was attempted.

Disposal closes descendant admission immediately. Cleanup runs children first,
siblings in creation order, reverse registration within a scope, with a callback
budget per pump. Callback errors/unwinds are recorded in a bounded diagnostic
ring and independent cleanup continues. Coordinator Drop releases captures but
never invokes host callbacks: explicitly drain before dropping it.

Counts describe metadata and cleanup attempts, not acknowledged worker/IO drain.
There are no task/feed adapters, request replacement lanes, completion gate,
executor, backend cancellation, guarded save/close or UI adoption in F3.
See [F4 readiness](../../docs/design/rndr-flow-f4-readiness.md) and the
[correction report](../../docs/reports/rndr-flow-f3-correction.md).

## Incremental retirement versus exact observation

`pump_work(budget) -> PumpWork` is the bounded retirement API. A zero budget is
a strict no-op; a positive budget charges every traversal node/edge, cleanup
entry (including ineligible lifetime entries during suspension), lease selection
or ancestor edge, traversal restart/revalidation and FIFO command. Callback
invocation is included in its selection unit and also reported in `callbacks`;
no call invokes more than `Limits::cleanups_per_pump` callbacks. Empty-cursor,
lease-wrap and callback-cap retry attempts also count. There is no hidden
`Counts` scan, full child lookup, recursive traversal or ancestor liveness walk
in this API. Child and cleanup order are indexed at admission; traversal uses
constant cursor storage, not a newly allocated retirement queue. Command and
diagnostic rings are preallocated at construction.

Retirement uses a resumable child-first walk and independent lease cursor,
alternating traversal and lease steps. An owner drop or first explicit disposal
sets an allocation-free coalescing flag. A charged restart from the root must
revisit newly dead ancestors' earlier children before more callbacks. Repeated
terminal requests do not keep resetting progress. Suspension also requests immediate charged revalidation to preserve child-first
activity cleanup when a child is suspended between pumps. Creation and
registration request a later pass instead of preempting the current pass, so
live admission churn cannot continually restart pending retirement. Retained lease
objects on dead scopes do not keep physical lease slots alive.

With a finite admitted workload/ordering changes, repeated positive-budget
pumps and returning/unwinding callbacks eventually reclaim all dead scopes,
callbacks and inactive leases, even with a one-unit budget, full command queue,
no callbacks, or saturated metadata. Lease progress is not reset by traversal
revalidation. Continued newly signalled ancestor deaths may require repeated
order-preserving rescans, as can repeated new suspensions; there is no fixed
completion bound under an endlessly
changing workload, nonreturning callback or a host that stops pumping.

Each work unit does a constant number of ordered-table operations, each
O(log admitted capacity). A traversal pass takes O(scopes + cleanup entries)
units; a lease pass takes O(leases * (depth + 1)) units. A death-triggered restart
can repeat a prefix (worst case a full traversal); this repeated work is charged.
These are deterministic work bounds, **not real-time latency guarantees**.
Callback/destructor duration, allocator/deallocator and platform costs are not
bounded by the work-unit count.

In particular, external `Liveness` tokens (and retained owners) keep crate-owned
`Life` metadata alive through strong-parent `Arc` ancestry even after registry
`Counts` reach zero. Releasing the final token can recursively destroy O(depth)
internal nodes, including when a cleanup callback releases a captured token;
this is not merely time spent in custom user destructors. External retained
ancestry is not included in `Counts`. Ordinary callback-free child-first registry
removal keeps the parent registry reference alive and does not imply such a
last-token cascade or a hidden pump full scan. No stack overflow was demonstrated;
token release is not promised stack-safe or hard-real-time. Stronger bounded
or stack-safe token destruction would require separate reclamation work.

Exact logical counts cannot be maintained by synchronous counters alone because
owner/lease drops only signal external flags. `counts()` remains an explicit
synchronous observation: O((scopes + leases) * depth + cleanups), plus table
lookups. It reflects unpumped drops and physical retained slots without caching
or weakening pending counts. During shutdown after root disposal, drain until
**all physical counts including leases and queued commands are zero**, not only
pending scope/callback counts: lease sweeping can finish later. As before, it
is not a transactionally atomic
snapshot against concurrent token drops. Compatibility `pump() -> Counts`
performs at most **65,536 retirement units** then calls this **unbudgeted exact
observation**; hosts needing bounded retirement should call `pump_work` and
choose when to observe separately. Exact document-close refusal also scans
leases synchronously. Lease admission at capacity observes until it finds one
inactive slot to reclaim (O(leases * depth)); it never calls callbacks or runs
a global retirement sweep. After stale/invalid validation, admission preflights
serial exhaustion without mutation; only successful capacity checks permit
reclamation and serial commitment. Neither Exhausted nor Capacity refusal
consumes a serial or changes retained storage. Otherwise inactive physical slots remain charged
until incremental sweeping. Single-scope admission/liveness APIs still walk
ancestors synchronously. None of these observation/admission APIs claims a
retirement-budget or real-time bound.
