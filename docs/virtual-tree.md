# Virtual tree

`rndr_ui::tree::VirtualTree` is a tree for file trees, outlines, and other large hierarchies. The host owns the nodes. The tree owns focus, selection, expansion gestures, and which rows exist.

These checks are headless. They do not certify a platform screen reader, a physical keyboard, or a native window.

## Mount

`VirtualTree::mount` places a scroll viewport under the parent. Pass every node the host has loaded, not only the visible window. Children are the nodes whose `parent` is that id, in slice order. `expanded` is the set of open branches. An id in that slice that is not a node is rejected. A later mount with the same id reuses the viewport. The host sets the viewport width and height. The row stride is `TreeProps::row_height` (32 points unless the host changes it) and every row, including a loading row, uses it.

A node is a branch when it is `expandable`, `loading`, or already has children. Expanding a branch that has no children and is not already `loading` emits `LoadChildren`. The tree shows a "Loading…" row until the host mounts the children or clears the expansion. The host applies that mount. The tree does not fetch anything.

Total content height is `visible rows × row height`, stored in the scroll offset as `f32`. Integer row positions stay exact while that height is at most 2^24 points. At 32 points per row that is 524,288 rows. Larger models still virtualize, and the scroll offset is no longer one point per row.

## Keyboard and selection

The tree is one tab stop. Arrows, Home, End, and Page Up/Down move focus inside the visible window. They do not select. Right opens a closed branch, or moves to its first enabled child when it is already open. Left closes an open branch, or moves to the parent. `*` expands every collapsed sibling branch, including the focused one, and does not move focus. Typing focuses the next visible enabled label with that prefix. The buffer lasts 500 milliseconds. Enter activates and does not select. F2 emits `RenameRequested`. The tree does not edit the label.

| Input | Effect |
| --- | --- |
| Up, Down, Home, End, Page | Move focus and keep that row in view |
| Right, Left | Expand, collapse, or move to the child or parent |
| `*` | Expand collapsed siblings |
| Type | Move focus to the next matching label |
| Space, click | Select, according to the mode below |
| Enter | `Activated` |
| F2 | `RenameRequested` |
| Alt+Up, Alt+Down | Ask to move the focused row before the previous visible row, or after the next, when reorder is on |
| Alt+Right, Alt+Left | Ask to move it inside the previous visible row, or after its parent |

`TreeSelection::Single` replaces the one id. Ctrl or Command on that id clears it. `TreeSelection::Multiple` replaces the set on a plain click. Shift selects the visible enabled range from the anchor. Ctrl or Command toggles one id. Shift with Ctrl or Command adds the range. On macOS a Control-click is the secondary gesture, so Command is the toggle. `TreeSelection::None` does not select. A disabled node, and a node under a disabled ancestor, cannot be focused or selected.

## Drag, reveal, and menus

`TreeProps::with_reorder` makes each enabled visible row a source and a target. The kind is `TREE_NODE` (`DragKind(2)`), owned by this tree, so a row from another tree does not land here. The geometry is before, inside, and after. A drop on itself, into its own subtree, or one that would not change the forest is refused. The tree emits `Reordered` and does not move the node. `reorder_nodes` is the helper that returns the next preorder slice, or `None` when the move is refused. Moves are announced on the polite live region. That is the same region item drag uses. Headless tests read its name. They do not drive a platform screen reader.

`reveal` opens ancestors and scrolls the id into the viewport. `focus` does that and moves focus. Keyboard movement also keeps the focused row in view. A wheel that leaves the focused row parks focus on the viewport so the row does not pull the scroll back. The logical id is remembered.

A secondary press (right-click, Menu, or Shift+F10) emits `ContextMenu` with the row id and a point. That event is readable on the frame after the gesture, because the secondary press is published at the Update boundary. If the row was not selected, it becomes the selection first.

## Accessibility

The viewport is a tree. Each realized row is a tree item with its level, expanded state when it is a branch, and its one-based position and set size among siblings. A loading row is a disabled tree item named "Loading". Only realized rows are in the accessibility tree. Set size and position describe the sibling set, not the viewport. The platform adapter publishes those values. This is not a VoiceOver, Narrator, or Orca session.

## Run

```sh
cargo test -p rndr_ui --test tree
cargo run -p rndr_ui --example tree
```

The example expands a small file tree, selects, drags, and prints the time to mount a model of about 100,000 nodes. The test prints the same kind of timing. Both are headless.
