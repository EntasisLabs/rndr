# Changelog

## 0.2.0

- Embedded terminal in `rndr_term`. A PTY (ConPTY on Windows) and a VT state machine copy cells into a `MonoGrid`, and unchanged rows are not shaped again. The `rndr` feature `term` re-exports it and is off by default, so a desktop build does not link a PTY.
- Background feeds deliver values from a thread or future onto the UI thread. A burst wakes the window once, a fixed capacity applies backpressure, and an older epoch is dropped so a slow search cannot overwrite a newer query.
- Multi-line prompt. Plain Enter submits, Shift+Enter inserts a newline, and the editor reuses the single-line caret, undo, clipboard, and IME.
- Streaming markdown view. Appended tokens reuse the stable prefix of the document, and the scrollport stays pinned to the bottom until the reader scrolls away. The optional `highlight` feature colors fenced code by token class.
- Doc comments on every public item.

## 0.1.0

First published release.

- Entity world: hierarchy, components, events, tasks, and timers
- Layout in logical points, including rows, columns, clip, scroll, splits, tabs, and overlays
- Text input: caret, fields, wrapping, a gap buffer, decorations, and input-method preedit
- Paint step that walks the world and returns colored quads
- Desktop runner that opens a winit window and presents those quads with wgpu
- Text shaping and rasterization, with Inter, Noto Sans Arabic, and Noto Color Emoji bundled
- Global hotkeys on macOS, Windows, X11, and the Wayland global shortcuts portal
- Tray icon and menu on macOS, Windows, and Linux (StatusNotifierItem)
- Window commands: show, hide, focus, hide on blur, frameless panels, and always on top
- Filesystem backends for the real disk and for an in-memory tree
