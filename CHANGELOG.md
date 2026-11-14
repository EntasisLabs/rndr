# Changelog

## Unreleased

### Tab strip

- `rndr_ui::tabs::TabStrip` is an editor and document tab strip. A tab has a label, an optional icon, a dirty mark, an optional pin, and a close button. Middle-click, Delete, and Ctrl+W (Cmd+W on macOS) ask the host to close when the tab allows it. The tablist uses roving focus: Left/Right/Home/End move, Enter and Space activate. Overflow scrolls horizontally, with chevrons and a menu of the tabs that are not fully in view. The selected tab is scrolled into view when it changes and when the strip first has a size. Drag uses the item-drag primitive. Before/after zones reorder inside a strip or move a tab onto another strip. A pane-edge drop emits a split request (direction and tab id) the host can mount as a `SplitSash`. Alt+Left and Alt+Right move a tab the same way. The strip does not apply order or splits; the host does, as with `Reordered`. Moves are announced on the polite live region. See [tab strip](docs/tab-strip.md). `cargo run -p rndr_ui --example tab_strip`.

### Split sash

- `rndr_ui::split::SplitSash` is a focusable splitter built on core `Splitter`. Pointer drag and clamping stay in `apply_split`. The sash adds a slider role, arrow keys with a configurable step, Home/End, double-click reset to a default ratio, an accessible name and value, and a `SplitSashChange` event the host can persist. Horizontal and vertical splits nest. A core handle on its own stays non-focusable. See [split sash](docs/split-sash.md).

- In-app item drag, separate from file drop. A `DragSource` carries a host `DragPayload` (kind, value, and optional owner). A press stays a click until it crosses the scroll slop, then a preview follows the pointer and does not steal the hit. `DropTarget` accepts or rejects per payload. Hover reports before, after, inside, or an edge zone so a later tab strip can drop-to-split; this change does not build that strip or a kanban. Escape and a release outside an accepted target cancel. A drag near a scrollport edge autoscrolls, including virtual lists. Alt+Enter picks up a generic source; list and tree reorder use Alt+arrow. Announcements go to a polite live region. `ViewProps::reorder` is off by default. List and tree emit `Reordered` and leave the model to the host (`reorder_list`, `reorder_tree`). See [item drag](docs/item-drag.md). `cargo run -p rndr --example item_drag`.

## 0.6.0

Workspace 0.6.0. `scripts/publish-crates.sh` uploads sixteen crates in this order: `rndr_core`, `rndr-flow`, `rndr_paint`, `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_liquid`, `rndr_chart`, `rndr_liquid_ui`, `rndr_canvas`, `rndr_canvas_ui`, `rndr_render`, `rndr_media`, `rndr_browser`, and `rndr`. First crates.io publish for `rndr-flow`. Examples, including Desktop Workshop, `source-session`, `titlebar`, and `lifecycle`, stay `publish = false`. Minimum Rust version remains **1.95**. 0.6.0 is a 0.y release, not a 1.0 stability promise. See [package policy](docs/ui-status.md#package-policy).

### New publishable crate

- `rndr-flow`: optional headless scope ownership for an application-owned world. Depend on package `rndr-flow` and import `rndr_flow`; there is no facade feature or automatic re-export. Covers F0–F5 foundations: lifecycle ownership and identity, bounded async work with cancellation and native completion gates, retained transitions, and coordinated close guards (including opt-in all-World Session validation). F6 and application migration are not included. Qualification is headless; native GUI/event-loop/platform behavior is not certified. Destruction is nontransactional. See [`rndr-flow`](crates/rndr_flow/README.md) and [session close validation](docs/design/rndr-flow-f5-session-close-validation.md).

### Liquid Markdown host

- Stream Liquid with `push_liquid_str` / `LiquidStream`: append reparses only past the last stable prefix, finished blocks stay mounted, and an unclosed fence shows a pending skeleton until it closes. `set_liquid_source` keeps unchanged block reader state on append or identical replacements.
- Persist Liquid component state across host-slot recycling and publish interaction envelopes for host-facing events.
- Recipe step timers (`Start` / `Pause` / `Resume` / `Reset`) survive recycling; deadlines use the app clock and emit `timer_*` envelopes (plus `LiquidEvent::StartTimer` on start).
- Feeds and dashboard tiles load through `LiquidHost::load_feed` without the renderer opening a network connection. Loads move through pending, ready, failed, and stale; a later failure keeps the last ready body. Tiles refresh manually or on an interval.
- Visual fidelity for Liquid blocks: styled prose fonts/size/alignment/line spacing, colored code tokens with a code accessibility role, chart tooltip overlays, and positioned slide image layers on a 640×360 stage.

### Markdown host slots and editing

- Markdown views virtualize blocks: only the visible window plus overscan is realized; heights are measured when realized and estimated otherwise. Host-slot roots stay stable across scroll-away. Opt-in recycling releases offscreen host slots when a view asks for it. **Migration:** `children_of(view)` is the realized window, not one child per block; keep document data on `MarkdownView::blocks()`. See [markdown host slots](docs/markdown-host-slots.md).
- Source commits and undo history store splices instead of cloning the whole document; views apply only the edited range.
- Plain-text editor scroll queries binary-search row tops; plain-text row heights are indexed instead of retaining every caret edge. Scroll anchors and whole-line folds survive edits. Idle frames skip reshaping labels whose stored layout is still current. Structured text hit-testing follows indented layout.

### Packaging

- Publish order and CI archive checks include `rndr-flow`. The facade package glob is narrowed so `rndr-flow` is not mistaken for `rndr`.

### Dependencies and known limitations

- **Known limitation (Linux `browser` feature):** unchanged from 0.5.0 — `glib` **0.18.5** remains via the pinned `gtk`/`webkit2gtk` stack (`RUSTSEC-2024-0429`). The optional facade `browser` feature stays off by default. Accepted for 0.6.0 with this disclosure.

## 0.5.0

Workspace 0.5.0. `scripts/publish-crates.sh` uploads fifteen crates in this order: `rndr_core`, `rndr_paint`, `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_liquid`, `rndr_chart`, `rndr_liquid_ui`, `rndr_canvas`, `rndr_canvas_ui`, `rndr_render`, `rndr_media`, `rndr_browser`, and `rndr`. First crates.io publish for `rndr_liquid`, `rndr_chart`, `rndr_liquid_ui`, `rndr_canvas`, `rndr_canvas_ui`, `rndr_media`, and `rndr_browser`. Examples, including Desktop Workshop, `source-session`, `titlebar`, and `lifecycle`, stay `publish = false`. Minimum Rust version is **1.95** (`AtomicU64::try_update` in `rndr_core`). 0.5.0 is a 0.y release, not a 1.0 stability promise. See [package policy](docs/ui-status.md#package-policy).

### New publishable crates and facade features

- `rndr_liquid`: headless Liquid Markdown grammar. Component fences parse to typed blocks; anything that does not parse stays a code fence.
- `rndr_chart`: ten chart families (bar, line, area, pie, donut, radar, radial, scatter, combo, heatmap) laid out headlessly and painted with `rndr_paint`.
- `rndr_liquid_ui`: mounts Liquid blocks in markdown host slots with `rndr_ui` controls and `rndr_chart` paint surfaces, behind a `LiquidHost`.
- `rndr_canvas`: headless canvas documents (ink, text, shapes, lines, images, groups, frames), transactions with undo/redo, geometry, and storage. Reads Medousa Create v3 scenes and writes `rndr-canvas` v1.
- `rndr_canvas_ui`: paints and edits canvas documents: scene painter, headless editor, overlay, `CanvasView`, toolbar and layers panels.
- `rndr_media`: native video and audio `Player` (GStreamer on Linux, AVFoundation on macOS). Video frames go to a `FrameSurface` for `SurfaceView`. Optional facade feature `media`, off by default.
- `rndr_browser`: optional embedded browser, facade feature `browser`, off by default. Ordered immediately before `rndr` because the optional dependency is resolved even when the feature is off. Enabling it does not enable `dialog`.
  - macOS WKWebView: show, hide, navigate, title, URL, load outcome, back/forward, downloads and file-input intents as host answers. Client rects are logical points from the top-left of the parent. Server-trust challenges use the system certificate check; other authentication and 401/407 responses are cancelled. Find and script stay unsupported. Native cells stay unverified.
  - Windows WebView2: same host session contract. Uses an already installed runtime; a missing runtime is `BrowserError::RuntimeMissing`. The crate does not download or bundle one.
  - Linux WebKitGTK on an X11 parent: same host session. A winit Wayland surface cannot host the GTK widget; a missing display is `BrowserError::Prerequisite`.
- Facade optional features `chart`, `liquid` (enables `chart`), `canvas`, `media`, and `browser`, all off by default.

### Embedded browser, media, and surfaces

- `FrameSurface` and `SurfaceView` for latest-wins video frames, with fit modes for embedding media in layout.
- Media controls: play, pause, seek, rate, looping, mute; state, time, duration, natural size; `position()` as a `Live<f32>`; worker-thread wakers for state changes.
- `rndr::media::Recorder` records the default input: WAV, Ogg Opus or FLAC on Linux (GStreamer) and AAC (`.m4a`) or WAV on macOS (`AVAudioRecorder`), with pause, resume, cancel, elapsed time, a `Live<f32>` input level, and macOS microphone permission checks/requests. Windows recording is not claimed. See [`rndr_media`](crates/rndr_media/README.md).

### Window chrome, launches, links, and the app icon

- Custom window chrome: `WindowConfig::titlebar: Some(Titlebar::new(40.0))` draws an app titlebar. On macOS the content runs under a transparent title bar with traffic lights centred in the band (or placed with `Titlebar::traffic_lights`); `WindowChrome::leading` reports their width. On Windows and Linux the native frame is removed, `rndr_ui::window_controls` draws minimize/maximize/close, and a resizable window gets 6 pt resize edges with matching cursors. `WindowDrag` moves the window on press while nested buttons and fields keep their presses; double-click maximizes (or follows the macOS setting); right-click opens the window menu on Windows. `WindowConfig::backdrop` asks for Mica, Acrylic or Tabbed on Windows 11 and vibrancy on macOS when the window is `transparent`. Linux X11 is tested natively; macOS and Windows are compile-checked only. On macOS, AppKit titlebar drag no longer moves the window from buttons in a custom band. `cargo run -p titlebar` shows them.
- Single-instance launches, incoming links, Dock reopen, and the app icon: `rndr_render::single_instance(id)` keeps one process per user (a later launch hands arguments to the first and returns `false`; macOS already keeps one process). The primary window's world gets `UrlOpened` for each `myapp://…` link and `Reopened` for a launch without a link or a macOS Dock click, after the runner shows and focuses that window. Links pass through unchecked. `set_app_icon(png_or_jpeg)` sets the Dock icon on macOS and each window's icon on Windows and X11. Linux X11 is tested natively; macOS and Windows are compile-checked only. `cargo run -p lifecycle` shows them.

### Browser (wasm32) text input and IME

- While a text field has focus in the wasm32 WebGPU runner, rndr focuses a hidden `<textarea>` at the caret: keys still reach winit, composition becomes IME preedit and commit, and the browser `paste` event puts the other app's plain text and HTML on the clipboard before paste runs, without a permission prompt. Tested in Chrome on Linux only; the bundled fonts have no CJK glyphs. Other browsers are untested.

### Core, layout, paint, and the desktop runner

- Richer layout: align, justify, per-side padding, max size, aspect ratio, absolute children, wrap, grid, sticky headers, and shift-wheel horizontal scrolling. Hug labels stay within `MaxSize`/`AspectRatio`.
- SVG decoding and rounded, feathered subtree clips. Subtree blur and backdrop blur with motion and live bindings.
- GPU effect slots: application wgpu passes in paint order with an optional backdrop.
- Live values: thread-safe bindings to opacity, offset, scale, fill, rect, text, border, shadow and gradient colours, plus `LiveText` labels. Live-only frames skip idle work; redraws stop while the OS minimizes or covers a window.
- Colour animation, keyframes, size motion, press scale, scroll glides, fling, rubber-banding, and shared-element flights.
- OpenType features, variable axes, strikethrough, and selectable labels. Structured documents gain underline, strike, code marks, atomic mention pills, safe HTML import/export, and HTML clipboard round-trips with composer keys.
- Secondary gesture: right-click, macOS Control-click, Menu key, and Shift+F10. Drawn context and popup menus in `rndr_ui::menu`, with point and beside placement and nested popup scopes. Native app menu bar on macOS and Windows through `muda` (`menubar` path).
- OS basics: open the default browser for a URL, CSS cursor set, system appearance, and reduced motion.
- Range and multiple selection on lists and tables. `None` and `Single` still publish `Selected(Option<ItemId>)`; `Multiple` and `Range` emit a set-changed event with no id list. See [selection modes](docs/selection-modes.md).
- `FileHover` and `FileDrop` carry `position: Option<[f32; 2]>` in logical points. The desktop runner follows winit 0.31.0-beta.4 drag events: client physical pixels are divided by the window scale; every path in one transfer shares that point. A callback with no point leaves the stored point unchanged. A non-finite point clears it. A path list is applied only for the fetch still outstanding on the active drag. `FileHoverCancel` still clears every hovered path. Wayland drag events are not accepted. `accesskit_winit` 0.34 is still built on winit 0.30, so the accesskit feature uses a local adapter. See [file drop](docs/file-drop.md).
- Windows pen and eraser input follows winit tablet-tool pointer and button events: motion, tip contact and release, pressure, tilt, and twist. The macOS pen monitor is unchanged and does not run on Windows. No tablet session was run.
- Linux screen-reader bounds use the window's outer position plus the window-relative surface offset.
- A dragged file URI keeps its platform path. Windows `file://server/share/...` stays a UNC path. On Unix, `file:///C:/notes.txt` stays `/C:/notes.txt`, and a percent-encoded non-UTF-8 byte stays in the path.
- Optional wasm32 WebGPU browser runner for `rndr_render` (`examples/web`). The example stays unpublished.

### Dependencies and packaging

- `pulldown-cmark` minimum raised to **0.13.6** (RUSTSEC-2026-0433). rndr's markdown parser never enabled `ENABLE_WIKILINKS`; the bump still clears the advisory.
- Lockfile picks up non-yanked `clipboard-win` **5.4.3** (Windows `arboard` path) and `async-executor` **1.14.1** (Linux accesskit/tray/`zbus` path).
- **Known limitation (Linux `browser` feature):** `glib` **0.18.5** remains via the pinned `gtk`/`webkit2gtk` **0.18** / **2.0.2** stack (`RUSTSEC-2024-0429`, unsound `VariantStrIter`). rndr_browser does not call `VariantStrIter` / `array_iter_str`. Whether transitive GTK/WebKit bindings reach those iterators is not proven here. There is no newer `webkit2gtk` release on the GTK 3 binding line that pulls `glib >= 0.20`. The optional facade `browser` feature stays off by default. Accepted for 0.5.0 with this disclosure; revisit when upstream WebKitGTK bindings move.
- Packaging dry-run and CI derive `--exclude` and the expected archive set from publishable workspace packages, and reject both missing and unexpected archives (so unpublished examples such as `titlebar` cannot enter the upload set unnoticed).

## 0.4.0

Workspace 0.4.0. `scripts/publish-crates.sh` uploads eight crates in this order: `rndr_core`, `rndr_paint`, `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_render`, and `rndr`. `rndr_std` and `rndr_term` have no behavior change in this release; they publish at 0.4.0 with the others so one version resolves. Examples, including Desktop Workshop and `source-session`, stay `publish = false`. 0.4.0 is a 0.y release. It adds public API, including `rndr_ui`, `rndr_app`, and the facade features `ui` and `observe`. It is not a 1.0 stability promise. See [package policy](docs/ui-status.md#package-policy).

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

### Semantic UI, the app starter, and the facade

- `rndr_ui` 0.4.0. Semantic themes, controls, retained lists and trees, a uniform-height virtual `ListView` used by the palette, data tables, a property inspector, checkbox, single-select, and multi-select. On a collection row, screen-reader Activate selects; keyboard Enter emits `Activated` and does not select. Popup and selection behavior is covered by headless tests, not by VoiceOver, Narrator, or Orca.
- `rndr_app` 0.4.0. The starter installs form navigation, scrolling, and scrollbars, and stores bounded JSON preferences. It does not depend on `rndr_ui` and the facade does not depend on it. Desktop Workshop (`examples/desktop`) is the offline example and stays unpublished. `scripts/package-desktop-macos.sh` writes `CFBundleShortVersionString` `0.1`; that string is example metadata. Native typing, IME, clipboard, VoiceOver, and signed distribution were not re-run for 0.4.0. [Desktop readiness](docs/desktop-readiness.md) records an earlier macOS session and still lists those checks as open.
- The `rndr` facade 0.4.0 re-exports the world, paint, filesystem, and window. Optional `ui` exposes `rndr::ui` with no glob into the legacy root, and depends on `rndr_ui`. Optional `observe` enables the Unix whole-window probe and headless component capture, including `rndr-capture`. Both features are off by default. `hotkey`, `tray`, `parley`, and `accesskit` stay on by default.

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
