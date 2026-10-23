# rndr

A Bevy-shaped ECS for desktop applications. Core owns the world. wgpu draws a flat list.

```
Application
    │
    ▼
┌─────────────────────────────┐
│         rndr_core           │
│  Entities  Components       │
│  Hierarchy Layout Expand    │
│  Clip Scroll Focus Pane     │
│  Split Caret Body Wrap      │
│  FsPath   Files  Feed       │
└──────────────┬──────────────┘
               │
     ┌─────────┼──────────┐
     │         │          │
 Shell    Files plugin   Paint
     │         │          │
     │    std::fs      quads
     │   (rndr_std)  (rndr_paint)
     │
     ▼
 winit + wgpu (rndr_render)
```

`mob3` is the prototype of this cut: application structure in the core, the renderer as a plugin that never owns parenting. Expand is a hierarchy walk. Opening a node spawns children; collapse cascade-despawns them. The next layout pass rewrites rectangles. The GPU never hears about expand.

## Crates

| Crate | Role |
| --- | --- |
| `rndr` | Facade. Re-exports the libraries below so `cargo add rndr` is the desktop entry point. Features match `rndr_render` |
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus, pane, split, caret, body, wrap, `Font`, `Files`, `Chrome`, events, tasks, timers, markdown |
| `rndr_paint` | `frame`: update, then quads in points. Reads a `TextLayout`. The bitmap face is an 8×8 ASCII atlas |
| `rndr_std` | `std::fs` plugin |
| `rndr_term` | Embedded terminal. A PTY and a VT state machine copy cells into a `MonoGrid`. Optional `rndr` feature `term` re-exports it. Core and paint do not link the PTY |
| `rndr_render` | winit window writes `Input` and `Viewport`, then presents quads. `WindowConfig` chooses title, size, and panel behavior. The default `hotkey` and `tray` features add system-wide chords and a menu-bar icon |
| `examples/expand` | File-backed expand demo: folder tree, pane, splitter, caret, create, delete |
| `examples/idle` | Empty window. `ControlFlow::Wait` when nothing is scheduled |
| `examples/overlay` | Frameless centered panel. Hides on blur and Escape, returns on Alt+Space |
| `examples/palette` | Command palette: debounced filter, virtual list, shortcuts |
| `examples/stream` | Streamed markdown answer: a background feed appends tokens into a pinned text view |
| `examples/terminal` | Shell in a window. `cargo run -p terminal` |

A blank `App` holds the world, `Input`, and `Viewport`. `Shell` registers the file-tree systems. `Chrome` is optional and leaves `Shell` alone: named chords, buttons, tabs, a single-line field, a multi-line prompt, overlays, a virtual list, an in-memory clipboard, styled runs, a gap-buffer editor, decorations, and IME preedit. `frame` also reports `SurfaceSlot` rects and leaves those interiors empty. `StdFsPlugin` inserts the disk backend. `MemoryFs` is the headless backend. `rndr_paint::frame` steps the world and returns quads in points. `rndr_render` writes the window into `Input` and `Viewport`, then multiplies those quads by `scale` and presents them. Headless crates stop at the standard library. The desktop crate depends on `wgpu`, `winit`, and `pollster`, and, when the default features are on, on `parley` for text, `tray-icon`, and the hotkey backends. `rndr_core` stays headless either way. No layout engine and no third-party ECS.

Layout, hit-testing, and scroll are in logical points. `Viewport.scale` is physical pixels per point. The desktop plugin divides window size, the cursor, and pixel-wheel deltas by that scale, then multiplies rects by it when uploading instances. `Viewport.insets` shrinks root placement for a status bar or home indicator. `Input.pointer` is one contact: the mouse writes id `0`. A drag inside a scrollport follows that contact after 4 points. A drag that starts on the seam still resizes the split.

## Frame

Stages run in this order. Input finishes, and the events it sent are merged, before Update reads them.

1. The platform writes `Viewport` (size, scale, insets) and `Input`.
2. `rndr_paint::frame` runs Startup once, then Input, Update, PostUpdate, and Layout.
3. Paint walks the rects and returns quads in points.
4. Render systems run. The surface multiplies by `scale` and presents.

Input starts with window dismiss, then the keymap, list navigation, and the text field. A field emits `TextChanged` and `Submitted` there, so an Update system sees the new query on that same frame. PostUpdate rebuilds the virtual list after Update has changed its count. Layout then places rows at the list width, which is why a resize stretches them. `ScaleChanged` and `Resized` are already readable in that Layout pass.

`Color` values are sRGB with straight alpha. The upload decodes red, green, and blue when the surface format is sRGB, and writes them unchanged when it is not, so a theme color lands on screen as authored on either format. Alpha stays coverage. An sRGB surface blends in linear light.

Headless tests call `frame` and never open a window. The desktop runner is the only thing that calls winit.

## Time, events, and tasks

`App::update` ticks a `Time` resource from a [`clock::Clock`](crates/rndr_core/src/clock.rs), flips double-buffered `Events<T>`, pumps tasks and timers, runs Input, merges those events, then runs Update. `spawn` runs a `Send` future on the `Executor` the runner installed. `spawn_local` runs on the UI thread, so the future may hold non-`Send` state across `.await`. The `TaskHandle` from either call is `Send + Sync` and lives in a normal resource; dropping it cancels the task, and `detach` lets it finish. State that cannot be `Send` goes in `World::insert_local` instead. Completions are written back on the UI thread as events.

A [`feed`](crates/rndr_core/src/feed.rs) is the path for many values, not one completion. `feed::open` returns a handle and a sender. The sender enqueues from a thread or from any future. The pump delivers the queued values as events on the UI thread and wakes the window once per burst, so a redraw is requested only when a new value is actually waiting. The queue has a fixed capacity: the sender can block, drop the oldest value, or refuse. `FeedConfig::latest` keeps the newest value. `Feed::advance` publishes an epoch; a send stamped with an older epoch is discarded and does not wake, which is how a slow search loses to a newer query. Dropping the handle closes the sender and unblocks a waiting send. `spawn_detached` runs a future that reports through the sender. The optional `tokio` feature spawns that future on a runtime the application already started. The default build does not depend on tokio.

`after` and `every` emit events. `sleep` parks a task. `debounce` resets a per-key timer so a burst of calls becomes one.

The desktop runner enables the `pool` feature (a small thread pool, not tokio) and wakes winit with an `EventLoopProxy` user event. When nothing is runnable the loop uses `ControlFlow::Wait`, or `WaitUntil` the next timer. Pressed keys still land in `Input.keys` for that frame. `cargo run -p idle` is the idle window; it should sit near 0% CPU. `--tick` arms a one-second timer and waits with `WaitUntil`.

Headless tests use `Sim`: an inline executor and a `ManualClock`. `advance` moves time, `run_until_parked` polls until every task is waiting, and `with_seed` shuffles ready background tasks. The same seed repeats the same order. Tests do not sleep.

## Window

Insert a `WindowConfig` before `run`. Omit it and the window stays what it was: title `rndr`, 800×600, decorated, resizable, placed by the platform. `show`, `hide`, `focus`, and `toggle` are commands. A `WindowHandle` is `Send`, so a system or a task can call them. The process keeps running while the window is hidden, the event loop stays on `Wait`, and the GPU surface is not created again when the window comes back.

`WindowFocus` is one event queue, so blur then focus stays in that order. Focus and blur from the platform or a task are merged before Input, so that stage can read them. `hide_on_blur` records the blur immediately and hides only after that blur settles. A focus that returns in the same batch or within the settle keeps the window visible, and a blur during the short grace after a programmatic show waits until the grace ends. The hide does not emit a second blur. `Dismiss::Hide` and `Dismiss::Close` run at the start of Input, before the keymap and text fields, so Escape can hide or quit without a plugin. The blur from that hide is readable in Update, with the other events Input sent. The default is `Dismiss::Ignore`, which leaves the file tree's own Escape handling alone. A show or hide queued by an Update, PostUpdate, or Layout system is applied after Layout; its focus event waits until the next frame.

`publish_metrics` writes `Viewport` (size and scale, insets untouched) and queues `ScaleChanged` then `Resized` when those values change. The flip at the start of the frame makes them readable in Input, Update, and Layout. A zero physical size keeps the last logical size.

| | macOS | Windows | X11 | Wayland |
| --- | --- | --- | --- | --- |
| Title, size, min size, resizable, decorations | yes | yes | yes | yes |
| Position and center | yes | yes | yes | ignored |
| Transparent framebuffer | requested; opaque if the surface cannot composite | same | same | same |
| Always on top | floating level | yes | yes | ignored |
| Skip taskbar | no taskbar; `accessory` leaves the Dock | yes | utility window type | ignored |
| Panel over full-screen, every Space, non-activating | yes | no | no | no |
| Accessory activation policy | yes | ignored | ignored | ignored |
| Hide, show, focus without a new GPU surface | yes | yes | yes | surface stays; hide and focus are ignored |

A macOS panel is an `NSWindow` (winit 0.30 does not build an `NSPanel`) with the non-activating style bit, pop-up window level, and collection behavior for every Space including full-screen. Focusing it does not call `activateIgnoringOtherApps`. `accessory` sets the accessory activation policy and skips launch-time activation. On Wayland, winit 0.30 ignores position, `set_visible`, `focus_window`, and window levels; the logical phase still updates.

## Tray

Insert a `TrayConfig` before `run`, or from a Startup system. The icon is RGBA pixels or PNG bytes, plus a `template` flag. On macOS a template image follows the menu bar's light or dark tint. The menu is labels, optional shortcut display text from `Keymap::shortcut` or `Keystroke::display`, enabled and checked state, separators, and submenus. `set_label`, `set_enabled`, `set_checked`, and `set_icon` update a live icon. Shortcut text is shown beside the label. macOS and Windows use the accelerator column when the chord parses. Linux draws one string and has no accelerator column, so the chord is part of the label there too. It is not a hotkey: a keymap chord can be more than one key.

Clicks arrive as `TrayEvent::Click` and `TrayEvent::DoubleClick`. Choosing a row arrives as `TrayMenu::Activated(id)`. Both are readable in Update, and the callback wakes the loop while the window is hidden. `toggle_on_click` is the one-line helper that toggles the window on a left click. Right click opens the menu and is not a `TrayEvent`. An item built with `Item::quits` sets `Input::quit`, and the runner exits even if the window is hidden.

`hide_window` starts the app with no visible window. Put the config in the world before `run` so the window is created hidden. The process stays on `ControlFlow::Wait` until a click, a timer, or quit.

The desktop crate uses [`tray-icon`](https://docs.rs/tray-icon) 0.26 (MIT OR Apache-2.0) behind the `tray` feature, which is on by default. Linux uses that crate's `ksni` backend: a StatusNotifierItem on the session bus, on a worker thread, with no GTK loop on the winit thread. The default AppIndicator backend is off because it needs GTK on this thread and would either spin or miss clicks during `Wait`. `rndr_core`, `rndr_paint`, and `rndr_std` do not depend on it. Building `rndr_render` with `--no-default-features` leaves the tray out.

At runtime on Linux the icon appears when a StatusNotifierItem watcher is on the session bus (Ubuntu's Ayatana/AppIndicator host, or KDE). No extra `-dev` packages are required to compile. If no watcher is running, the process still runs and the icon is not shown.

| | macOS | Windows | X11 | Wayland |
| --- | --- | --- | --- | --- |
| Icon | menu bar status item | notification area | StatusNotifierItem | StatusNotifierItem |
| Template image | yes | ignored | ignored | ignored |
| Tooltip, menu, separators, submenus | yes | yes | yes, when a watcher is running | same |
| Left click | yes | yes | yes | yes |
| Double click | not emitted | yes | not emitted | not emitted |
| Hidden window keeps running | yes | yes | yes | yes |

## Global hotkeys

`register` and `toggle_window` grab a chord that fires while another application is focused. The binding uses the same words as a keymap chord: `primary+Space`, `Alt+Space`, `Ctrl+Shift+K`. `primary` is Command on macOS and Ctrl elsewhere. Each call returns a handle. Dropping the last handle, or calling `unregister`, releases the grab. Pressing the chord wakes the event loop and delivers a `GlobalHotkey` on that frame, including while the window is hidden. The loop stays on `Wait` otherwise. `toggle_window` is the launcher case: a hidden window is shown and focused, a visible window is hidden.

```rust
let _hotkey = rndr_render::toggle_window(app.world_mut(), "Alt+Space")?;
rndr_render::run(app);
```

Registering a chord this process already holds returns another handle for the same id and does not grab twice. `primary+Space` and `Ctrl+Space` are the same grab on Windows and different grabs on macOS. A second application that already owns the chord gets `AlreadyTaken`. `note_unregistered` logs that failure and keeps the process running; the same value is a `HotkeyRejected` resource and event so the app can show it. A binding this platform cannot encode gets `InvalidBinding`. No backend, or a session that cannot bind hotkeys, gets `Unsupported`.

The parser, the registry, and the per-platform key codes live in `rndr_core` and have no OS dependencies. `rndr_render`'s `hotkey` feature (on by default) installs the backend. Turn it off and headless builds do not compile Carbon, Win32, X11, or a D-Bus client.

The backends are small and written against this event loop, rather than the `global-hotkey` crate. That crate's Linux path is X11 only and polls every 50ms, it has no Wayland portal, and its chord type is not rndr's `Keystroke`. Carbon and Win32 are thin FFI. X11 uses `x11rb` (MIT or Apache-2.0, no libX11): a grab that loses comes back as `BadAccess` instead of a process-wide X error handler, and the worker blocks in `poll`. Wayland uses the XDG GlobalShortcuts portal through `zbus`.

| | macOS | Windows | X11 | Wayland |
| --- | --- | --- | --- | --- |
| Mechanism | Carbon `RegisterEventHotKey` | `RegisterHotKey` | `XGrabKey` on the root window | XDG GlobalShortcuts portal |
| Works while another app is focused | yes | yes | yes | yes, after the portal grants it |
| Permission | none (no Accessibility prompt) | none | none | a portal dialog the user can edit |
| Chord already owned | `AlreadyTaken` | `AlreadyTaken` | `AlreadyTaken` (`BadAccess`) | portal declines the grant (`AlreadyTaken`) |
| Unavailable session | — | — | no X server (`Unsupported`) | no portal (`Unsupported`); X11 is not used as a fallback |
| Lock keys | ignored by Carbon | ignored by `RegisterHotKey` | grabbed again with Caps Lock, Num Lock, and Mod5 | portal / compositor |
| Key repeat | one event per press | `MOD_NOREPEAT` | auto-repeat releases are skipped | one `Activated` per press |
| Idle | the Cocoa run loop is already waiting | the hotkey thread blocks in `GetMessage` | the hotkey thread blocks in `poll` | the portal thread blocks on D-Bus |

F21–F24 have no Carbon virtual key, so they are `InvalidBinding` on macOS and valid elsewhere. On Wayland, `BindShortcuts` may show a system dialog and the user may change the trigger; the id still fires when that trigger is pressed. Each registration is its own portal session, because a session can only be bound once. A Wayland session (`WAYLAND_DISPLAY`) never falls back to an X11 grab. Menu shortcut text stays display-only; a tray row can show this chord beside its label, as the palette's Show Palette item does.

## Headless tests

CI on macOS, Windows, and Ubuntu runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo clippy -p rndr_render -p rndr --no-default-features --all-targets -- -D warnings`, `cargo test --workspace`, `cargo test -p rndr_core --features tokio`, `cargo test -p rndr_core --features highlight`, then `cargo build -p expand -p idle -p overlay -p palette -p terminal -p stream -p rndr_render`. The Ubuntu job also runs `cargo package --workspace` (a verified build of each packaged crate) and checks that the Inter license and the snapshot files are in those crates, then builds `cargo doc --no-deps` with warnings denied. It installs the X11, Wayland, and Vulkan packages winit and wgpu need to compile. The tray and hotkey features add no further packages.

A headless test drives one frame without a window:

1. Build an `App`, insert a `Viewport`, and spawn the scene.
2. Write one frame of `Input` (pointer edge, key, or chars).
3. Call `rndr_paint::frame`.
4. Assert on the world, or snapshot [`quads_snapshot`](crates/rndr_paint/src/snapshot.rs). That dump rounds rects, clips, and radii to the nearest 0.5 point so float noise does not churn the file. Colors print to three decimals and UVs to four. Quad order is paint order.

[`crates/rndr_paint/tests/pattern.rs`](crates/rndr_paint/tests/pattern.rs) is the copyable version: click a button, assert `Fired`, snapshot the quads. One-frame edges are cleared after `frame`, the same way the winit runner drops them after present.

After an intentional paint change, rewrite the snapshots and commit the `.snap` diff:

```bash
INSTA_UPDATE=1 cargo test -p rndr_paint
```

Text measurement goes through a [`TextEngine`](crates/rndr_core/src/text.rs) resource. `layout` shapes a run or a paragraph from a font family, size, weight, and style, and returns positioned glyphs, line metrics, and caret hit-testing. Shaped lines are cached by text, style, and width for the current frame and the one before it. The builtin engine reads a [`Font`](crates/rndr_core/src/font.rs): advances, kerning, and a coverage atlas, with a slot left for a color atlas. [`BitmapFont`](crates/rndr_core/src/font.rs) is the face when none is inserted: one cell per scalar, the same 8×8 atlas as before. The expand demo inserts [`LatinFace`](crates/rndr_paint/src/latin.rs), a proportional ASCII atlas rasterized from Inter Regular (real advances and kerning), and a [`Theme`](crates/rndr_core/src/theme.rs). A label can set its own text color; [`Spans`](crates/rndr_core/src/font.rs) paint ranges with a color, a weight, and an optional highlight. Theme text stays the default. A hugging label takes its height from that measurement, and `Layout.min_width` / `min_height` keep a fill from squashing it. Placed edges snap to physical pixels. After rects are final, the frame writes a `TextLayout` onto each label. Paint reads that run and snaps glyph edges to physical pixels. A `Radius` on a surface rounds that quad in the same shader. A `TextFace` resource swaps the face the builtin engine resolves, without touching the caret, the splitter, or the file tree. The desktop runner installs a parley and swash engine by default (`rndr_render`'s `parley` feature): it shapes with bidi and line breaking, rasters coverage glyphs and color emoji into atlases that grow and evict a glyph only when a new one does not fit, and re-rasterizes when the scale changes. Headless tests and the existing snapshots keep the builtin atlas, so they do not depend on system fonts.

## Demo

```bash
cargo test -p rndr_core
cargo run -p idle
cargo run -p overlay
cargo run -p palette
cargo run -p stream
cargo run -p expand
cargo run -p expand -- /path/to/dir
```

`overlay` opens a frameless, always-on-top panel in the center of the screen. Escape or a click away hides it. Alt+Space shows it again, from any application, and hides it if it is already visible. `RNDR_HOTKEY` or `--hotkey` replaces Alt+Space (`cargo run -p overlay -- --hotkey Ctrl+Space`).

`palette` is a command list in that same kind of window: frameless, centered, always on top, hidden on blur and on Escape, and summoned by the same hotkey. On macOS it runs as an accessory app, so the Dock stays out of it. A tray icon remains: Show Palette (the global hotkey, `⌥Space` on macOS and `Alt+Space` elsewhere, unless `--hotkey` or `RNDR_HOTKEY` changed it), Settings (disabled), and Quit. Left click toggles the window. The search field emits `TextChanged`. Each change starts a background feed that waits, then filters thirty commands. A reply stamped with an older epoch is dropped, so a slow result cannot replace a newer query. A virtual list shows the matches with shortcuts from `Keymap::shortcut` (`⌘K` on macOS, `Ctrl+K` elsewhere, `Ctrl+G M` for a two-stroke chord). The shortcut is a label with its own dim color. Letters the filter hit are painted with a highlight. Up and Down move the selection, Enter runs it, and the run is a task whose handle sits in ordinary app state. The field and the row labels measure through `TextEngine`. Activating Ask Medousa switches that field to a multi-line prompt: Enter sends, Shift+Enter inserts a newline, and Ctrl+Enter or Cmd+Enter returns to the list. Esc returns when the prompt is empty, and the unsent draft is still there the next time Ask opens. Filter replies stay ignored while the prompt is open.

`stream` opens a window. A background feed sends a canned markdown answer a few characters at a time. The view appends each chunk and stays pinned to the bottom until the pointer scrolls up. A link click writes the destination into the status line.

A tree column on the left and a detail pane on the right. Drag the seam to resize. The body wraps to the pane width; drag the splitter and the next frame reflows. The demo opens the first argument if it is an existing directory, otherwise `examples/expand/fixture`. Click a row to select it. Right lists a directory; Enter opens a file into the pane. Click a title or body cell to place the caret; Shift extends the selection. Type in the title to rename, Tab into the body to edit, Ctrl+S writes the file. A mark on the tree row means the file is dirty. Ctrl+N creates `untitled.txt` (Ctrl+Shift+N a folder) and starts a title edit. Delete removes a file or an empty folder. Collapse writes dirty children first. Hidden names and binary files are skipped for the body. Escape leaves the caret, then the pane.
