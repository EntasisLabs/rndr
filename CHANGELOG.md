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
- Display list beside the quad list: paths, images, glyphs, transforms, and layers. Quads stay the fast path, and a frame with no commands presents the same way it did before.
- Vector paths. Fills use non-zero and even-odd rules. Strokes take a width, caps, joins, a dash array, and a hairline that stays one device pixel wide. Ellipse, polygon, line, and rounded-rect helpers build the path. Tessellation runs on the CPU and is cached by geometry and zoom bucket.
- Edges of paths and layers are 4× multisampled on the offscreen target, then resolved before a blur, an opacity composite, or the quad pass. UI quads keep their signed-distance edges. A device that cannot resolve 4× draws the same mesh at one sample.
- 2D affine transforms per command, plus a root camera. Device pixels are world units times the camera scale times the viewport scale.
- PNG, JPEG, and WebP decode off the UI thread. A GPU texture cache evicts least-recently-used images to a byte budget, builds mipmaps, tiles past the device texture limit, and crops with a UV rect and rounded corners. List rows can show an image with `ImageView`.
- Group opacity, rounded-rect and path clips, and a separable Gaussian drop shadow. The shadow is two filter passes, so another effect is another pass.
- A canvas painter draws inside a `SurfaceSlot`, under the widgets in that slot.
- Offscreen export renders a world rectangle at a chosen scale to RGBA and PNG. A side longer than 8192 pixels, or an image over 32 megapixels, is rejected.
- Text exposes letter spacing, paragraph spacing, and alignment. Glyphs follow rotation and zoom. Runs at or above 64 device pixels are filled from face outlines; smaller runs stay in the atlas.
- Stroke arrowheads. [`Marker::Chevron`](crates/rndr_paint/src/path.rs) and [`Marker::Triangle`](crates/rndr_paint/src/path.rs) are drawn with the stroke but stay solid unless `dash_markers` is set, so a dash on the shaft does not break the head. `examples/canvas-scene` uses a chevron on the dashed arrow.
- [`ZoomCamera`](crates/rndr_paint/src/zoom.rs) stores wheel zoom as an integer step around the cursor that started the gesture. Ten notches in and ten notches out restore the pan and the scale exactly. A press, move, and release that arrive before the next frame pan by the full distance.
- `examples/create-view` loads a saved Create scene (schema `medousa-create` version 3) and draws shapes, lines, ink, text, images, groups, and frames. The scene types live in the example. Published crates do not depend on Medousa. `--export out.png` renders offscreen. Fixture scenes under `examples/create-view/fixtures` cover every node kind.
- `TextStyle::line_height` is an optional multiple of the font size. `None` keeps the face's own line height. Paragraph spacing is still added after a newline.
- A face with no bold or italic master is synthesized: the outline is emboldened, or skewed when the requested style is italic and the selected face is upright.
- Rotated glyph runs are filled from face outlines. When a face has no outlines, those sprites are sampled linearly. Axis-aligned runs below 64 device pixels stay on the nearest sampler.
- `ZoomCamera::drag_from` anchors a pan at the press position, so a move that arrives with the press is kept.
- Canvas underlines use the font's underline thickness, at least one device pixel, snapped to the pixel grid.
- An ink ribbon closes with the same rounded cap at both ends.
- A chevron marker is one stroke with a miter join at the tip, so the two arms meet.

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
