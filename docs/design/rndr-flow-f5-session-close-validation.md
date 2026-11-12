# F5 opt-in Session destructive-boundary validation

This is the narrowly authorized renderer seam, not F6 or an application migration.
It supersedes the **unimplemented** status of the seam in
`rndr-flow-f5-session-seam-blocker.md`; that assessment and its probes remain
historical evidence. The independent report and original failed receipts remain
unchanged. See `../reports/rndr-flow-f5-session-seam.md` for exact qualification.

## Public contract

`rndr_render::Session::close_validator(validator)` installs an owned, main-thread,
`'static`, higher-ranked `FnMut(&mut [CloseWorld<'a>]) -> Result<(), String>`.
The callback cannot retain the borrowed Worlds. No `Send`, extra executor,
backend authority, parallel mutation, new document store, or flow dependency is
added to the renderer. `rndr` already reexports the renderer API.

Each `CloseWorld` contains runner-owned immutable metadata and a mutable borrow
of the **actual** live World:

- `id()` is the native slot's stable Desktop `WindowId`, not a flow ID or an
  application-supplied resource. Map flow identity through that World's checked
  `WorldOwner` and `HostWorld::new`; do not conflate these IDs.
- `primary()` identifies the first Session window.
- `closing()` is the private runner slot's active native close participation.
  It includes every window for a primary close or quit from any window, and only
  actively requested secondary windows otherwise. **A public guard request is
  not participation.**
- `world()` and `world_mut()` access the real admitted World. All live windows
  are supplied, including unrelated, hidden, unguarded and dynamically admitted
  windows, not just the closing subset. Ordering is not a semantic guarantee.

The callback is trusted application code. It must validate its complete live
World cohort and actual closing membership using existing protection owners;
perform final `ProtectionHost::sync_native` after any choices/receipts/other
mutations, and return `Ok(())` only after that validation. It must not remove
or replace guards/Worlds, retain aliases, run updates or make later mutations
that undo its own validation. `Ok` alone never makes a Pending/Failed guard ready.
Unknown or missing required bindings must fail closed, not be omitted. Existing
application-lifetime ownership requirements for shared native managers remain.

An F5 host bridge can collect all `WorldOwner` identities from these borrowed
Worlds, construct the exact `HostWorld` slice, and call the existing
`ProtectionHost::sync_native(&flow, &mut worlds)`. Keep both flow and host in
their existing application-lifetime owners. Keep save/choice UI work before that
final sync. Do not fabricate guard requests to make nonclosing Worlds appear
eligible. The renderer rejects such mismatches before the hook and again during
consumption. If a nonclosing presentation holds a closing manager's document,
the existing greatest-fixed-point protection must leave that manager Pending.
No automatic document transfer is introduced. Save/discard decisions in tests
are deterministic fixture choices, not new application policy.

## Exact owner ordering

Both `Running::window_event` and `Running::about_to_wait` call the same
`Running::reconcile_close`, which uses `CloseDrain::reconcile`:

1. Finish the event's ordinary updates/builds and admit one bounded pending-open
   batch. Recursive opens remain queued; this does not loop until quiescence.
2. **Prepare only**: consume close/quit edges, establish real participation,
   request guards, handle cancellation and clear every tentative secondary flag.
   No destructive decision is published.
3. Service newly needed hidden close frames (visible slots request a redraw).
   Hidden callbacks may mutate, invalidate, cancel or quit here, **not later**.
4. Prepare again to settle that callback batch. If it creates another needed
   close frame/cohort, wake and defer the whole destructive pass. Do not run
   an unbounded callback loop. A cancelled member cancels process exit.
5. Refuse fabricated inactive guard requests. If there is actual close work,
   call the opt-in validator once with **all live Worlds and actual membership**.
6. On `Err` or a caught unwinding panic, clear/no secondary decisions and refuse
   **all** destruction for this pass. Do not cancel requests, release native
   freezes, discard documents, or invent successful receipts. The owner remains
   available for retry, cancel or an informed discard. Error UI/diagnostics belong
   to the application's existing owner; the renderer does not convert the error
   into a successful guard. The panic hook may still print the caught panic.
7. With **no intervening app/build/hidden callback or mutable World/topology
   change**, recompute consumption from final guards and actual slots. New quit
   intent, participation cancellation or a fabricated inactive request refuses
   the entire pass. Secondary `closed` flags are recomputed, never sticky.
8. Immediately remove eligible secondaries, or attempt existing atomic
   `WindowHub::try_commit_exit` for process exit. No callback/build/update occurs
   between final validation and this consumption/removal/queue commitment.
   If opens raced into the queue, retain all process-exit Worlds, wake, admit a
   new bounded batch and repeat **all-World** validation. Admission after the
   successful queue fence is rejected by the existing hub. Destructors are part
   of destruction itself, not a separately scheduled application callback phase.

The callback-free private `step` test harness uses the same prepare/consume
functions for unchanged reviewer probes; production uses `reconcile`, not that
shortcut. The headless tests exercise the real production-used ordering/helper
and the real public `Session` builder. The renderer's internal one-method
`CloseParticipant` trait only permits that common helper to operate without
native/GPU objects in deterministic tests; it is not a public framework.

## Compatibility and fail-closed boundaries

Without a hook, ordinary App/Session constructors and primary/secondary/quit,
CloseGuard pending/failure/retry/discard/cancel behavior remain supported. No
application migration or opt-in is forced. Two safety corrections apply to
guarded sessions even without a hook: inactive fabricated guard requests cannot
bless destructive participation, and later invalidation clears old secondary
closed flags. Hidden close frames now precede consumption rather than follow it.
No-hook operation does **not** gain F5 all-World document protection magically.

Only validator **unwinding** panics are caught here. `panic=abort`, double panic,
arbitrary application/hidden-system panic containment, OS termination and
process failure are not made recoverable. A first panic payload is dropped
under a second unwind fence. If that destructor panics sequentially, ordinary
string secondary payloads are dropped safely; an opaque secondary payload is
intentionally forgotten rather than recursively invoking arbitrary destructors.
This exceptional path can leak that one payload per occurrence; it is not a
universal no-leak or hostile-code recovery guarantee. Nor is malicious trusted validation
code sandboxed. Hook failure retains existing ownership but cannot roll back
arbitrary application side effects the callback itself performed. Existing
freeze rules and unsupported mutation APIs remain as documented in the host
contract. Guard removal/World replacement is not an authorized opt-out from an
in-flight protected close.

## Evidence and remaining limitations

- Original unchanged repro suites are required by the new source-fenced runner:
  original F5 13, independent additions 4, original seam 9 (both formerly failing
  safety assertions plus the unchanged seven runner tests).
- New deterministic tests exercise the public Session hook, actual shared helper,
  hidden mutation/invalidation order, late cancellation/quit, Err/panic refusal,
  no-hook compatibility, dynamically admitted Worlds and the existing queue
  fence. A 90-coordinate native `ProtectionHost` matrix varies all six iteration
  orders, chains/cycles, outside blockers, callback withdrawal, primary close,
  secondary close and quit. Hook Err/panic probes retain actual native readonly
  freezes and exact decisions. Earlier R1–R3 and F1–F4 proofs remain required.
- These are **headless tests plus source trace of both real event routes**, not
  executions of winit's native callbacks or certification of OS window/IME,
  interactive dialogs, GPU surface teardown, durability/performance or backend
  quiescence. A renderer library/default-feature check is not GUI certification.
- The nine inherited ignored core/UI cost/diagnostic tests remain unrun and
  explicitly listed by the auditor. Renderer GPU/pixel/integration suites beyond
  the explicitly selected public API test are not claimed run, including its
  separate ignored shaped-editor release-cost probe. No timings are certified.
- F6/application migration, policy/backend changes, runtime/executor changes,
  approval, integration, push, publication and deployment are excluded. Sender
  retains independent re-review, corrections and sealed native integration.
