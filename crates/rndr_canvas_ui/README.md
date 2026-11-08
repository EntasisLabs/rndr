# rndr_canvas_ui

Paint and edit [`rndr_canvas`](../rndr_canvas) documents with `rndr_paint`,
`rndr_core` input and `rndr_ui` controls.

| Module | What it does |
| --- | --- |
| `paint` | `paint_scene` draws a scene into an `rndr_paint::Canvas`: frames with rounded clipping, shadows, shapes, dashed lines with arrowheads, pressure-shaped ink, styled text and cropped images. `visual_bounds` gives the painted extent. |
| `editor` | `CanvasEditor`, a headless editor over a `CanvasStore`. It takes one `EditorInput` per frame and needs no window, so it can be driven from tests. |
| `overlay` | `paint_overlay` draws the selection box and handles, hover outline, marquee, lasso, eraser trail and snap guides. |
| `view` | Runs a `CanvasView` in an `rndr_core` world: reads mouse, touch, pen, wheel and keys over a `SurfaceSlot` and paints the scene and overlay into it. |
| `panels` | Toolbar, arrange buttons, stroke/fill swatches, widths, text sizes and a layers list (select, hide, lock), built from `rndr_ui` buttons. |

Text is measured with the world's `TextEngine` (`EngineMeasure`), so
picking text lines up with what is painted. Without an engine the
estimate from `rndr_canvas` is used.

## Editing model

While a gesture is in progress the editor keeps a draft copy of the nodes,
which is what gets painted. Releasing commits one `Transaction` to the
store, so each gesture is one undo step. A conflict reloads the document and
is reported through `CanvasEditor::last_error`. Camera, selection, tool and
style live in the editor, never in the document.

Tools: select (`V`), hand (`H`), pen (`P`), eraser (`E`), rectangle (`R`),
ellipse (`O`), line (`L`), arrow (`A`), text (`T`) and frame (`F`).

- Select: click, shift-click, marquee, alt-drag lasso, double-click into a
  group or to edit text. Drag to move with grid and smart-guide snapping;
  drag handles to resize, rotate (shift snaps to 15°) or move line ends.
- Shapes, lines and frames: drag to place; shift keeps squares, circles and
  45° lines. A new node is placed inside the frame under the pointer.
- Pen: records pressure, tilt and time from the pointer's samples. A pen's
  eraser end, or the eraser tool, removes whole strokes or cuts ink at the
  points it touches.
- Pan with the hand tool, the middle button or the wheel; zoom with the
  primary modifier (Ctrl, or Cmd on macOS) and the wheel.
- Keys: undo `Mod+Z`, redo `Mod+Shift+Z`/`Mod+Y`, duplicate `Mod+D`,
  group `Mod+G`, ungroup `Mod+Shift+G`, select all `Mod+A`, delete
  `Delete`/`Backspace`, layer order `]`/`[`, nudge with arrows (shift for
  10), `Escape` cancels a gesture, then clears the selection, then leaves
  the group.

## Using it

```rust,ignore
let editor = CanvasEditor::open(store, "board")?;
view::install(world, CanvasView::new(editor, SLOT));      // world has a SurfaceSlot { id: SLOT }
panels::mount(world, toolbar, side, [toolbar_width, side_width]);
app.add_system(Schedule::Update, |world: &mut World| {
    for request in take_action_intents(world) {
        panels::dispatch(world, &request);
    }
    view::update(world);
    panels::refresh(world);
});
```

`examples/canvas-editor` is a complete app; `--export out.png` writes a frame
without opening a window.

## Limits

- Text is typed and erased at the end only: no caret placement, selection
  or IME composition inside the canvas.
- The inspector applies its swatches and sizes to the selection and to new
  nodes; it does not read the selected node's current style, and offers
  fixed swatches rather than a color picker.
- Keys reach the editor whenever the view updates; an app with other text
  inputs should only call `view::update` while the canvas has focus.
- Image assets are drawn from texture ids the app supplies in
  `CanvasView::images`; the editor does not import images yet.
