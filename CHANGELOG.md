# Changelog

## Unreleased

### 0.3.0

- Several windows on one event loop. `Session` holds one app per window, each with its own world and surface. `Desktop` sends and broadcasts `WindowMsg`, and can open another window after the loop has started. Closing the first window exits the process. Closing a later window removes only that window. Panel mode is unchanged. `examples/windows` is two windows that tell each other about a save.
- `Pointers` stores every contact for the frame: kind (mouse, touch, pen, eraser), position, optional pressure, tilt, and twist, buttons, a timestamp, and the coalesced samples since the last frame. `Input.pointer` stays the primary contact, so existing widgets do not change. The mouse stays primary while that contact exists. An app calls `Pointers::set_primary` when a pen should drive those widgets. Palm rejection stays in the app.
- winit touch events write a touch contact. `Touch.force`, where the platform provides it, is pressure. Two touch contacts that are down synthesize pinch, pan, and rotation `Gesture` events. On macOS, `PinchGesture` and `RotationGesture` are the same event. A platform magnification of `0` is stored as scale `1`.
- IME preedit and commit are resolved once per frame in the field, the prompt, and the editor, so a commit is inserted once. A focused text widget allows the platform input method.
- Accessibility tree with a role and a name for the existing widgets. The `accesskit` feature, on by default, publishes it on macOS, Windows, and Linux. A `ListKeys` list is focusable and can be walked with the keyboard when no keymap context already handled those keys.
- File hover, drop, and hover-cancel events. winit 0.30 supplies them on macOS, Windows, and X11, with a path and no position. Wayland does not.
- Clipboard images: straight RGBA8, written when `image_dirty` is set and read when `want_image` is set. `None` does not clear the operating-system image.
- Native open and save dialogs behind the `dialog` feature, which is off by default. The result is `FileChosen`. Cancel leaves the path empty. Linux uses the file-chooser portal.
- Custom cursor images. `CursorStyle::Image` uses the `CursorImage` resource. `rotate_cursor` rotates pixels and the hotspot clockwise in screen space.
- `examples/pen-probe` logs pen and touch pressure. On macOS a local `NSEvent` monitor copies tablet pressure, tilt, and twist onto `PEN_ID`. On Windows a pen arrives as touch and `Touch.force` is the pressure; the kind stays touch. Neither path is verified without a tablet or a digitizer. Trackpad pressure is not a pen.

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
