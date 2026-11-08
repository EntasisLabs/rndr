# rndr

**A native desktop application toolkit for Rust, built around an application-owned world.**

Build interfaces from entities and components, compose them with retained semantic UI,
and render text, images and vector graphics through a GPU-backed desktop window.
The same core can run without a window or GPU for application logic, layout and tests.

rndr brings UI, document editing and desktop integration into one explicit frame
lifecycle. The application owns its state; components express behavior; the renderer
presents the result. There is no third-party ECS or external layout engine.

## What you can build

- **Desktop tools and workspaces.** Compose themed controls, toolbars, lists, trees,
  data tables, property inspectors, scroll regions and resizable panes.
- **Document and content interfaces.** Display streaming Markdown, embed host-owned
  controls in Markdown, edit source-backed text, or work with a structured document
  tree with typed transactions and its own undo history.
- **Launchers and background utilities.** Combine command palettes, global shortcuts,
  tray menus, hidden windows, asynchronous tasks and bounded streaming feeds.
- **Graphics and embedded surfaces.** Draw paths, images and layers, build pan/zoom
  scenes, or embed a PTY-backed terminal without making it a core dependency.

These are working capabilities with examples, not a claim that every component,
platform or editing path is production-certified. See [status and boundaries](#status-and-boundaries).

## Start here

Work from this checkout for the current integrated toolkit. Rust **1.95 or newer**
is required; the repository follows the stable toolchain.

```sh
git clone https://github.com/EntasisLabs/rndr.git
cd rndr

# A minimal native window.
cargo run -p idle

# An offline starter with settings and cancellable local work.
cargo run -p desktop -- --memory

# A searchable command palette with a virtualized list.
cargo run -p palette

# Semantic controls, themes and layout.
cargo run -p rndr --example ui_gallery --features observe
```

The desktop stack uses winit and wgpu. Default features enable Parley text shaping,
AccessKit accessibility, global hotkeys and tray integration. Linux builds need the
native development libraries used by winit/wgpu and fontconfig; the repository's
[CI configuration](.github/workflows/ci.yml) records the build packages. Runtime
capabilities also depend on the desktop session and available graphics backend.

**Release 0.4.0** publishes eight crates, in order: `rndr_core`, `rndr_paint`,
`rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_render`, and `rndr`.
The facade's `ui` feature uses `rndr_ui`. `ui` and `observe` are off by default.
Examples, including Desktop Workshop and `source-session`, stay `publish = false`.
This is a 0.y release, not a 1.0 stability promise.
See [package policy](docs/ui-status.md#package-policy).

### A small application

The desktop facade exposes the core world, paint and native runner:

```rust,no_run
use rndr::{
    run, App, Color, Label, Layout, LogicalSize, Schedule, WindowConfig,
};

fn setup(world: &mut rndr::World) {
    world.spawn((
        Layout { padding: 24.0, ..Layout::column() },
        Color::rgb(0.12, 0.13, 0.16),
        Label::new("Hello from rndr"),
    ));
}

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "Hello, rndr".into(),
        size: LogicalSize::new(640.0, 480.0),
        ..WindowConfig::default()
    });
    app.add_system(Schedule::Startup, setup);
    run(app);
}
```

For higher-level components, enable the facade's **`ui` feature** and install
`rndr::ui::UiPlugin`. UI names stay explicitly scoped: `rndr::ui::controls::Button`
is a semantic control; the existing root `rndr::Button` remains the core API.
`UiPlugin` shares core interaction, focus and scrolling rather than installing a
second runtime. Start with the [UI composition guide](crates/rndr_ui/README.md).

## How it fits together

```text
Application state, systems and document authorities
                         |
          rndr_ui        |  semantic composition (optional)
                         v
                    rndr_core
          world / hierarchy / layout / input
          text / editing / events / tasks / feeds
                         |
                         v
                    rndr_paint
          quads + paths + images + layers
                         |
                         v
                    rndr_render
          native windows / text shaping / wgpu
```

| Crate | Responsibility |
| --- | --- |
| [`rndr`](crates/rndr) | Desktop facade and feature selection |
| [`rndr_core`](crates/rndr_core) | GPU-free ECS, hierarchy, layout, input, focus, text, document models, events and scheduling |
| [`rndr_ui`](crates/rndr_ui) | GPU-free semantic themes, controls, collections, data editors and document composition |
| [`rndr_paint`](crates/rndr_paint) | World-to-paint projection, display operations and headless paint snapshots |
| [`rndr_render`](crates/rndr_render) | Native event loop, windows, text realization, GPU rendering and desktop integration |
| [`rndr_std`](crates/rndr_std) | Process-filesystem backend; core also supports an in-memory filesystem |
| [`rndr_liquid`](crates/rndr_liquid) | Headless Liquid Markdown grammar: component fences parsed to typed blocks |
| [`rndr_chart`](crates/rndr_chart) | Ten chart families laid out headlessly and painted with `rndr_paint` |
| [`rndr_liquid_ui`](crates/rndr_liquid_ui) | Liquid blocks mounted natively in markdown views, with a `LiquidHost` for app behavior |
| [`rndr_canvas`](crates/rndr_canvas) | Headless canvas documents: nodes, transactions, undo, geometry and storage; reads Medousa Create v3 scenes |
| [`rndr_canvas_ui`](crates/rndr_canvas_ui) | Paints and edits `rndr_canvas` documents: scene painter, editing tools, overlay, toolbar, layers and inspector |
| [`rndr_term`](crates/rndr_term) | Optional embedded terminal: PTY, VT state and cell projection |
| [`rndr_browser`](crates/rndr_browser) | Optional embedded browser. Off by default. Publish-eligible and ordered immediately before the facade. macOS WKWebView, Windows WebView2, and Linux WebKitGTK on X11 |

### One frame, explicit ownership

1. The platform writes input and viewport metrics in logical points.
2. `rndr_paint::frame` runs Startup once, then Input, Update, PostUpdate and Layout.
   Events emitted during Input are merged before Update reads them.
3. Paint projects the world into quads and a vector display list; Render systems run.
4. The native surface applies the display scale and presents the frame.

The renderer does not own hierarchy, document history or application state.
Headless tests drive the same core/frame path without opening a native window.
The desktop loop waits when idle and wakes for input, tasks, feeds and timers;
applications do not need a continuously polling render loop.

Source-backed and structured editing have **distinct document authorities**.
A source session owns source admission, revisions and history. A structured tree
owns typed transactions and tree history. Their disposable text projections do not
become a second undo authority, and structured editing is not Markdown serialization.

## Explore the examples

Run these from the repository root. Gallery `observe` builds make native observation
available, but observation still requires an explicit application-session opt-in.

| Explore | Command |
| --- | --- |
| Controls and themes | `cargo run -p rndr --example ui_gallery --features observe` |
| Checkboxes and selection popups | `cargo run -p rndr --example selection_gallery --features observe` |
| Context, popup and app menus | `cargo run -p rndr --example menus` |
| Tables and property inspection | `cargo run -p rndr --example data_gallery --features observe` |
| Typed data-editor drafts | `cargo run -p rndr --example data_editors_gallery --features observe` |
| Virtualized collections | `cargo run -p rndr --example virtual_list_gallery --features observe` |
| Markdown with host-filled slots | `cargo run -p rndr --example markdown_gallery --features observe` |
| Structured document editing | `cargo run -p rndr --example structured_gallery --features observe` |
| Source-session editing host | `cargo run -p source-session` |
| Streaming Markdown | `cargo run -p stream` |
| File tree, split pane and editing | `cargo run -p expand` |
| Hidden overlay and global shortcut | `cargo run -p overlay` |
| Multiple windows and messages | `cargo run -p two-windows` |
| Embedded shell | `cargo run -p terminal` |
| Pan/zoom vector scene | `cargo run -p canvas-scene` |

## Features

The facade keeps optional integrations explicit:

| Feature | Default | Enables |
| --- | --- | --- |
| `parley` | On | Shaping, bidi, line breaking and glyph rasterization |
| `accesskit` | On | Native accessibility adapter; the core accessibility tree stays GPU-free |
| `hotkey` | On | System-wide shortcuts through the platform backend |
| `tray` | On | Menu-bar / notification-area integration |
| `menubar` | Off | Native app menu bar on macOS and Windows through `muda` (`set_app_menu`, `AppMenuPicked`). Linux and browsers have none; use `rndr_ui::menu` |
| `svg` | On | SVG and SVGZ decoding through `resvg` (`decode_svg`, `load_svg`; `load_image` accepts SVG). Text in an SVG must be converted to paths |
| `appearance` | On | Linux dark/light, accent and reduced-motion settings through the XDG desktop portal. Off leaves Linux on winit's theme only; macOS, Windows and browsers need no extra dependency |
| `ui` | Off | Semantic components under `rndr::ui` |
| `dialog` | Off | Native open/save dialogs |
| `notify` | Off | Present, dismiss, and a host-owned click token. Feature off returns unsupported. A click does not activate a window. Native cells stay unverified or unsupported |
| `term` | Off | Embedded terminal re-exports and PTY dependency |
| `browser` | Off | Embedded browser. Does not enable `dialog`. macOS WKWebView, Windows WebView2, and WebKitGTK on X11. WebKitGTK on Wayland, find, and script are unsupported. Native cells stay unverified |
| `observe` | Off | Explicitly opted-in live native-window observation |
| `tokio` | Off | Feed producers on an application-provided Tokio runtime |
| `chart` | Off | `rndr_chart` under `rndr::chart` |
| `liquid` | Off | Liquid Markdown under `rndr::liquid` (grammar) and `rndr::liquid::ui` (native blocks, `LiquidHost`). Enables `chart`. Icons, slide visuals and styled-block fonts are not drawn yet; see [`rndr_liquid_ui`](crates/rndr_liquid_ui/README.md#current-limits) |
| `canvas` | Off | Canvas documents under `rndr::canvas` (model, history, geometry, storage) and `rndr::canvas::ui` (painter, editor, overlay, panels). Text editing is end-only and the app owns keyboard focus; see [`rndr_canvas_ui`](crates/rndr_canvas_ui/README.md#limits) |

`--no-default-features` removes the default integrations, **not the desktop runner**.
Depend on `rndr_core` (and optionally `rndr_ui` / `rndr_paint`) when no window or GPU
is wanted. The default executor does not require Tokio.

### In a browser

`rndr_render` and `rndr` also build for `wasm32-unknown-unknown`. The app draws
into one WebGPU canvas: the page's `<canvas id="rndr">`, or one appended to
`<body>`. GPU setup is asynchronous, frames come from `requestAnimationFrame`,
and background tasks run on the page's main thread between frames. Text uses
the bundled fonts, since a browser exposes no system fonts.

`hotkey`, `tray`, `dialog` and `notify` compile to nothing on wasm32, so default
features build there. Copying from the app writes text, or HTML with a plain-text
fallback, to the browser clipboard. Paste reads only rndr's own clipboard;
pasting from other apps, IME and browser text input are not wired yet.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version <version in Cargo.lock>
scripts/web-build.sh
python3 -m http.server -d examples/web 8000
```

Verified in Chrome 156 with WebGPU on a software adapter (SwiftShader), not on
a hardware GPU. Other browsers are untested.

## Status and boundaries

rndr is under active development. Implemented behavior, automated evidence and
native-platform acceptance are different things:

- **Semantic UI:** retained controls and collections share core focus/interaction.
  Uniform-height virtual lists are supported; not every collection is virtualized.
  See the [UI guide](crates/rndr_ui/README.md), [virtual-list contract](docs/virtual-list.md)
  and [integration status](docs/ui-status.md). Status documents also preserve earlier
  milestone boundaries; use the feature-specific guides for subsequent additions.
- **UI compositing:** `Opacity` and `Transform` (translate, scale, rotate about a
  pivot) apply to any laid-out entity and its subtree. The subtree is painted as
  one offscreen group and composited once; layout is unaffected and hit testing
  follows the transform. Positional gestures inside a transformed control (caret
  placement, slider drags) still read layout coordinates, a scaled group samples
  its unscaled raster, and foreign surfaces (webviews) are not transformed. There
  is no blur or backdrop effect yet.
  `cargo run -p rndr --example ui_compositing -- out.png` renders headless pixels.
- **Motion:** `rndr_core::motion` animates opacity, offset, scale, and rotation
  with closed-form springs and tweens sampled at the frame time. Retargeting keeps
  velocity; enter/exit transitions keep an exiting entity until it settles;
  `LayoutMotion` glides position changes (size still snaps). The desktop runner
  redraws while anything moves. Not yet: shared-element moves, choreography,
  fling/decay, keyframes, and off-thread animation.
  `cargo run -p rndr --example motion_strip -- out.png` renders a frame strip.
- **Decorations:** `Corners` (per-corner radii), `Gradient` (linear at any angle or
  radial, multi-stop), `Shadows` (outer and inset box shadows with offset, blur and
  spread) and `BorderSides` (per-side widths, optional dash) on any laid-out entity.
  They are drawn analytically on the quad pipeline, with no offscreen passes.
  `cargo run -p rndr --example decor_gallery -- out.png` renders a gallery.
- **Rounded and soft clips:** `ClipRounded` masks an entity and its subtree to its
  per-corner rounded rect, optionally with a feathered edge, in the quad shader.
  It applies to fills, decorations and text; images and canvas drawings keep the
  rectangular clip. Overlapping quads are masked one by one, so a lower layer can
  show through inside a feathered edge. Hit testing stays rectangular.
  `cargo run -p rndr --example svg_clip_gallery -- out.png` renders SVG icons and clips.
- **Layout options:** optional components extend `Layout` without changing it.
  `Align` sets main-axis `Justify` (start, center, end, space-between/around/evenly)
  and cross-axis `Items` (start, center, end, stretch); `AlignSelf` overrides per
  child. `Padding` gives per-side insets, `MaxSize` caps size (minimums win) and
  `AspectRatio` derives the free axis. `Absolute` children leave the stack, pin to
  any parent edges, scroll with content and paint and hit-test after in-flow
  siblings. `Wrap` breaks a row onto lines. Children are still clipped to their
  parent's rect, so an absolute child cannot overflow it.
  `cargo run -p rndr --example layout_gallery -- out.png` renders a gallery.
- **Grid, sticky and horizontal scroll:** `Grid` lays children out in `Track`
  columns (`Px`, `Fr`, `Auto`) with column and row gaps, filling rows in order;
  `GridSpan` takes several columns. `Sticky::top` / `Sticky::bottom` pin a child
  to the nearest vertically scrolling `Scroll` ancestor until its parent scrolls
  away, painting and hit-testing above in-flow siblings; with no such ancestor it
  stays in place. A `Scroll` row clamps and scrolls on X independently of Y, and
  Shift turns a vertical-only wheel horizontal. Scrollbars remain vertical only.
  `cargo run -p rndr --example grid_scroll_gallery -- out.png` renders a gallery.
- **GPU effects:** an `EffectSlot { id, data, backdrop, animated }` entity marks a
  laid-out box that an app-registered `GpuEffect` (in the `GpuEffects` resource)
  draws with wgpu, at the slot's place in paint order. The effect gets the device,
  queue, frame encoder, slot pixel rect, parent clip, scale, target format, sample
  count, frame time and four floats of data. Inside a faded or transformed group it
  draws into the group's layer, so the group's opacity and transform apply. With
  `backdrop` it also gets a copy of everything drawn before it in its target; inside
  a group that is only the group's own layer. `animated` keeps frames coming while
  the slot is shown. Each slot closes and reopens the current render pass, and a
  backdrop copies the whole target, so many slots per frame cost passes and copies.
  Native backdrops need a surface that allows copies; otherwise `backdrop()` is `None`.
  `cargo run -p rndr --example effect_gallery -- out.png` renders a glow and a frosted panel.
- **Live values:** a `Live<T>` (`f32`, `[f32; 2]`, `[f32; 4]` or `Color`) is a
  cloneable latest-wins handle any thread can `set`. `LiveOpacity`, `LiveOffset`,
  `LiveScale`, `LiveFill` and `LiveRect` bind it to an entity's `Opacity`,
  `Transform` or fill, as is or through a motion `Spring`. A change wakes each
  visible window that draws the handle once until its next frame; equal values and
  hidden, minimized or fully covered windows do not wake. When a live value is the only reason for a frame the
  runner calls `rndr_paint::frame_live`, which skips Input, Update, PostUpdate and
  Layout, so text does not rewrap and siblings do not move. `LiveRect` moves and
  scales the laid-out box through `Transform`. In a headless CPU probe
  (`cargo test -p rndr_paint --release --test live_cost -- --ignored --nocapture`)
  a live-only frame cost about a sixth of a full frame.
- **Video frames:** a `FrameSurface` is a cloneable latest-wins frame slot any
  thread can `push` a `VideoFrame` into: RGBA, BGRA or NV12 with BT.601/BT.709
  matrix and video/full range, tightly packed. Malformed frames are refused with a
  `FrameError` and the old frame stays. A `SurfaceView` draws the latest frame in
  its entity's box, contained, covered or stretched, optionally mirrored and with
  a corner radius, in paint order and inside clips and faded or transformed groups.
  A push wakes visible windows that draw the surface the same way `Live` values do,
  so a new frame repaints without layout. The renderer uploads each new frame once
  and converts NV12 on the GPU; frames are copied from CPU memory, not imported
  zero-copy. `stats()` counts frames pushed and shown.
  `cargo run -p rndr_render --example frame_surface` animates frames from a thread.
- **Video and audio playback:** with the `media` feature, `rndr::media::Player`
  plays a file or URL through the platform player (GStreamer on Linux,
  AVFoundation on macOS; other platforms report `PlayerState::Failed`). Video
  decodes into the player's `FrameSurface`, so a `SurfaceView` shows it, with the
  first frame shown before playback; audio clips play without a view. Controls are
  play, pause, seek, rate, loop and mute; state, time, duration and video size are
  readable, `position()` is a `Live<f32>`, and `set_waker` reports state changes.
  Linux builds need the GStreamer development packages. See
  [crates/rndr_media/README.md](crates/rndr_media/README.md).
- **Desktop settings, links and cursors:** the runner publishes the desktop's
  `SystemAppearance` (light/dark, accent colour, reduced motion) and sends
  `SystemAppearanceChanged` when it changes; `UiTheme::follow_system` applies the
  light/dark preference. The OS reduced-motion setting makes the motion runtime
  snap layout and transform animations. `request_open_url` opens checked `http`,
  `https` and `mailto` links in the default handler (`xdg-open`, `open`,
  `ShellExecuteW`, or a new browser tab). `CursorStyle` covers the CSS cursor set,
  including `Hidden` and resize cursors, besides custom RGBA images. Linux is tested
  natively; macOS and Windows are compile-checked only.
  `cargo run -p rndr_render --example os_basics` shows them.
- **Text features and selectable labels:** `TextStyle` and `TextSpan` take OpenType
  `FontFeature`s (`FontFeature::on(b"tnum")`) and variable-font `FontAxis` values
  (`FontAxis::new(b"wght", 550.0)`), applied by the Parley shaper; a span's settings
  follow the style's. `TextSpan::strike` draws a strikethrough. A `Selectable` label
  with `SelectablePlugin` selects by drag, double-click (word) and triple-click
  (paragraph), extends with Shift, copies with Ctrl/Cmd+C, selects all with
  Ctrl/Cmd+A, publishes the primary selection and shows an I-beam. Selection is
  paint-only: it does not re-lay out or reshape text. Native clipboard behavior is
  covered by headless tests only.
- **Rich composer:** structured documents add `underline`, `strike`, `code` and
  mention `pill` marks. `toggle_mark` flips one mark across a selection while keeping
  each run's other marks; Ctrl/Cmd+B, I and U call it. `insert_pill` inserts an atomic
  pill: carets step over it, Backspace and Delete remove it whole, and typing next to it
  never extends it. `selection_html`, `to_html` and `paste_html` exchange a safe HTML
  subset (marks, links, pills, headings, lists, quotes) with plain text as the fallback;
  scripts, styles and unsafe link schemes are dropped. Copy writes `Clipboard::html`
  beside the text and paste prefers it. The native runner exchanges it with the OS
  clipboard; that path is covered by headless tests only.
  `cargo run -p rndr --example text_features_gallery -- out.png` renders a gallery.
- **Document editing:** source sessions and structured editing have explicit ownership
  and regression coverage. The structured editor remains experimental; native IME,
  clipboard and assistive-technology acceptance must be checked independently of
  headless tests. The [editing epic](docs/design/native-document-editing-epic.md)
  records the staged design, not a blanket claim that all proposed APIs shipped.
- **Desktop platforms:** macOS, Windows, X11 and Wayland have different capabilities.
  Wayland may ignore positioning, visibility, focus and window-level requests;
  global shortcuts depend on the portal grant, and Linux tray icons need a host.
  A successful build does not certify every native interaction.
- **Observation:** the Unix transport captures a live whole-window surface and a
  matching flattened accessibility tree, subject to explicit content policy and
  fail-closed privacy checks. It is not component-targeted capture, proof of OS
  compositor presentation, or native screen-reader/IME certification.
- **Pen input:** the pen probe is a hardware-validation spike, not a supported,
  cross-platform tablet API.
- **Publishing:** 0.4.0 publishes `rndr_core`, `rndr_paint`, `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_liquid`, `rndr_chart`, `rndr_liquid_ui`, `rndr_canvas`, `rndr_canvas_ui`, `rndr_render`, and `rndr`. The facade's optional `chart`, `liquid` and `canvas` features version-depend on the five Liquid, chart and canvas crates, so they are packaged before it. `rndr_browser` is now part of that package order, immediately before the facade, because the optional `browser` dependency is resolved even when the feature is off. Examples stay unpublished. This tree does not upload. See [package policy](docs/ui-status.md#package-policy).

## Documentation

| Topic | Guide |
| --- | --- |
| Semantic components, themes and composition | [UI README](crates/rndr_ui/README.md) |
| Offline app starter and local macOS packaging | [Desktop apps](docs/desktop-apps.md) · [Recovery evidence](docs/desktop-recovery.md) · [Native release gates](docs/desktop-readiness.md) |
| Data tables and inspectors | [Data components](docs/data-components.md) · [Data editors](docs/data-editors.md) |
| Selection and popup ownership | [Selection controls](docs/selection-controls.md) |
| Virtualization and stable row identity | [Virtual lists](docs/virtual-list.md) |
| Source-backed mutation and undo | [Source editing contract](docs/source-backed-editing.md) |
| Structured document architecture | [Editing epic](docs/design/native-document-editing-epic.md) · [Structured editor](docs/design/native-document-editing-e5.md) |
| Live Markdown content and controls | [Host-filled slots](docs/markdown-host-slots.md) |
| Unsaved state and native close/quit | [Safe close](docs/safe-close.md) |
| Capture, privacy and evidence limits | [Native observation](docs/native-observation.md) · [Component capture](docs/component-capture.md) |
| File drag position | [File drop](docs/file-drop.md) |
| Hands-on appearance checks | [UI polish](docs/ui-polish.md) |
| API reference for published versions | [docs.rs/rndr](https://docs.rs/rndr) (may differ from this checkout) |

## Development

Start with a focused headless test, then run the gates appropriate to your change:

```sh
cargo test -p rndr_core
cargo fmt --all --check
git diff --check
```

A headless frame test creates an `App`, supplies a `Viewport` and `Input`, calls
`rndr_paint::frame`, then asserts on state or paint. See the
[copyable interaction/snapshot test](crates/rndr_paint/tests/pattern.rs).
Time-dependent core tests can use `Sim` and a manual clock instead of sleeping.
Do not update snapshots merely to hide an unexplained regression.

[CONTRIBUTING.md](CONTRIBUTING.md) covers naming, mutation ownership, module
boundaries and verification commands. The [CI workflow](.github/workflows/ci.yml)
is the source of truth for the broader platform/feature matrix. The
[maintainability record](docs/design/codebase-maintainability-plan.md) explains
recent refactoring decisions and their evidence.

## Inspiration and acknowledgements

Bevy inspired rndr's entity/component/system approach and plugin-oriented composition.
rndr is an independent implementation with its own ECS, layout, UI, document
models and desktop lifecycle; it does not depend on Bevy.

The desktop stack builds on projects including winit, wgpu, Parley, Swash and
AccessKit. Bundled font licensing is recorded alongside the assets.

## License

MIT. See [LICENSE](LICENSE).
