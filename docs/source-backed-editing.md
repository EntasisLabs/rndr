# Source-backed editing: bounded gap audit and integration contract

Baseline: owner-integrated `ef621ddc34edf4f9ecafb1630066bbf2a5e33cd8`.
This pass is independent of the unaccepted desktop-app foundation. No Liquid
parser, application persistence, or close policy belongs to this change.

## Grounding and API inventory

Read-only app inspection: Blooey `PRODUCT.md`, `native/src/main.rs`, and
`native/src/preview.rs`. The inspected native code keeps one source `Buffer`
per note, suspends input rather than destroying editor/history state, and uses
source ranges to partition display-only preview parts. Live is explicitly
unavailable in that inspected baseline. The product requires exact source bytes,
precise task edits with undo, and eventually source/rendered selection continuity.
The historical RNDR-GAPS failures are **not** evidence of current failures.

Current framework contracts:

| Surface | Existing reusable behavior | Boundary |
| --- | --- | --- |
| `Buffer` + `Editor` | Source-byte caret/anchor, visible window publication, soft-wrap visual navigation/reveal/drag | Editable source, not a rich projection; the published `Label`/`Caret` are window-relative |
| `Spans` + `TextStyle` | Style source-byte ranges; editor measurement, navigation, hit testing and paint use `TextEngine` | Styling does not remove source bytes; one entity-level base style, not a heterogeneous document block model |
| `TextEngine::layout` / `TextLayout` | Shaped glyphs, visual rows, byte-index caret edges, hit testing, selection geometry | Indices refer to the exact shaped string, **not another source string with hidden markup** |
| `MarkdownView` / `MarkdownBlock` | Display blocks, styled inline text, read selection/copy and links | Read-oriented. `MarkdownBlock::text` excludes markers; copy can reconstruct Markdown, not guarantee exact source byte identity |
| `EditHistory` | Per-entity bounded keyboard undo/redo, typing coalescence, selection snapshots | Snapshot internals intentionally private; low-level `Buffer` mutations do not implicitly join history |
| `Input` / IME | Shared composition filtering, focused editor routing, preedit display | Headless input injection is not native platform/IME acceptance |

## Demonstrated blockers and repair

### 1. External source operation bypasses keyboard history

Red reproduction is local commit `778f8a5` (test-only, directly on baseline).
Run `cargo test --offline --locked -p rndr_core --test source_edit` there:

1. A real `App` with `Chrome`, focused `Buffer`/`Editor`, and queued key input
   types `!` into `- [ ] task`.
2. The existing public `delete_range(3, 4)` / `insert_str(3, "x")` path toggles
   the source to `- [x] task!`.
3. Standard queued Ctrl-Z should undo only the source operation first.
4. Actual: `- [ ] task`; expected: `- [ ] task!`.

This is a missing reusable transaction API, not a claim that the documented
history-free low-level buffer functions violate their own contract.

The repair is additive:

```rust
use rndr_core::{edit_buffer, BufferEditError};
// `note` is the existing note Buffer entity. The app derives a fresh range.
// edit_buffer(world, note, 3..4, "x") -> Result<bool, BufferEditError>
```

`edit_buffer(&mut World, Entity, Range<usize>, &str)`:

- validates ordered, in-bounds UTF-8 scalar byte boundaries; no lossy clamping;
- returns `Ok(true)` for a change, `Ok(false)` for identical bytes;
- refuses missing buffers, invalid ranges, or target IME preedit without changing
  text, selection, history, clipboard, focus or pending input;
- records one step in the **same entity's** standard `EditHistory`, clears redo,
  and breaks typing coalescence before/after the source operation;
- preserves endpoints before the range, shifts endpoints at/after its end, and
  sends endpoints inside replacement to its end; insertion is right-affine;
- snaps resulting selection endpoints backward to grapheme boundaries; Undo and
  Redo restore their respective text/selection snapshots;
- invalidates transient visual navigation/drag state, without stealing focus or
  forcing a different scroll policy. Chrome republishes on its next editor pass.

The original keyboard-history regression now passes: first Undo restores the
unchecked marker **and retains `!`**, second Undo removes `!`, and two Redos
restore both operations. The raw `Buffer` APIs remain unchanged/history-free.

### 2. Standalone IME commit bypasses keyboard history

An expanded real-input regression first failed after the range API repair:
preedit `に`, commit `日本` with **no queued key**, source toggle, two Ctrl-Zs.
After undoing the toggle, the next Undo left `日本` in the document instead of
removing the commit. This exposed the no-key fallback to history-free legacy
editing, not a font/shaping failure.

Standalone native-style commits now use the existing queued editing function,
including selection replacement and history. Keyed commits also break typing
coalescence around the commit. A regression compares keyed and standalone
commits over a reversed selection, checks exact text/selection Undo/Redo, and
ensures the preceding typed run remains its own step. Explicitly mixed legacy
`Input.key`/`Input.chars` plus commit frames retain their legacy path; this pass
does not redesign legacy input semantics or claim parity for every synthetic
mixture of legacy and native input.

## App recipe and ownership

See the compiled `edit_buffer` rustdoc example. The app interprets a task/card/
other control, validates its parser revision, resolves the **current source**
range, then calls the transaction on the note's existing Buffer entity. Do not
mutate a detached copy and replace the entity: that would discard per-note undo.

Observe the authoritative Buffer after Chrome's PostUpdate editor pass (including
keyboard Undo/Redo/IME). Rebuild app-derived parser data, styled spans, decorations,
and preview from that source. `edit_buffer` does not rebase these semantic ranges,
emit a new app save event, serialize an AST, or acknowledge durability. Existing
app dirty/recovery/save logic must observe the Buffer exactly as it does typing.
An app Update handler runs before Chrome's editor pass, so it must not interpret
queued keyboard edits as already applied. A deferred parser result must be
revision-checked by the app before applying any range operation.

Liquid syntax, canonical grammar, malformed/unknown fallback, component actions,
and source preservation remain Blooey's responsibility. Nothing is executed or
fetched by this API.

## Deferred projection contract — recommendation, not a new implemented editor

No generic lossless hidden-markup projection API was demonstrated by this pass.
Do **not** treat the displayed index in `MarkdownBlock::text` as a Buffer index,
or round-trip `MarkdownView::selected_text` into source for exact-byte editing.
Keep Source editing working while this contract is designed/tested separately:

1. App produces an immutable, revision-stamped source/display map with explicit
   copy, hidden, replacement, and atomic-component segments. rndr need not know
   what Liquid or Markdown means.
2. Mapping defines both affinities at hidden delimiters and soft-wrap/bidi
   boundaries, plus source selection behavior across non-text blocks. A single
   offset delta cannot encode all these cases.
3. Shape the **display** string once per style/width revision. Map the shaped
   byte/grapheme hit back through the same projection snapshot; never reuse a
   source-string layout for a different display string.
4. Translate an allowed operation to validated source edits on the same Buffer
   and history. Multi-range atomic transactions, stale-revision refusal, and
   app-defined reveal/fallback around unsupported blocks need separate contracts.
5. Reproject after source changes and undo; define native IME preedit placement,
   composition cancellation/commit, clipboard source-vs-display policy, keyboard
   block traversal, bidi caret affinity and accessibility before claiming Live.

No hidden-markup mapping bug, RTL native-input bug, resize failure, or emoji
failure is asserted merely because these are investigation candidates. The
included production-Parley test passes for **styled source** with Arabic, emoji,
soft wraps, resizing, range edits, Undo/Redo and pointer-to-source hit testing.
It does not establish editable hidden-markup blocks or full bidi caret affinity.

## Verification boundary

`crates/rndr_core/tests/source_edit.rs` drives real Chrome/Input systems, not a
replacement editor. It covers the repaired history behavior, UTF-8 refusal,
no-op/redo, source byte fidelity (BOM/CRLF/opaque fences/Arabic/emoji), selection
mapping and grapheme joins, typing boundaries, IME, clipboard non-interference,
and per-note focus/input/history isolation.

`crates/rndr_render/tests/source_edit.rs` drives `rndr_paint::frame` with bundled
**production ParleyText**, styled source ranges, real color emoji shaping,
soft-wrap resize, source transactions, standard Undo/Redo, and pointer hit testing.
It is a headless shaping/paint-input pipeline test, **not native window input or
visual signoff**. Native platform clipboard, OS IME, VoiceOver, captured drag
outside the window, and visual acceptance remain owner/app acceptance work.

Existing editor navigation/caret/drag, clipboard, IME, Markdown, text engine,
paint, close-state and renderer tests must remain green. This pass does not
rerun or claim the prior 18 native close scenarios; close code is unchanged.
See `evidence/source-backed-editing.md` for exact commands/results and resources.
