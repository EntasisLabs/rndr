# rndr safe-close implementation evidence

## Exact scope / revision

- Implementation baseline: `a3a3c70bc1038eefc24bfc71265a092d19077c10`.
- Local branch: `worktree/rndr-draft-safe-window-closing`.
- Implementation/review commit: the commit containing this report and
  `crates/rndr_core/src/close.rs`; obtain its exact OID with
  `git log -1 --format=%H -- crates/rndr_core/src/close.rs` (also reported in the
  sender handoff). No integration, approval, merge, push or publication occurred.
- `/Users/theelevators/blooey` is an empty initializer at
  `99fd1a666cf88be0c596160e65a490c9ac8717aa`; its preserved source worktree is clean
  at `b38b370a5e28fa51930e75550e798ce60b005fe6`. Both were inspected read-only.
  No Blooey source, journal, notes, branch or worktree was changed or launched.
- Existing rndr main/editor/readiness/text worktrees were inspected read-only.
  No sibling changes were merged or modified. See `scope.log` for refs.

## Code and integration contract

- `rndr_core::CloseGuard`, `CloseRequest`, `CloseState`: opt-in, asynchronous,
  UI-thread-owned gate with Pending/Failed/Ready state, retry/cancel/informed
  discard authorization, cross-gate unique tokens and stale-result rejection.
- Renderer coordinates local secondary close versus process-wide drain. An
  unguarded primary cannot destroy guarded secondary windows. Windows opened
  during drain join it. A cancellation in any participant cancels process exit.
  Quit requests are consumed to prevent an immediate re-close after cancel.
- macOS native Quit is intercepted only for guarded sessions through the existing
  winit delegate's optional termination-decision selector; no replacement of its
  other lifecycle/input behavior. This closes a bypass found during inspection.
- See `../docs/safe-close.md` for the complete public contract and exact Blooey
  integration recipe, including capture after Chrome editing, all-draft epoch
  validation, serialized/joined pending writes, persistent errors and confirmation
  before discard. The consumer must freeze edits during Pending/Ready or
  invalidate every mutation before Ready is consumed. The framework cannot
  infer application revisions or whether a human confirmed discard.

## Commands / results

All commands ran in the isolated rndr worktree. Builds were offline/locked, used
2 jobs, disabled incremental compilation and debug info, and used only this
worktree's `target` directory:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
cargo fmt --all -- --check
cargo test --locked --offline -p rndr_core -p rndr_render -p rndr
cargo test --locked --offline -p rndr_core -p rndr_render --no-default-features
cargo clippy --locked --offline -p rndr_core -p rndr_render -p rndr --all-targets -- -D warnings
cargo build --locked --offline -p rndr_render --example safe_close
python3 scripts/test-safe-close-native.py evidence/native-verified
git diff --check
```

| Check | Observed result | Receipt |
|---|---|---|
| Default affected-crate regression, integration tests and doctests | **411 passed**, 0 failed/ignored, 43 suite result lines | `regression.log` |
| No-default-feature core/render regression and doctests | **387 passed**, 0 failed/ignored, 41 suite result lines | `no-default-features.log` |
| Strict Clippy, all targets in affected crates | Exit 0, no warnings | `clippy.log` |
| Native example build | Exit 0 | `native-build.log` |
| Final macOS native harness | **6 scenarios passed**, exit 0 | `native-verified-results.log`, `native-verified/*.log` |
| Formatting and diff whitespace | Exit 0 | Tool receipts; exact commands above |

The two regression counts overlap; they are not 798 distinct tests. Ten focused
new close tests passed (3 core state/token tests, 7 renderer coordinator tests).
They cover actual ENOTDIR IO failure, retained exact in-memory text and no exit,
pending success/cancel/retry/discard, stale and duplicate success rejection,
post-success mutation invalidation, ordinary app behavior, secondary/process
scope, cancellation of a multiwindow exit, and windows opened during drain.

The first native-example build caught a test-driver-only use of a nonexistent
World removal API; the driver was corrected to never install a guard for ordinary
apps. A later fmt check found a long probe log line; it was formatted and the
check, build, strict lint and final native suite were rerun successfully. Neither
was a remaining product defect or an unreported failing verification.

## Real native macOS behavior (not unit-test stand-ins)

Environment: macOS 27.0.1 (26A434), arm64, Rust/Cargo 1.98.0. See `environment.log`.
Accessibility permission is **available** (`AXTrusted=true`). Screen Recording
is **not available** (`screenCapture=false`). No permission changes were made.

1. `ax-close-retry`: Swift pressed the actual native AX close button for the
   synthetic probe PID (AX status 0). The app remained alive while a background
   write was pending and after real ENOTDIR failure, preserving exact UTF-8
   draft bytes. Another native close did not discard or silently retry. Explicit
   retry wrote/sync_all'd the exact bytes before normal exit.
2. `native-close-discard`: AppKit `performClose:` reached the real close delegate;
   failed protection retained the process. The harness explicitly authorized
   synthetic discard for the current failure, then the app exited without a
   recovery file. This is API/native-path proof, not a human confirmation UI test.
3. `native-close-cancel`: cancellation while the worker was pending restored Idle.
   The late real worker result was rejected (`accepted=false`); process and exact
   draft survived. A new close got a fresh token, failed again and required an
   explicit synthetic discard to exit.
4. `native-quit-retry`: AppKit `NSApplication.terminate:` used the guarded native
   termination decision instead of exiting behind the gate; pending/error and
   duplicate Quit retained the process, then successful retry permitted exit.
5. `hidden-close-retry`: the window was hidden before native close. Hidden frame
   pumping and real background completion still drained correctly without an
   immediate exit or a stuck protection request.
6. `ordinary-close`: **unguarded negative control** deliberately injected the same
   recovery failure, then exited on native close with only in-memory synthetic
   bytes and no recovery file. This reproduces the original loss mechanism and
   proves compatibility for apps that opt out. It is not a claim to have run the
   old Blooey binary or baseline renderer revision.

Recovered synthetic bytes were compared exactly by both probe and harness:
`Synthetic unsaved draft α\nexact bytes, no live notes\n`.
SHA-256 of `ax-close-retry/recovery`:
`86ff1d09147c3f2ad13c32fa973ab570ad30377dc200f7af8eed7bf549cfb808`.
Scratch files are local/ignored; final logs are committed. The harness refuses
reused scenario directories: for independent reruns use a fresh output path,
for example `evidence/native-independent-review`.

## Remaining acceptance / risks

- **No visual signoff:** Screen Recording is denied. Actual window close/IO/task
  behavior was exercised; screenshot, appearance and human-dialog usability were
  not accepted. The probe is a synthetic single-window app, not Blooey.
- Multiwindow scope/races are verified through the exact runner coordinator in
  headless tests; there is no claim of a native multiwindow visual acceptance run.
- Blooey adoption, recovery retention and integrated native acceptance are separate
  authorized undertakings and remain untouched. Rich Live/Write remains separate.
- Protection cannot survive Force Quit/SIGKILL/crash/power loss on its own. A worker
  hang/panic leaves the gate pending, not an unsafe automatic exit. App timeouts,
  persistent failure UI, write serialization, revision checks, informed discard
  UI and actual durable recovery/save semantics remain consumer responsibilities.
- The native Quit hook is checked against locked winit 0.30.13 and fails closed
  on an unexpected pre-existing termination decision method. Reassess on upgrades.
  Full deferred OS logout/shutdown negotiation is not implemented; native OS
  termination is cancelled while rndr performs its own orderly drain.
- Disk checked before/through/after bounded builds: 13 GiB free initially, 11 GiB
  finally, isolated target 1.8 GiB. No shared caches/worktrees were cleaned.

The sender retains independent review and integration responsibility. This
execution does not approve or merge its own undertaking.
