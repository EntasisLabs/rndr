# rndr_canvas

Headless canvas documents for whiteboards and design tools: ink, text,
shapes, lines, images, groups and frames, edited through transactions with
undo/redo, picked and arranged with scene geometry, and saved through a
`CanvasStore`.

This crate has no rendering or UI dependency. Painting and the interactive
editor are separate layers built on top of it.

## Documents

A `Document` has an id, revision, title, pin flag, timestamps, a `Scene` and
the bitmap `Asset`s it references. Scene nodes are listed back to front;
`parent_id` nests nodes under groups and frames. Coordinates are scene
units, and each node's `Transform` is relative to its parent, rotated in
degrees about the center of its box. Camera, selection and other editor
state never go into a document.

Scenes use the same JSON as Medousa Create version 3. `Scene::default()`
writes the `rndr-canvas` v1 header and `Scene::medousa()` the
`medousa-create` v3 one; both are read and validated the same way and keep
their header when saved. A frame's `design_source` is kept as opaque
`Node::origin` data.

## Editing

Every change is a `Transaction`: an id, the revision it was made against,
an undo label and a list of `Operation`s (`put_node`, `remove_node`,
`reorder`, `set_background`, `rename`, `pin`). `CanvasRecord` is the single
place a document changes:

- A stale `expected_revision` fails with `CanvasError::Conflict(current)`.
- The batch applies to a copy and the result is validated before it is
  kept, so a failing batch changes nothing.
- Removing a node removes its descendants.
- A retried transaction id with the same input returns the original result
  with `duplicate: true`; the same id with different input is refused.
- Each transaction pushes an undo step (up to 100 steps or 4 MiB) and
  clears redo. `HistoryRequest` undoes or redoes one step as a new revision.

Validation limits match Medousa Create: 10,000 nodes, 250,000 ink points,
an 8 MiB scene, 256 assets of up to 25 MiB each, finite bounded numbers,
hex or `transparent` paint, acyclic hierarchies under groups and frames,
and text spans on UTF-16 boundaries.

## Storage

`CanvasStore` creates, gets, lists (cursor pages sorted by id), edits,
soft-deletes documents and imports bitmap assets (PNG, JPEG, GIF, WebP,
named `a_<sha256>`).

- `MemoryStore` keeps everything in memory.
- `FsStore` writes `documents/<id>.json` and `assets/<id>/<asset_id>` under
  a root directory. Writes go through a synced temporary file and a
  rename, and each request holds an exclusive OS file lock on the
  document, so processes sharing the directory serialize their edits.

## Geometry

`SceneIndex` wraps a node list for queries and edits:

- transforms and bounds: `world_matrix`, `bounds`, `object_bounds`,
  `scene_bounds`, `to_parent`;
- selection: `editable`, `selection_roots`, `selection_frame`,
  `selection_target`, `marquee`, `lasso`, `hit`, `hit_node`;
- edits returned as operations or nodes: `translate`, `group`, `ungroup`,
  `scale`, `rotate`, `resize_from_handle`, `move_line_endpoint`, `align`,
  `align_to_frame`, `distribute`, `arrange`, `erase_nodes`, `erase_ink`;
- `layer_rows` for a layers panel and `snap_targets` with
  `Snapping::snap_move` for grid and smart-guide snapping.

Text boxes are laid out with a `TextMeasure`. `EstimateMeasure` assumes
0.6 em per character, which is enough for tests and headless tools; a
renderer should pass its own shaper so picking matches what it draws.
`Camera` maps between view and scene coordinates for fit and zoom.

```rust
use rndr_canvas::*;

let store = MemoryStore::new();
let doc = store.create(CreateRequest {
    id: "board".into(),
    title: "Board".into(),
    scene: Scene::default(),
})?;
let note = Node::new(
    "note",
    Content::Shape {
        shape: ShapeKind::Rectangle,
        fill: "#ffd166".into(),
        stroke: "transparent".into(),
        stroke_width: 0.0,
        line: None,
    },
    Transform { x: 10.0, y: 10.0, width: 120.0, height: 80.0, rotation: 0.0 },
);
let done = store.transact("board", Transaction {
    id: "t1".into(),
    expected_revision: doc.revision,
    label: "Add note".into(),
    operations: vec![Operation::put(note)],
})?;
let index = SceneIndex::new(&done.document.scene.nodes);
assert!(index.hit(Vec2::new(50.0, 50.0), 0.0).is_some());
```

## Differences from Medousa Create

- Medousa's AI proposals, jobs, visual context and code handoff are not
  included; the store keeps only documents, history, receipts and assets.
- Store records are this crate's own format. Scene and document JSON is
  shared with Medousa; the record wrapper around them is not.
- Medousa's legacy five-point "arrow" ink is drawn as ink, not as a line.
- Partial ink erasing removes the points the eraser touches and splits
  the stroke there; it does not cut segments between points.
- Lasso selection takes ink when at least half its points are inside the
  loop, and other nodes when their bounds are entirely inside.

## Tests

`tests/fixtures` holds two scenes copied from Medousa's
`tests/fixtures/creative`, used to check lossless round-trips.

```sh
cargo test -p rndr_canvas
```
