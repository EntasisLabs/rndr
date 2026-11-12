# rndr-flow

Optional, headless ownership metadata for an application-owned rndr world.
Depend on package `rndr-flow` and import `rndr_flow`; there is no automatic
facade export or plugin. Its only runtime dependency is feature-off `rndr_core`.

```rust
use rndr_core::World;
use rndr_flow::{CleanupKind, Coordinator, Limits};

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
while flow.counts().pending_scopes != 0 || flow.counts().pending_cleanups != 0 {
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
Metadata scans are bounded by admitted capacity but not time-sliced; no latency
claim is made. See [F4 readiness](../../docs/design/rndr-flow-f4-readiness.md)
and [verification report](../../docs/reports/rndr-flow-f3.md).
