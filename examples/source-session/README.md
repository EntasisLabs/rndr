# Experimental source session (E1 + E2 + E3 candidate)

Run `cargo run -p source-session` on a native desktop. This is a fake host:
**nothing is written to disk**. It shows exact-source editing, revision/change
counts and host-owned dirty/save state, not structured Markdown editing.

- Type, select, undo/redo, and try native IME in the editor.
- Run `cargo run -p source-session -- --split` for two views over the same
  document. Selections and scroll remain independent; edits/history are shared.
  Each pane has a distinct accessible name. Readonly is document-wide.
  Try a selection in each pane, type into one, then undo from the other.
  `--split --capture target/source-session-split.png` captures the public fake fixture.
- The [E3 implementation/evidence record](../../docs/design/native-document-editing-e3.md)
  separates layout/model measurements from native event-to-present certification.
- Page Up/Down moves by a shaped viewport; hold Shift to extend selection.
- Drag the vertical scrollbar or click its track to page. When focused, arrows,
  Page Up/Down and Home/End adjust the same document viewport; readonly permits
  scrolling. Tab/Shift-Tab traverses the editor, scrollbar and host commands.
  Disable retained chrome with `EditorSurfaceProps::scrollbar = false` when
  composing a host-supplied control.
- Hold selection drag above/below the editor for scheduled timed autoscroll.
  Release or blur cancels it; outside-window delivery depends on the desktop
  backend, not a global pointer-capture promise.
- Tab does
  not insert indentation. During active managed IME composition candidate keys
  remain owned by the IME rather than menus or focus traversal.
- The [E2.1 input candidate](../../docs/design/native-document-editing-e2-input.md)
  records the exact keyboard/composition contract and remaining native gates.
- The [E2.2 semantic ranges](../../docs/design/native-document-editing-e2-semantics.md)
  adds AccessKit editor text ranges and directed selection for focused editable
  and readonly source. Headless bridge tests are not VoiceOver/NVDA certification.
- The [E2 viewport/closeout ledger](../../docs/design/native-document-editing-e2-viewport.md)
  records scrollbar, drag and shaped Unicode tests and the still-open native
  certification cells. E2 implementation is not a blanket platform pass.
- **Host splice** submits an explicit revisioned append, without serializing Markdown.
- **Capture save**, edit more, then **Ack save**: only the captured snapshot is
  acknowledged; newer local edits must remain dirty and intact.
- **Reload saved** explicitly replaces the current session from acknowledged
  bytes using the current base revision. Core rejects stale reload revisions;
  this button intentionally requests reload at the current revision.
- **Toggle readonly** changes session permissions; copy, selection and navigation remain.

## Presentation and visual checks

The document is the primary surface; the fake host controls live underneath it.
The header reports snapshot equality, edit permissions, revision and any captured
save awaiting acknowledgement. Snapshot equality does **not** mean a disk save.
Source uses 15-point shaped monospace text (Menlo on macOS; the engine's
monospace fallback elsewhere), with Inter UI chrome and aligned line numbers.
The initial source fixture, including BOM, CRLF and opaque blocks, is unchanged.

Native and offscreen modes share `demo_app()` and `ParleyText::new()`; do not
install the legacy `TextFace` bitmap fallback in this demo. The earlier native
bitmap override masked the normal renderer engine while capture used Parley,
so the previous offscreen check was not representative of native typography.
System font differences can still change pixels across machines.

To capture only the built-in public fake fixture, without opening a window:

```sh
cargo run -p source-session -- --capture target/source-session-preview.png
```

The output path must not already exist. Capture uses the native default logical
size (800×600). Tests cover layout bounds at 640×540 (the native window minimum),
800×600 and 1000×720, shared shaped-engine configuration, readonly status, and
delayed-save acknowledgement preserving subsequent edits. These are layout and
model checks, not native input certification.

Headless lifecycle tests do not certify visual layout, native IME candidate
placement, clipboard integration, screen-reader announcements or native focus.
Those require running this example on each supported desktop platform.

## E7: supported source-preserving staged editor

```sh
CARGO_INCREMENTAL=0 cargo run -p source-session --bin projection
# Public fixture capture; output must not already exist:
CARGO_INCREMENTAL=0 cargo run -p source-session --bin projection -- --capture target/e7-preview.png
```

This is the supported staged projection composition, **not** a general inline
Markdown editor. Reusable `Projection::capture` / `ProjectionTarget` in the core
library own the checked edit target; the example has no private target API. The
source pane owns the sole document/history. The rendered pane hides one-pair
emphasis delimiters, displays atomic task markers and gives unsupported blocks
an explicit source-reveal fallback. Nothing is serialized or auto-fetched.

- Apply the seeded `quiet` → `calm` replacement, then **Undo source**: exact
  delimiter/BOM/CRLF/raw-block bytes must return.
- Select literal text in Rendered, **Capture target**, type/compose in the native
  replacement field, then **Apply replacement**. Apply is one source transaction.
- **Toggle task** splices only the status byte. **Reveal source** selects the raw
  region without mutation. Native Copy uses the selected pane's text policy.
- Capture, edit Source, then Apply the old target: stale refusal, no guessed rebase.
- **Read preview** switches source readonly policy. IME commands refuse during
  preedit; finish or cancel native composition before applying/switching.

The supported grammar is deliberately narrow; structural/cross-segment edits
reveal source. Policy, copy/affinity/IME limits and the block-custody comparison:
[`docs/design/native-document-editing-e7.md`](../../docs/design/native-document-editing-e7.md).
The original desktop experiment was accepted on 2026-11-05. Native OS
IME/clipboard/AT certification and a full inline projected widget remain separate.
See the executable module recipe and `projection_contract.rs` for promotion gates.
