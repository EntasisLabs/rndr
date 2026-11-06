# Changelog

## 0.4.0

Workspace version 0.4.0, since the published 0.3.1 crates. `scripts/publish-crates.sh` uploads `rndr_core`, `rndr_paint`, `rndr_std`, `rndr_term`, and `rndr_render`. `rndr_ui`, `rndr_app`, and every example, including Desktop Workshop, stay `publish = false`. Their package versions move with the workspace so path requirements match; that does not put them on crates.io. The `rndr` facade is 0.4.0 in this checkout and is not uploaded. Its optional `ui` feature depends on `rndr_ui`, and Cargo requires that dependency to resolve from the registry before it will accept the facade, whether or not the feature is enabled. Publishing `rndr_ui`, or removing `ui` from the facade, is left as an open public-surface decision. See [package policy](docs/ui-status.md#package-policy).

`rndr_std` and `rndr_term` have no behavior change in this release. They move to 0.4.0 so a single set of path versions resolves.

### Core, paint, and the desktop runner

- A `Scrollbar` drives one `Scroll` viewport, or the vertical range of a soft-wrapped editor. `apply_scrollbars` handles the pointer and keys. The thumb is at least 24 points long when the track allows it. Paint draws that thumb, and a scroll indicator stays on the visible clipped edge when an outer clip cuts the viewport. A pointer highlight on the thumb clears on release and does not take focus. `list_window` is the shared uniform-height visible range for the core list and the semantic virtual list.
- An entity with `ResolvedColor` keeps its authored color when focused. A legacy button with no `ResolvedColor` still brightens on focus.
- Experimental source sessions (`editing`) and a separate experimental structured document (`structured`). The session owns source bytes, revisions, and history. The structured tree owns typed transactions and its own history. A buffer projection is not a second undo log, and structured editing does not serialize Markdown. Directed source selections, literal search and replacement, whole-line folds, and a staged source-preserving projection are in the same experimental surface. The [source-backed editing note](docs/source-backed-editing.md) is the contract; design records under `docs/design/` are not a list of stable APIs.
- Markdown host slots. `mount_markdown_slot` binds a host-owned subtree to one block's source byte range. An insert above that range, or a streaming append after it, keeps the slot on that block. An edit that deletes the range, or overlaps it without containing it, retires the slot. After layout, the slot height is measured from the mounted subtree. `set_height` is an explicit override of that measurement. Copy still emits the original markdown. The parser does not run the host's controls.
- Opt-in close guards. With no `CloseGuard`, primary close still exits, a later window's close still removes only that window, and `Input::quit` still exits the process. With a guard, native close and quit become a request. A pending or failed guard keeps the window, including windows that are hidden or opened while the request is draining.
- Headless IME coverage for preedit, commit, cancel, and caret keys on the field, the prompt, the editor, an attached source session, and a structured document, using the strings `にほん`, `汉字`, and `مرحبا`. Those tests do not open an operating-system input method. `reading_layout` is still half an em times the character count, which is a poor column estimate for CJK and Arabic. Japanese, Chinese, Korean, and Arabic input on macOS, Windows, and a live Linux input method were not run. See [IME and complex scripts](docs/ime-complex-scripts.md).
- Native observation, feature `observe`, off by default. On Unix, after the application opts in and attests the content, the runner can record the frame it submitted and a flattened accessibility tree. There is no Windows transport. The recording is not component capture, and it is not proof that a compositor showed the frame.
- Headless component capture, the same `observe` feature. `capture` reads the offscreen viewport composite inside the target's box, so a sibling can cover it. `capture_isolated` paints that subtree alone and then crops the same box. Metadata is ids, bounds, clips, and hidden state. Accessible names are left out, including for content the host called public. Any `ObservationSensitive` entity denies the whole capture. The `rndr-capture` binary renders a declarative scene to PNG and JSON and does not open a window. This is offscreen wgpu, not a desktop screenshot. See [component capture](docs/component-capture.md).
- Hidden-window GPU startup. A window that is visible at creation waits for the device on the UI thread. A window that starts hidden creates its surface on the UI thread, then creates the adapter, device, and pipelines on an `rndr-gpu` thread. Show and focus reuse that device. `StartupTrace` records `window_ready`, `device_ready`, and `first_present`. No frame was presented while this was written, so there is no measured time-to-first-present. See [startup budget](docs/startup-budget.md).
- Overlay panel requests, checked as placement math and Win32 constants. No Wayland compositor was driven, no Windows desktop was driven, and the existing macOS panel was not launched again. `platform_support` is the table; `Unverified` means the runner sends the request and no session in this repository has shown the OS honor it.
  - Wayland: a panel asks for `zwlr_layer_shell_v1`. `Position::Center` anchors no edge. `Position::Platform` anchors the top edge. `Position::At` anchors the top-left and sets margins. The layer is `overlay` when the window is a panel or always-on-top, and `top` otherwise. Exclusive keyboard is requested only while the panel is visible and focus was requested. If the compositor does not advertise the protocol, the panel falls back to a winit toplevel. GNOME does not implement this protocol. Position, hide, focus, stacking, skip-taskbar, above-fullscreen, and all-spaces are `Unverified`. `panel_nonactivating` stays `Ignored`: the keyboard mode takes the keyboard, which is not a non-activating focus. A layer-shell panel does not publish an accesskit tree, and `zwp_text_input_v3` is not bound.
  - Windows: a panel show sets `WS_EX_NOACTIVATE`, raises with `HWND_TOPMOST`, and uses `SW_SHOWNOACTIVATE`. An explicit focus clears the no-activate bit and calls `SetForegroundWindow`. `panel_above_fullscreen` is `Unverified`. `panel_all_spaces` stays `Ignored`; Windows has no stable public API for every virtual desktop, and the undocumented shell interfaces are not called. `panel_nonactivating` stays `Ignored`. Position, hide, focus, always-on-top, and skip-taskbar stay `Supported` on the strength of the existing winit calls, which this change did not re-run on a Windows machine.
  - See [overlay windows](docs/overlay-platform.md).

### In this checkout, not uploaded

- `rndr_ui` 0.4.0. Semantic themes, controls, retained lists and trees, a uniform-height virtual `ListView` used by the palette, data tables, a property inspector, checkbox, single-select, and multi-select. On a collection row, screen-reader Activate selects; keyboard Enter emits `Activated` and does not select. Popup and selection behavior is covered by headless tests, not by VoiceOver, Narrator, or Orca. The manifest keeps `publish = false` ("dependency/publication policy is not release-ready"). This release does not flip that.
- `rndr_app` 0.4.0 and `examples/desktop`. The starter installs form navigation, scrolling, and scrollbars, and stores bounded JSON preferences. Desktop Workshop is the offline example. `scripts/package-desktop-macos.sh` writes `CFBundleShortVersionString` `0.1`; that string is example metadata, not this release. Native typing, IME, clipboard, VoiceOver, and signed distribution were not re-run for 0.4.0. [Desktop readiness](docs/desktop-readiness.md) records an earlier macOS session and still lists those checks as open.
- The `rndr` facade at 0.4.0 adds optional `ui` (`rndr::ui`, with no glob into the legacy root) and optional `observe`. crates.io `rndr` remains 0.3.1, which has neither feature, until the `rndr_ui` dependency decision above is made. `scripts/check-facade-archive.py` checks that the facade tarball normalizes and matches source. It does not resolve the registry and it does not upload.

## 0.3.1

- Text no longer needs a background `Color` to be painted. Labels and styled text with a `Rect` now emit glyphs and text decorations even when their entity has no background. A missing `Color` leaves the background unfilled; an explicit transparent color stays transparent.
- Headless regressions cover uncolored labels through fallback and frame layout, transparent and opaque backgrounds, label and theme text colors, hidden and clipped text, text marks and caret geometry, and uncolored containers.

## 0.3.0

- Headless export takes one process-wide lock around device creation, submission, and destruction. Lavapipe faults when several threads create and destroy wgpu devices together. The queue is drained before the device is dropped.
- A drop shadow is blurred on the shape's own pixel grid, over the shape bounds plus the kernel. The grid stays fine when the view is zoomed, so the falloff does not stretch into blocks. Panning, including a pan that clips the shape, does not change the shadow. The viewport clips it only when the shadow is composited.
- Several windows on one event loop. `Session` holds one app per window, each with its own world and surface. `Desktop` sends and broadcasts `WindowMsg`, and can open another window after the loop has started. Closing the first window exits the process. Closing a later window removes only that window. Panel mode is unchanged. `examples/windows` is two windows that tell each other about a save. They sit side by side with a gap.
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
- Vector paths. Fills use non-zero and even-odd rules. Strokes take a width, caps, joins, a dash array, and a hairline that stays one device pixel wide. Ellipse, polygon, line, and rounded-rect helpers build the path. Tessellation runs on the CPU and is cached by geometry and zoom bucket. An open subpath stays open: the next `move_to` does not close it, so a dashed gap is not filled.
- Edges of paths and layers are 4× multisampled on the offscreen target, then resolved before a blur, an opacity composite, or the quad pass. UI quads keep their signed-distance edges. A device that cannot resolve 4× draws the same mesh at one sample.
- 2D affine transforms per command, plus a root camera. Device pixels are world units times the camera scale times the viewport scale. A local map is applied before the camera, so a rotated child stays put under zoom and pan.
- PNG, JPEG, and WebP decode off the UI thread. A GPU texture cache evicts least-recently-used images to a byte budget, builds mipmaps, tiles past the device texture limit, and crops with a UV rect and rounded corners. List rows can show an image with `ImageView`. The icon is centered on the row's cross axis and clipped to the row. Samples sit on texel centers, so a linear filter does not darken the fringe.
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
- `rndr_paint`'s `highlight` feature forwards `rndr_core`'s fenced-code token colors. The markdown paint snapshot keeps a second file when that feature is on.
- A press and a release in the same frame both stay set, so a click with no gap still activates a button.
- A wheel over a list scrolls the rows and leaves the selection where it is.
- Palette row icons use the same left inset as the label, and the icon is solid through its edge.
- A rounded frame clips a child that crosses the corner.

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
