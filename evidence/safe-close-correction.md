# rndr queued-participant safe-close correction

## Revision and scope

Correction parent: `7cf316074774fdc60a3fb6d4d76a363ec165f18c`, which independent
review rejected for P1 queued-window draft loss. The correction revision is the
local commit containing this report (`git log -1 --format=%H -- evidence/safe-close-correction.md`);
the exact OID is also in the sender handoff. This report supersedes the original
candidate's new-window safety claim, not its historical test receipts.

Only the admitted `worktree/rndr-draft-safe-window-closing` worktree was changed.
No Blooey/sibling worktree edits, live notes, merges, approvals, pushes,
publication, deployment, credentials, spending, or cleanup. No child workers.
The sender retains fresh independent review and integration responsibility.

## Defect and correction

An Update could queue `Desktop::open` and complete the last admitted guard in the
same frame. `reconcile_close` inspected only current slots and called irreversible
`event_loop.exit()`. A later callback could build the queued window after that
commitment; its Idle guard never participated and its draft was dropped.
Hidden updates after the initial spawn and opens recursively queued by builders
had the same missing-admission boundary.

- `Running::reconcile_close` now admits a queued batch **before** checking every
  admitted guard. This also catches opens produced by the latest hidden frame.
- `WindowHub::try_commit_exit` atomically checks queue emptiness and seals open
  admission under the same mutex used by `Desktop::open`. A concurrent open is
  either queued before commitment (blocking it) or after committed shutdown.
  Post-commit open requests are ignored, explicitly documented. No application
  code runs between final guard validation and commitment.
- Recursive builder opens remain queued and prevent commitment. They are admitted
  on subsequent bounded event-loop passes, not an unbounded recursive loop. The
  next pass revalidates all guards, not just the newly admitted window.
- After `exit()` commits, resumed/user/window/about-to-wait callbacks return
  without running application updates or builders. Finishing a native callback
  batch cannot mutate Ready worlds or admit windows behind the committed drain.
- The native driver now publishes an atomic exact-sequence acknowledgment only
  **after executing** a command. The Python harness waits for it before sending
  another command. Duplicate close cannot be overwritten by Cancel in the shared
  command file. `native-correction-final/native-close-cancel.log` proves ordered
  acknowledgments 1 through 6 and rejection of the real late worker result.

Ordinary/secondary scope, guard state/token rules, mutation invalidation and
macOS native Quit behavior are preserved. Existing headless coordinator tests
and all six native checks were rerun; this correction does not replace them.

## Durable independent reproduction and old/new proof

`queued-review/original-repro.rs` is an exact copy of
`/tmp/rndr-queued-open-review.rs`; matching SHA-256 values are recorded in
`correction-environment.log`. Both independent loss receipts are retained:

- `queued-review/original-first-loss.log`: late window built, exact synthetic
  draft dropped, run returned. Its normal return was **not** safety success.
- `queued-review/original-failure.log`: final intentional assertion, exit 101,
  after the same unauthorized drop.
- `queued-review/original-corrected-final.log`: unchanged reproduction linked to
  the final corrected renderer stayed alive for three seconds, late guard
  Pending, no draft drop or run return. This original probe has no authorize
  path, so the harness then terminated **only that synthetic process**. This is
  bounded retention proof, not orderly shutdown proof.

The new `queued_close` example uses the **real winit/native event loop** and
`Desktop::open`, never manual slot insertion. Before rebuilding product code,
it was linked with the unchanged parent build artifacts and run in three modes.
All deliberately failed with **exit 101**:

| Old mode | Evidence | Failure |
|---|---|---|
| final-completion leaf | `queued-review/old-leaf.log` | built draft dropped without authorization |
| recursive builder | `queued-review/old-recursive-build.log` | queued participant never built |
| recursive update | `queued-review/old-recursive-frame.log` | built draft dropped without authorization |

The old artifact invocation was (the probe was subsequently rustfmt-formatted):

```sh
rustc --edition=2021 crates/rndr_render/examples/queued_close.rs \
  -L dependency=target/debug/deps \
  --extern rndr_core=target/debug/deps/librndr_core-6203380e6fb25308.rlib \
  --extern rndr_render=target/debug/deps/librndr_render-7873ea1deaf66207.rlib \
  -C debuginfo=0 -o target/debug/examples/queued_close_old
# Before the correction build, for each mode leaf/recursive-build/recursive-frame:
# mkdir fresh-scratch
# target/debug/examples/queued_close_old fresh-scratch visible MODE protect
```

That local old binary is not committed; its SHA-256 is retained in
`correction-environment.log`. Current build artifacts now contain the correction;
repeating the old comparison requires independently rebuilding the exact parent,
not relabeling current artifacts as old.

On the correction, the new driver passes **12 scenarios**: visible/hidden ×
leaf/recursive-build/recursive-frame × protect/discard. Every protected participant
holds exact UTF-8 bytes through an actual ENOTDIR failure for at least 600 ms.
Only a successful explicit retry (write, sync_all, exact-byte readback), or an
explicit synthetic discard of its Failed token, authorizes destruction. Drop
instrumentation and final built/authorized/dropped counts reject silent loss.
Recursive-frame scenarios protect two drafts; other scenarios protect one.
No recovery files exist on discard; successful files are byte-compared by the
driver too. Final per-scenario logs are in `queued-native-final/` with aggregate
`queued-native-final-results.log`. Scratch content is local/ignored, not live data.

## Final commands and observed results

Disk checked **first**, between builds, and after tests: about **12 GiB free**;
isolated target ended at **2.1 GiB**. No cleanup or shared-target mutation.
All Cargo builds/tests/lints were locked/offline with two jobs, incremental
compilation disabled, and both dev/test debug info disabled.

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
cargo fmt --all -- --check
cargo build --locked --offline -p rndr_render --example safe_close --example queued_close
cargo test --locked --offline -p rndr_core -p rndr_render -p rndr
cargo test --locked --offline -p rndr_core -p rndr_render --no-default-features
cargo clippy --locked --offline -p rndr_core -p rndr_render -p rndr --all-targets -- -D warnings
python3 scripts/test-queued-close-native.py evidence/queued-native-final
python3 scripts/test-safe-close-native.py evidence/native-correction-final
git diff --check
```

Native output paths must be fresh on independent reruns; do not reuse the paths
above. Original reproduction rustc checks used `-C debuginfo=0` and existing local
Cargo artifacts, without rebuilding/downloading dependencies.

| Final check | Result | Durable receipt |
|---|---|---|
| Default affected crates, integration tests and doctests | **413 passed**, 0 failed/ignored; 44 suite summaries | `correction-default.log` |
| Minimal core/render plus doctests | **389 passed**, 0 failed/ignored; 42 suite summaries | `correction-minimal.log` |
| Strict all-targets Clippy | Exit 0, no warnings | `correction-clippy.log` |
| Both native example builds | Exit 0 | `correction-build.log` |
| Formatting | Exit 0 | `correction-fmt.log` (empty successful output) |
| Six original synthetic native scenarios | **6 passed**, exit 0 | `native-correction-final-results.log`, `native-correction-final/*.log` |
| Queued-open native race scenarios | **12 passed**, exit 0 | `queued-native-final-results.log`, `queued-native-final/*.log` |
| Unchanged independent repro on final code | Pending/alive, no drop/return at 3s; synthetic harness termination afterward | `queued-review/original-corrected-final.log` |

The default/minimal counts overlap. The two new core tests cover recursive queue
commitment and 100 concurrent open/commit races. Earlier native runs before the
final post-commit callback suppression also passed; their separately named logs
are retained, but the table refers to the final-code reruns. Initial development
of the new probe caught unsupported configuration/capturing-system syntax;
that test driver was corrected before the old/new native comparison and final
build/lint/test passes. Expected old negative-control assertions are failures of
the old implementation, not failed correction checks.

## Acceptance limits

Screen Recording remains denied: **no visual acceptance claim**, no permission
change, and no native screenshot signoff. AX close used the actual native close
button; Quit used AppKit termination through the existing guarded hook. Synthetic
native behavior is not Blooey integration or rich Live/Write acceptance.

Apps still must install guards before frames, freeze/invalidate mutations through
Ready, validate all draft epochs, serialize pending writes, and confirm discard.
An endless open stream can keep exit pending safely. Force Quit, SIGKILL, crash,
power failure and external `process::exit` remain outside the close-veto contract.
Blooey adoption, usable recovery retention, and integrated product acceptance are
still separate remaining work. This local correction is ready for the sender's
fresh independent review; it is not approved or integrated.
