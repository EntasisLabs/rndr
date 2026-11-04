# E1: experimental native source session

2026-11-04 implementation continuation of the [editing epic](native-document-editing-epic.md). The user authorized the **first source slice only**. This record does not approve later milestones or declare native/performance gates complete.

## Delivered boundary

- `rndr_core::editing`: process-unique `DocumentId`, monotonic `Revision`, exact UTF-8 source, atomic base-coordinate `SourceBatch`, forward/inverse changes/maps, provenance and ordered pull-based `ChangeEvent`s.
- One document-owned history path for native typing, paste, IME commit, host splices and undo/redo. Reuses the existing keyboard/history implementation, not another editing engine.
- `SourceDocuments` resource outlives retained entities. E1 permits **one attached view per document**; a second view refuses explicitly. Selection survives detach/reattach; this is not yet a multi-pane selection policy.
- `rndr_ui::editing::EditorSurface` follows retained `UiId` mount/update/stale-handle/destruction conventions, binds a document, supplies an accessible name, theme/code typography, optional existing line-number gutter and existing editor scroll/layout/input mechanics.
- `examples/source-session`: a runnable, in-memory fake host. Revisioned host splice, capture-save, delayed acknowledgement, explicit reload and readonly toggle. No filesystem persistence, Medousa, LSP, structured tree, projection or Markdown serialization dependency.
- Legacy unmanaged `Buffer`/`Editor` behavior remains. `edit_buffer` deliberately returns `ManagedDocument` on a managed view: callers must use a revisioned session batch rather than bypass document history/events.

This is a concrete experimental API, not an implementation of every illustrative type in the architecture sketch. It keeps String source and existing Buffer projection/layout; it does **not** introduce a rope, incremental layout, providers, find UI, rich clipboard, interactive scrollbar, accessibility range operations or universal document positions.

## Decisions exercised by this slice

1. **Batch coordinates:** ordered, nonoverlapping scalar-boundary UTF-8 byte ranges, all in the same immutable base revision. Adjacent ranges are allowed. Duplicate starts are rejected rather than inventing insertion ordering. Wrong identity, stale base, invalid/overlapping ranges and readonly admission refuse before changes/history/events. Net no-ops preserve revision, selection, history and redo.
2. **Position maps:** explicit `Before`/`After` affinity; deleted interiors collapse to the selected side. Source splice validity is scalar-based; interactive selections snap to grapheme stops. Inverses are after-coordinate batches, coalesced when adjacent deletions would otherwise create duplicate insertion positions. Maps require valid base offsets; they are not a cross-model position type.
3. **History:** document owns snapshot history; a mounted entity never has a competing history owner. Existing typing grouping is retained; host edits and each IME commit are barriers. Undo/redo produce new revisions. Oldest snapshots evict above a **16 MiB snapshot budget** (plus small container overhead); an edit whose prior snapshot exceeds the budget may have no retained undo. A focus change is not a content event.
4. **Notifications:** no synchronous subscriber callback inside commit. Hosts drain ordered events and enqueue their next transactions normally. At **256 undrained events**, mutations refuse with `EventBackpressure`; native refusals are observable in `SourceInputError` on the view. Copy/selection/navigation continue. This is a count bound, not a total event-byte budget; large replacements carry proportionally large inverse payloads. Hosts must drain each frame and report refusals, not ignore them.
5. **Composition:** transient preedit captures the focused view's selection and never enters source/history. Host mutations, readonly transitions and explicit detach refuse while it is active. Native commit replaces the captured range once, as one history step; cancellation produces none. Pointer selection does not relocate an active composition. On a focus move, the old owner remains blocked until native commit/cancel; an orphan commit is discarded rather than inserted in the new document. OS cancellation is not claimed to be synchronously requested/acknowledged by this API. Abrupt raw entity destruction is outside retained teardown; reattachment cancels the old transient session, never resurrecting it as source.
6. **Readonly:** session policy admits every managed content origin, including host, undo/redo and reload. It is projected into local semantic readonly state. Inherited UI readonly additionally filters native input. A host must set session policy for document-level readonly; a visual ancestor is not the host authorization model. Current native accessibility `SetValue` remains unsupported; this slice does not add a raw-write bypass.
7. **External state:** `reload(current_revision, bytes)` is an explicit host-authorized clean replacement, not an acknowledgement. Changed reload clears history; identical bytes are a no-op. Dirty/conflict policy remains host-owned. `snapshot()` captures bytes and revision; acknowledging it must never reload those bytes over newer local edits. The fake host tests this delayed-save case. Concurrent saves, durable persistence and application-specific conflict resolution remain adapter work.

## Run and verify

```sh
cargo run -p source-session
cargo test -p rndr_core -p rndr_ui -p rndr_render -p source-session
cargo check --workspace --all-targets
# Explicitly public built-in fake fixture only; no native screenshot/window:
cargo run -p source-session -- --capture target/source-session.png
# Model-only, opt-in measurements; no timing assertion or native frame claim:
cargo test -p rndr_core --release --test source_session_cost -- --ignored --nocapture
```

The capture command refuses to overwrite an existing output path. It exercises the actual retained demo through production Parley shaping and offscreen GPU paint. It is **not** OS/native-input evidence.

### Final automated receipts

On this isolated worktree, `cargo test -p rndr_core -p rndr_ui -p rndr_render -p source-session` completed with **771 passed, 0 failed, 1 intentionally ignored** (the opt-in cost characterization, run separately and passed). `cargo check --workspace --all-targets`, `cargo fmt --all -- --check` and `git diff --check` passed. Workspace check reports one unused-variable warning in the unchanged `rndr` `ui_polish` example; no unrelated cleanup was made.

The corrected fake-host offscreen capture was generated and visually inspected at 1000×720: command row, readable host status, source/gutter and Unicode/emoji text are visible. The first capture exposed a missing canvas/status contrast style; the demo theme recipes were corrected and a second capture inspected. This is production-renderer visual evidence, not a native window or platform-input test. Reproducible logs/images remain under local `target/e1-*`; they are not committed build artifacts.

### Verification coverage

- Source-session tests: atomic refusals preserve redo/selection; wrong identities; net no-ops; monotonic events; affinity/maps; replayable adjacent inverses; 800 deterministic randomized Unicode/BOM/CRLF splice/inverse/undo/redo cases; typing + host + paste share one history; legacy input; readonly all origins; IME with/without queued keys; captured range restoration; orphan focus commit does not transfer; explicit host selection survives projection; detach/remount; raw Buffer mutation cannot become source; backpressure preserves copy/navigation.
- Retained UI tests: stable identity/caret/scroll/history across props updates, document mismatch, duplicate attachment, reciprocal sibling-key conflicts, malformed/stale/core-detached handles, readonly and accessibility metadata, borrowed-child teardown and remount history.
- Real Parley test: both managed and legacy paths through styled Arabic/emoji/Unicode source splices, undo/redo, narrow-width reflow and pointer hit testing.
- Fake host test: delayed acknowledgement updates only the captured saved baseline, leaves newer source/revision intact and dirty, then explicit reload restores acknowledged bytes.
- Independent read-only review found a backpressure navigation/copy refusal; fixed with a dedicated regression. Integration review also fixed reciprocal UI identity conflicts, stale readonly semantic state and detached-binding validation.

### Model cost characterization (not a performance pass)

Measured on Apple M4 arm64, macOS 27.0.1, Rust 1.98.0, release profile. Five samples per fixture/position; report medians and maxima, not statistically meaningful p95. Fixtures: ASCII lines, Unicode/CRLF lines and one long line; insertion at beginning/middle/end. Sizes are approximately 10 KiB / 1 MiB / 10 MiB (rounded to fixture units).

| Size | Session commit + undo, median range across fixtures/positions | Raw Buffer insertion, median range |
|---|---:|---:|
| 10 KiB | 0.360–0.612 ms | 0.005–0.016 ms |
| 1 MiB | 37.246–63.754 ms | 0.533–1.507 ms |
| 10 MiB | 381.086–651.808 ms | 5.508–15.582 ms |

These operations are deliberately **not equivalent**: managed timings include admission, mapping, snapshots, inverse/events and undo, whereas raw insertion is the pre-existing low-level baseline. They exclude session/view construction, layout, shaping, paint, native event latency and RSS. Existing whole-string copies/scans and grapheme traversal remain significant. No large-file responsiveness or E0/E2 performance threshold is claimed. Use this only as a starting measurement; profile before choosing storage/cache work.

## Gate status / remaining work

The source-session integration and headless proof are ready for review, **not a production/native sign-off**. E0 defaults above are bounded experimental choices, not universal API ratification. E0's full frame/layout/RSS/performance envelope and later epic tasks remain open.

Run the demo on the first intended native desktop before closing its platform gate: typing/repeat/release, selection/word/row movement, external clipboard, native IME preedit/commit/cancel/candidate placement, focus/Tab/toolbar composition transitions, resize/scroll, readonly copy and screen-reader announcements. Headless tests and offscreen captures cannot certify these. Windows/Linux, native accessibility text ranges and large-document frame budgets remain explicitly unverified. Nothing here authorizes an app migration, release, merge or later epic milestone.
