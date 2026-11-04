# Experimental source session (E1)

Run `cargo run -p source-session` on a native desktop. This is a fake host:
**nothing is written to disk**. It shows exact-source editing, revision/change
counts and host-owned dirty/save state, not structured Markdown editing.

- Type, select, undo/redo, and try native IME in the editor.
- Tab to a command and press Enter/Space (or click).
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
