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
│  FsPath   Files             │
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
| `rndr_core` | ECS, layout, clip, scroll, disclosure, focus, pane, split, caret, body, wrap, `Font`, `Files`, `Chrome`, events, tasks, timers |
| `rndr_paint` | `frame`: update, then quads in points. Reads a `TextLayout`. The bitmap face is an 8×8 ASCII atlas |
| `rndr_std` | `std::fs` plugin |
| `rndr_render` | winit window writes `Input` and `Viewport`, then presents quads. `WindowConfig` chooses title, size, and panel behavior |
| `examples/expand` | File-backed expand demo: folder tree, pane, splitter, caret, create, delete |
| `examples/idle` | Empty window. `ControlFlow::Wait` when nothing is scheduled |
| `examples/overlay` | Frameless centered panel. Hides on blur and Escape, returns on a timer |
| `examples/palette` | Command palette: debounced filter, virtual list, shortcuts |

A blank `App` holds the world, `Input`, and `Viewport`. `Shell` registers the file-tree systems. `Chrome` is optional and leaves `Shell` alone: named chords, buttons, tabs, a single-line field, overlays, a virtual list, an in-memory clipboard, styled runs, a gap-buffer editor, decorations, and IME preedit. `frame` also reports `SurfaceSlot` rects and leaves those interiors empty. `StdFsPlugin` inserts the disk backend. `MemoryFs` is the headless backend. `rndr_paint::frame` steps the world and returns quads in points. `rndr_render` writes the window into `Input` and `Viewport`, then multiplies those quads by `scale` and presents them. Platform dependencies stop at `wgpu`, `winit`, and `pollster`. No layout engine, no text crate, no third-party ECS.

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

`after` and `every` emit events. `sleep` parks a task. `debounce` resets a per-key timer so a burst of calls becomes one.

The desktop runner enables the `pool` feature (a small thread pool, not tokio) and wakes winit with an `EventLoopProxy` user event. When nothing is runnable the loop uses `ControlFlow::Wait`, or `WaitUntil` the next timer. Pressed keys still land in `Input.keys` for that frame. `cargo run -p idle` is the idle window; it should sit near 0% CPU. `--tick` arms a one-second timer and waits with `WaitUntil`.

Headless tests use `Sim`: an inline executor and a `ManualClock`. `advance` moves time, `run_until_parked` polls until every task is waiting, and `with_seed` shuffles ready background tasks. The same seed repeats the same order. Tests do not sleep.

## Window

Insert a `WindowConfig` before `run`. Omit it and the window stays what it was: title `rndr`, 800×600, decorated, resizable, placed by the platform. `show`, `hide`, `focus`, and `toggle` are commands. A `WindowHandle` is `Send`, so a system or a task can call them. The process keeps running while the window is hidden, the event loop stays on `Wait`, and the GPU surface is not created again when the window comes back.

`WindowFocus` is one event queue, so blur then focus stays in that order. Focus and blur from the platform or a task are merged before Input, so that stage can read them. `hide_on_blur` hides a panel after it has been focused once; the blur event is recorded and then the window is hidden. `Dismiss::Hide` and `Dismiss::Close` run at the start of Input, before the keymap and text fields, so Escape can hide or quit without a plugin. The blur from that hide is readable in Update, with the other events Input sent. The default is `Dismiss::Ignore`, which leaves the file tree's own Escape handling alone. A show or hide queued by an Update, PostUpdate, or Layout system is applied after Layout; its focus event waits until the next frame.

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

## Headless tests

CI on macOS, Windows, and Ubuntu runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, then `cargo build -p expand -p idle -p overlay -p palette -p rndr_render`. The Ubuntu job installs the X11, Wayland, and Vulkan packages winit and wgpu need to compile.

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

Text measurement goes through a [`TextEngine`](crates/rndr_core/src/text.rs) resource. `layout` shapes a run or a paragraph from a font family, size, weight, and style, and returns positioned glyphs, line metrics, and caret hit-testing. Shaped lines are cached by text, style, and width for the current frame and the one before it. The builtin engine reads a [`Font`](crates/rndr_core/src/font.rs): advances, kerning, and a coverage atlas, with a slot left for a color atlas. [`BitmapFont`](crates/rndr_core/src/font.rs) is the face when none is inserted: one cell per scalar, the same 8×8 atlas as before. The expand demo inserts [`LatinFace`](crates/rndr_paint/src/latin.rs), a proportional ASCII atlas rasterized from Inter Regular (real advances and kerning), and a [`Theme`](crates/rndr_core/src/theme.rs). A label can set its own text color; [`Spans`](crates/rndr_core/src/font.rs) paint ranges with a color, a weight, and an optional highlight. Theme text stays the default. A hugging label takes its height from that measurement, and `Layout.min_width` / `min_height` keep a fill from squashing it. Placed edges snap to physical pixels. After rects are final, the frame writes a `TextLayout` onto each label. Paint reads that run and snaps glyph edges to physical pixels. A `Radius` on a surface rounds that quad in the same shader. A `TextFace` resource swaps the face the builtin engine resolves, without touching the caret, the splitter, or the file tree. A later parley and swash engine implements `TextEngine` and replaces the resource.

## Demo

```bash
cargo test -p rndr_core
cargo run -p idle
cargo run -p overlay
cargo run -p palette
cargo run -p expand
cargo run -p expand -- /path/to/dir
```

`overlay` opens a frameless, always-on-top panel in the center of the screen. Escape or a click away hides it. Two seconds later the timer shows it again.

`palette` is a command list in that same kind of window: frameless, centered, always on top, hidden on blur and on Escape. The search field emits `TextChanged`, a debounced task filters thirty commands, and a virtual list shows the matches with shortcuts from `Keymap::shortcut` (`⌘K` on macOS, `Ctrl+K` elsewhere, `Ctrl+G M` for a two-stroke chord). The shortcut is a label with its own dim color. Letters the filter hit are painted with a highlight. Up and Down move the selection, Enter runs it, and the run is a task whose handle sits in ordinary app state. The field and the row labels measure through `TextEngine`.

A tree column on the left and a detail pane on the right. Drag the seam to resize. The body wraps to the pane width; drag the splitter and the next frame reflows. The demo opens the first argument if it is an existing directory, otherwise `examples/expand/fixture`. Click a row to select it. Right lists a directory; Enter opens a file into the pane. Click a title or body cell to place the caret; Shift extends the selection. Type in the title to rename, Tab into the body to edit, Ctrl+S writes the file. A mark on the tree row means the file is dirty. Ctrl+N creates `untitled.txt` (Ctrl+Shift+N a folder) and starts a title edit. Delete removes a file or an empty folder. Collapse writes dirty children first. Hidden names and binary files are skipped for the body. Escape leaves the caret, then the pane.
