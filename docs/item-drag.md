# Item drag

In-app drag moves a host payload between widgets in the same window.
[File drop](file-drop.md) is a different transfer: the operating system
delivers paths, and this crate does not turn those paths into rows.

There is one drag at a time. The session lives in `rndr_ui` and is installed
with `UiPlugin`. Core still owns the pointer, hit testing, and scroll capture.
A press on a drag source does not capture the scrollport. The source autoscrolls
that port itself when the pointer sits near an edge.

This is not a tab strip and not a kanban. Edge zones are reported so a later
tab strip can map them to drop-to-split. Nothing in this change splits a pane.

## Payload

A `DragSource` copies a `DragPayload` when the gesture starts. Later edits to
the source do not change the drag already in flight.

| Field | Meaning |
| --- | --- |
| `kind` | `DragKind(u64)`. Targets accept or reject by this value. |
| `value` | Host identity. List and tree reorder store `ItemId.0`. |
| `owner` | Optional scope. A target with an owner accepts only that owner. |

`LIST_ROW` (`DragKind(1)`) and `TREE_NODE` (`DragKind(2)`) are the collection
kinds. A host payload should use another kind, or the same kind with its own
owner. A list does not accept a tree node, and a row from one view does not
drop on a row from another.

## Threshold and preview

`DRAG_THRESHOLD` is the core scroll slop (4 points). A shorter move stays a
click: the row can still select on release. Crossing the threshold starts the
drag, cancels the pending click, and shows a preview.

The preview is a small label that follows the pointer, or the highlight during
keyboard pickup. It is marked `IgnorePointer`, so the drop target is whatever
is under the pointer, not the ghost. Paint draws it above the rows. Headless
tests check that the entity exists and that the drop still hits the row.

## Drop targets

A `DropTarget` accepts a payload when it is enabled, its kind list contains
the payload kind, and its owner matches when one is set. An optional gate can
still reject a place. A rejected hover sets `DropCue.accepted` to false and
the cursor to `NoDrop`. Releasing there cancels. It does not drop.

`DropCue` on the current target is the hover record: place, accepted, and
payload. An accepted hover also draws a bar, or a translucent fill for inside.
The cue is cleared when the drag ends.

## Places

`classify` maps a point in the target rectangle to a `DropPlace`.

| Place | Meaning |
| --- | --- |
| `Before` | Insert before the target along the geometry axis. |
| `After` | Insert after the target along that axis. |
| `Inside` | Drop into the target. |
| `Edge` | An outer band. `axis` is the axis that was measured. `end` is leading (top or left) or trailing (bottom or right). |

`DropGeometry` divides the rectangle:

- `axis` is the before/after direction.
- `edge` is the fraction of that axis, at each end, reported as `Edge`. Zero turns those bands off.
- `cross_edge` is the same fraction on the other axis. It is tested first. A later tab strip can treat these bands as drop-to-split.
- `inside` is the centered fraction of what remains, reported as `Inside`. Zero splits that remainder in half.

Presets: `reorder` is before/after only (lists). `nest` adds a centered inside band (trees). `split` sets `cross_edge` to 0.25 and leaves the interior as before/after. Fractions that are not finite are treated as zero. End bands are capped so a center remains.

A miss outside the rectangle is not a place. Releasing there cancels.

## Cancel

Escape cancels an armed press, a pointer drag, or a keyboard pickup. While a
gesture is active the session sets `DismissHold`, so that Escape does not also
close a window whose dismiss action is Close. The hold is read on the next
frame, which is the frame a following Escape arrives.

Releasing outside a target cancels. Releasing on a target that rejects the
payload cancels. Both send `DragNoticeKind::Cancelled`. A rejected release
announces `Cannot drop {name} here.` An ordinary cancel announces
`Cancelled drag of {name}.`

## Autoscroll

During a pointer drag, a point within `AUTOSCROLL_MARGIN` (28 points) of a
scrollport edge moves that port by `AUTOSCROLL_STEP` (16 points) per frame,
clamped to the content. The search starts at the hit and walks ancestors, so a
nested port that cannot move yields to its parent. Virtual lists use the same
`Scroll::offset`. Uniform lists read that offset as the window. Variable lists
treat a paint offset that differs from the last projection as input.

Focus reveal is paused for the drag, so the focused row does not pull the
viewport back. It resumes when the drag ends. A wheel still scrolls. A press
that is not on a drag source still uses core scroll-drag, which is why reorder
is off unless the view asks for it.

## Keyboard

Two paths.

`DragKeyboard::Pickup` is the default on a source the host builds itself.
Focus the source and press Alt+Enter. Arrows move the highlight through
accepting targets that currently have rectangles. Enter drops. Escape cancels.
The preview and `DropCue` follow the highlight. A text field with
`CapturesKeys` does not pick up.

List and tree rows use `DragKeyboard::Host`. Alt+Enter does not pick them up.
With `ViewProps::reorder`, Alt+Up moves the focused row before the previous
visible enabled row, and Alt+Down moves it after the next. On a tree, Alt+Right
moves it inside the previous visible row, and Alt+Left places it after its
parent. A move that would not change the forest, or that would drop a node
into its own subtree, announces `Cannot move {name}.` and does not emit a
reorder. The same chords on a virtual list use the logical item order,
including rows that are not realized.

Announcements go to one polite live region (`AccessRole::Status` and
`LiveRegion(Polite)`). The region is not painted. Its name is the announcement.
Headless tests read that name and the live-region component. They do not drive
a platform screen reader. AccessKit maps `LiveRegion` in the desktop adapter
the same way it maps a toast. That is not an AT certificate.

## List and tree reorder

`ViewProps::reorder` defaults to false. `with_reorder` turns it on. Existing
struct update from `ViewProps::new` stays source-compatible. A struct literal
that names every field must set `reorder`.

While it is on, each enabled visible row is a source and a target. The drag
kind is `LIST_ROW` or `TREE_NODE`, and the owner is the view. Lists use
`reorder` geometry. Trees use `nest`. A list rejects inside and edge. A tree
rejects edge, a drop on itself, and a drop into its own subtree. A no-op
(the order would not change) is rejected too. Disabled rows and rows hidden
by a collapsed ancestor are not sources or targets. Expanding a branch binds
the rows that become visible.

The view does not reorder its model. A successful drop or keyboard move emits
`CollectionEventKind::Reordered(ItemMove)` with the dragged id, the target id,
and the place. The host applies `reorder_list` or `reorder_tree` and mounts
again. Both helpers return `None` when the move is refused or would not change
the sequence. Row entities for surviving ids stay put across that mount, as
they do for any other reorder of the same ids.

`reorder_tree` moves the whole contiguous subtree. Before and after keep the
target's parent and do not insert between a node and its children. Inside
appends the subtree as the target's last children. Depth still stops at 32.
A node cannot be dropped onto a descendant.

Virtual lists (`mount_virtual` and `mount_variable`) use the same list event
and the same autoscroll. Only realized rows can be grabbed with the pointer.
The keyboard path names rows that are off screen.

## What this does not do

- No tab strip, and no code that splits a pane when an edge zone is dropped on.
- No kanban board.
- No drag of a file from the operating system. That remains `FileHover` / `FileDrop`.
- No claim that a platform screen reader spoke the announcement, or that a
  physical pointer was used. The tests are headless `Sim` frames.

## Evidence

```sh
cargo test -p rndr_ui --test item_drag
cargo test -p rndr_core --test layout ignore_pointer
cargo test -p rndr_core --test layout pointer_drag_owner
```

`item_drag` covers classify, the reorder helpers, a drop with a preview, Escape
and an outside release, a rejected kind, autoscroll in a plain scrollport and
in a virtual list, list drop and Alt+Down with the polite status name, a drag
that does not scroll from the pointer delta, tree inside and a descendant
reject, a disabled row, keyboard pickup, and a click that stays a click.
Collection and virtual-list regressions stay on their existing tests.

`cargo run -p rndr --example item_drag` opens a list and a tree. The host
applies `reorder_list` / `reorder_tree` when it sees `Reordered`.
