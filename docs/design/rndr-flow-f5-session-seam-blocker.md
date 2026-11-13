# F5 correction: public Session integration blocker

> Historical blocker assessment at `9721657`. The principal subsequently
> authorized this narrow seam. Its implementation/contract is now documented in
> `rndr-flow-f5-session-close-validation.md` and its exact-source evidence in
> `../reports/rndr-flow-f5-session-seam.md`. The original reviewer report, probe
> sources, failed logs and assertions remain unchanged; this text below records
> the pre-authorization blocker, not the current implementation status.

## Disposition

**BLOCKED, not demonstrated and not implemented.** The in-scope flow safety corrections can be qualified headlessly, but the existing public renderer API cannot supply the required all-World validation at every destructive close boundary. F5 must not be represented as fully integrated or ready for F6 merely because its private-runner unit tests pass. Sender retains independent re-review and scope assessment. No renderer/runtime/backend/SDK/security/policy changes were made by this correction.

## Existing production API and exact missing access

Inspected unchanged `crates/rndr_render/src/lib.rs`:

- `Session` at 342 owns private `apps: Vec<App>`. Its public methods at 348/353/359 are `new`, `window`, and blocking `run`. `run_apps` is private. The public `run`, hotkey and tray helpers provide no per-session World visitor or close-consumption hook.
- Private `Running` owns live slots. Private `Handler` owns each App. Application systems have only their own App/World; retaining a shared coordinator does not grant simultaneous access to all private session Worlds.
- Private `Running::reconcile_close` at 2700 calls `spawn_windows` first and then `CloseDrain::step` over every slot at 2705. On exit, the queue admission commitment/exit path follows. For independent secondaries, hidden-frame callbacks may run at 2728 **after step**, followed by `slots.retain` at 2732, which destroys manager stores.
- `about_to_wait` at 2911 runs shell/hidden callback batches and calls reconciliation at 2929. `window_event` at 2933 runs a per-slot callback and calls reconciliation at 2949. Both paths require validation after callback/topology changes, not just a hook in one App update.
- Unchanged `close.rs:20–88` requests guards and consumes readiness inside the same step. Secondary active slots are independently marked closed. Primary/quit exit waits for every guard. A flow phase or manually requested guard does not prove that the corresponding secondary runner slot is active.

Thus there is **no real existing public Session integration path** to demonstrate. Including private `close.rs` in tests supplies its actual consumption semantics for headless safety assertions, not the missing public access/scheduling seam. Even an invented pre-step-only callback would be insufficient if hidden callbacks/topology changes can occur between that validation and the later slot removal. Do not work around this using unsafe aliasing, raw World pointers, global registries or a parallel renderer/executor.

## Minimal proposed seam/diff scope for sender assessment ONLY

This is a proposal, not authority or an applied/qualified patch. A separately authorized narrow renderer change would need:

1. In `crates/rndr_render/src/lib.rs`, an opt-in **public Session-owned close-boundary callback/visitor**, carried through existing session construction into `Running`, exposing simultaneous mutable borrows of **all** live Worlds (including hidden, dynamic and unrelated primary Worlds). Provide each participant's actual pending/active close intent and stable identity/primary status; never infer it from an arbitrary manually requested guard. The host constructs `HostWorld`s using existing WorldOwners and calls flow `sync_native`. The callback should be fallible and its failure/panic must refuse destructive consumption rather than opt out or drop the previously owned protection. No dependency on `rndr-flow`, application migration or second document manager is needed in the renderer.
2. In the private reconciliation/`close.rs` coordination, separate request/preparation from destructive consumption as needed. First admit the bounded topology batch and request all closes; finish relevant application/hidden callback batches; invoke the all-World hook; then recheck actual cohort/guards and commit exit or secondary removal with **no application callback/topology batch between validation and consumption**. If callbacks remain after the first guard step, revalidate after them and recompute closed-slot decisions (including withdrawing previously marked `closed` flags). Queue admission commitment still needs its existing atomic boundary. A callback failure must clear tentative destruction decisions and leave the owned attempts frozen for retry/cancel.
3. Tests in the renderer's existing headless close owner (and a public API compile/integration example) should cover both `window_event` and `about_to_wait`, hidden callback mutations after tentative readiness, dynamic topology batches, independent secondaries, jointly closing cycles, application blockers, cancellation, hook error/panic, and no-hook compatibility. Actual Session/GUI evidence is separate from a private-module include.

Expected minimal touched production owners: `rndr_render/src/lib.rs` for Session configuration/all-slot access/reconciliation ordering and, only if necessary, `rndr_render/src/close.rs` for splitting prepare/consume/rechecking tentative decisions. Core/flow public changes beyond the current correction are not assumed. The exact callback signature and lifetime/exit error contract require sender design assessment; this document does not promise a reviewed buildable patch. F6/application wiring and all backend/SDK/security/policy work remain outside scope.

## In-scope safety result versus blocker

The corrected flow algorithm computes a dependency fixed point for the **actually participating native close requests supplied by a correctly scheduled host**, and the unchanged runner tests exercise real independent-secondary consumption over complete manually supplied Worlds. R1's original application-blocked B keeps A alive; free chains/cycles can close; withdrawal revokes readiness. That is meaningful in-scope safety evidence even though standard Session hosts cannot currently install the necessary production hook. It is not permission to fabricate runner participation or ignore an adapter error.

R2 final incoming owner/lease/native identity checks and R3 atomic native admission fixes are complete independently of this seam. Preserve those fixes and require independent re-review while withholding any full F5/native integration claim until the seam is separately assessed and authorized.
