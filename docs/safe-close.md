# Draft-safe window closing

This is an **opt-in upstream rndr API**, not Blooey integration or approval.
`rndr::{CloseGuard, CloseRequest, CloseState}` re-exports the headless core API.
No guard means the old behavior: primary close exits, secondary close removes
that window, and `Input::quit` exits the process.

## Contract

Insert `CloseGuard::default()` in each protected window's world **before run**.
The runner turns native window close and `Input::quit` into a request. Closing
the primary, Quit from any window, and native macOS Quit drain **all** windows,
including hidden windows and windows opened during drain. Closing a secondary
alone drains only that window. No participating app/world is dropped while any
gate is Pending or Failed. A cancel in any participant cancels process exit.

Queued `Desktop::open` requests are participants too, even when queued by the
frame completing the last guard or by a hidden update. Before checking guards,
the runner admits a batch of queued windows. Opens recursively queued by builders
prevent exit and are admitted on a later bounded event-loop pass; every admitted
guard is rechecked. Exit commits only when all guards authorize and an atomic
queue check closes admission. Requests racing that commitment either join the
drain or occur after shutdown is committed (post-commit opens are ignored).
No application update/build callbacks run after commitment. An endless stream of
new windows can keep shutdown pending; it cannot authorize losing their drafts.

| State | App responsibility | Runner behavior |
|---|---|---|
| Idle | Normal editing | No active protection attempt |
| Pending(token) | Freeze mutations; drain protection once for this token | Retain window/world and continue frames/tasks/timers |
| Failed(token, error) | Keep persistent warning; offer Retry, Cancel, informed Discard | Retain window/world; repeated close does not retry or discard |
| Ready(token) | Remain frozen until exit or cancellation | May close as soon as all affected gates are Ready |

- `request()` coalesces duplicates, including after failure. Normally the runner
  calls this; programmatic apps request closing with `Input::quit`.
- `complete(token, Result<(), String>) -> bool` accepts only the current Pending
  token. `Ok` means the app has actually protected every affected draft, **not**
  merely queued IO. Errors never authorize closing. Duplicate, cancelled,
  superseded and foreign-gate results return false.
- `retry(failed_token) -> Option<CloseRequest>` starts a fresh attempt only for
  the current failure. IO should be idempotent and writes serialized by the app.
- `cancel()` abandons closing, not necessarily IO, and invalidates its results.
  `Input::quit` is consumed by the runner, so cancelling does not re-arm it.
- `discard(failed_token) -> bool` is separate from protection success. Call only
  after the app explains exactly what remains unprotected and the user confirms
  losing it. It authorizes only the exact displayed failure. The framework does
  not invent a dialog or claim that a confirmation occurred.
- `invalidate()` creates a fresh Pending token if closing. **Tokens do not detect
  edits.** Freeze editing (including paste/IME/commands/programmatic changes)
  during Pending/Ready, or invalidate on every mutation *before* the runner can
  consume Ready. Protect every participating window's latest revision.
- Keep the guard installed; do not replace it while closing. Gate state and
  completion are UI-thread-owned. Use `spawn` output events to return worker
  results; do not move the world into a worker. Keep/detach the task handle
  deliberately. A panic, hung worker or lost result leaves Pending, never an
  implicit successful exit; the app should offer cancel and a timeout/error UI.

The runner does not block the event loop, poll IO, impose a timeout, save text,
claim durability, or kill a task. A successful gate grants permission to destroy
in-memory state; durable storage semantics remain the application's contract.
SIGKILL, process crash, forced logout/power loss and external code calling
`std::process::exit` are not vetoable. Recovery retention remains separate work.

### macOS Quit

winit 0.30.13's `windowShouldClose:` emits `CloseRequested` and returns false,
but the default Quit menu calls `NSApplication.terminate:` directly. Guarded
sessions therefore add the optional `applicationShouldTerminate:` method to
winit's existing delegate (not a replacement delegate). It cancels immediate
AppKit termination, posts a runner Quit event, and drains normally. With no
protected windows it leaves normal termination unchanged. The callback is
cleared when the loop returns. Unknown future delegates that already implement
this selector fail closed at installation rather than silently bypass protection.
This is deliberately pinned/tested against the locked winit version. Review this
hook when upgrading winit. Force Quit remains out of scope. OS termination
requests are cancelled while rndr performs its own orderly drain; this is not
an implementation of macOS's full deferred logout/shutdown reply protocol.

## Blooey integration recipe (separate undertaking)

Read-only inspection of `/Users/theelevators/blooey` and its preserved
`b38b370` worktree established the precise gap:

- `native/src/model.rs`: `Draft::unprotected()` means dirty bytes whose current
  `epoch` is not `recovery.durable_epoch`. Save/recovery tickets already isolate
  stale note/epoch results.
- `native/src/main.rs:retry_recovery`: `Sandbox::journal` failure is passed to
  `Notes::finish_recovery`. `update` displays `DRAFT IN MEMORY ONLY ... Do not
  close`, but there is no close-veto API in that pinned rndr.
- At upstream baseline `a3a3c70`, `rndr_render::Handler::window_event` immediately
  called `event_loop.exit()` on primary `CloseRequested`, independent of that
  state. Thus a real journal error plus close necessarily destroyed the only
  in-memory draft. This is source-level reproduction of the **Blooey** gap;
  Blooey itself was not launched or changed in this undertaking. The native
  synthetic probe separately reproduces the failure and verifies the fix.

Consumer steps:

1. Adopt this independently reviewed upstream revision and insert a guard before
   `run`. Do not merely change the warning string or register an unload callback.
2. On first Pending, suspend all editor mutations immediately. Flush any already
   committed active editor `Buffer` into `Notes`, preserving text/undo state.
   Blooey's Chrome editing runs in PostUpdate: put final capture and protection
   decisions after those editing systems, not an earlier Update snapshot.
   Account for *all* drafts/editors, not just the selected path. A later editing
   system must invalidate the gate if freezing is not absolute.
3. Track the close token plus per-note epoch/ticket. Join existing pending saves
   and recovery writes, rather than starting competing writes. Preserve each
   original-byte conflict check. Serialize writes; rejected close completions
   do not prevent an old writer from touching disk. Existing Notes ticket checks
   are still needed. Do not delete old recovery data to claim close safety.
4. For every `unprotected()` draft, attempt recovery or an explicitly requested
   save. Feed results through `finish_recovery`/save's existing ticket checks.
   Once pending work is drained, recheck **all** draft epochs/protection states.
   Call `complete(token, Ok(()))` only when none is unprotected and no pending
   operation can invalidate that conclusion. The close token is not a substitute
   for per-note revision validation.
5. On any storage/conflict/protection error, call `complete(token, Err(detail))`.
   Keep per-note errors persistent across selection/view changes; show the close
   failure prominently. Retry obtains a fresh gate token and rechecks all drafts.
   Cancel retains all text and restores editing without clearing recovery warnings.
6. Offer explicit informed Discard only for the current Failed token, listing
   every in-memory-only draft. Confirm before calling `discard(token)`. If the
   user chooses to resume editing after failure, cancel closing first (or
   invalidate every mutation). Never translate another OS close into discard.
7. Integrate Blooey recovery retention separately, then run synthetic/copied-note
   browse/edit/switch/save/close/reopen, pending-write/failure/crash scenarios and
   exact-byte comparisons. Native/visual and rich Live/Write product acceptance
   are still separate. This upstream commit does not make Blooey ready for live
   notes.

## Verification and reproducibility

Use the locked toolchain, no network or shared target modifications:

```sh
# Disk guard before/after builds; observed 13 GiB initially, 11 GiB after checks.
df -h .
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
cargo fmt --all -- --check
cargo test --locked --offline -p rndr_core -p rndr_render -p rndr
cargo clippy --locked --offline -p rndr_core -p rndr_render -p rndr --all-targets -- -D warnings
cargo test --locked --offline -p rndr_core -p rndr_render --no-default-features
cargo build --locked --offline -p rndr_render --example safe_close --example queued_close
python3 scripts/test-safe-close-native.py evidence/native-review
python3 scripts/test-queued-close-native.py evidence/queued-native-review
```

The native script refuses to reuse a scenario directory. Supply a fresh evidence
path on reruns. It launches only the synthetic probe, scoped by PID. The first
scenario requires Accessibility permission and presses the **actual AX close
button**. Other scenarios call AppKit `performClose:` / `terminate:` from the
probe's UI thread; they are native delegate checks, not fabricated unit events.
The background task waits 600 ms, then really fails IO by writing beneath a
regular file (`ENOTDIR`), or on retry writes and `sync_all`s the exact synthetic
UTF-8 bytes. The harness proves no process exit through pending/failure,
duplicate-close coalescing, late completion after cancel, retry/discard, hidden
window drain and ordinary-app close. The unguarded ordinary-app negative control
also injects a real recovery failure and intentionally closes with synthetic
bytes only in memory, reproducing the loss mechanism without running or modifying
the preserved Blooey app. It is not a human failure-dialog usability check. It writes only its fresh synthetic scratch directories.

The single-window driver waits for an exact sequence acknowledgment after each
command, so duplicate close is executed before Cancel rather than overwritten.
The queued-open driver runs 12 real event-loop scenarios: visible/hidden final
completion opens, recursive build/update opens, real IO failure retention, then
exact-byte protection or explicit synthetic discard. No manual slot insertion is
used. The original independent P1 reproduction and failing log are retained in
`evidence/queued-review/`; see `evidence/safe-close-correction.md` for the old/new
comparison and exact correction results.

Core tests cover token/state rules; renderer tests exercise the same close
coordinator used by the actual event loop, including injected IO failure,
post-success mutation/invalidation and process-exit protection of secondary/new
windows. Unit tests are **not native or visual acceptance**. Retained command
results and native logs are under `evidence/`; see `evidence/safe-close-results.md`
for exact counts, environment/permissions, scope, and remaining acceptance limits.
