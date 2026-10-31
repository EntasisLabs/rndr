# Bounded source-backed editing pass — verification

Baseline: `ef621ddc34edf4f9ecafb1630066bbf2a5e33cd8`.
Test-only red reproduction: `778f8a5` (direct child of baseline).
The final fix is the commit containing this report; no owner integration,
review approval, sealing, push, publication, or deployment was performed.

## Before / after

Real Chrome/App/Input reproduction, not a substitute editor:

- Type `!` with a queued `KeyEvent` into `- [ ] task`.
- Low-level `Buffer` range toggle produces `- [x] task!`.
- Ctrl-Z at red commit: actual `- [ ] task`, expected `- [ ] task!`.
- After additive `edit_buffer` API: first Undo retains `!` and restores unchecked
  marker; second Undo removes `!`; two Redos restore both steps.

The expanded IME regression then failed after the initial source transaction fix:
standalone commit `日本`, range toggle, two Undo operations left `日本` instead
of removing it. Standalone commits now go through the existing queued editor
path; keyed/standalone IME commits have explicit undo boundaries. Selection
replacement, previous typed-run separation and exact Undo/Redo are regressed.

## Final checks

All commands ran in the isolated Forge worktree. Common build environment:

```sh
export CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="$PWD/target"
```

| Command | Final result |
| --- | --- |
| `cargo fmt --all --check` | passed |
| `cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_render` | **483 passed**, 54 suite summaries, zero failed/ignored |
| same test command with `--no-default-features` | **460 passed**, 54 suite summaries, zero failed/ignored |
| `cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_render -p rndr --all-targets -- -D warnings` | passed |

Counts include doctests and the matrix overlaps; they are **not** 943 distinct
tests. New coverage is eight core tests, one production-Parley integration test,
and one compiled public API example. The no-default renderer test is correctly
feature-gated (zero tests there); the default matrix exercises it with Parley.
Existing navigation/caret/drag, keyboard, clipboard, IME, Markdown, paint, close,
renderer and offscreen export checks remain green.

The first expanded test build required correcting an invalid empty entity
fixture. Clippy initially flagged the intentionally reversed `4..3` literal;
the test now constructs an explicit malformed `Range`. Those harness/lint issues
were repaired and the final matrices rerun. No failed final test is omitted.

Local diagnostic logs (ignored build output, not required source artifacts):

- `target/source-edit-default-tests.log`: SHA-256
  `3aa2b69a95c190135b73cb1520b3e8728224ec817a1d5236e629fa9793f83e3f`
- `target/source-edit-minimal-tests.log`: SHA-256
  `074e97f00be5ca2057f966678aebd2462bb4aebe7a63abe038766e27e75c83d9`
- `target/source-edit-clippy.log`: SHA-256
  `e271478f64f1fb4e3332d6f65353410b1b34b907df5cab17ebf6a1d14a6acd8e`

## Resources and acceptance limits

Disk began at about 12 GiB available. Last measured about 9.2 GiB free, isolated
`target` about 2.1 GiB. Two build jobs, incremental disabled; debug symbols off
for the broad matrix to bound disk. No broad cleanup or shared target mutations.

Production `ParleyText::bundled` was driven headlessly through `rndr_paint::frame`
with styled source, Arabic, real color emoji, wrapping, resizing, pointer hit
testing and standard input Undo/Redo. This **does not** establish native OS IME,
window input, clipboard integration, accessibility or visual acceptance. The
prior 18 native close scenarios were not rerun in this pass; no close code was
changed. Owner retains independent review and native/app integration.

No Blooey, Medousa, owner-main or unrelated worktree was modified. Blooey product
and native source were inspected read-only. No desktop foundation was adopted.

See `docs/source-backed-editing.md` for API semantics, the app recipe, and deferred
hidden-markup projection / cross-block / multi-range / native acceptance gaps.
