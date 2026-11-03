# Composable data browsers — milestone 1

`rndr_ui::collections::{ListView, TreeView}` are framework components, independent of Blooey, Shell, filesystems, databases and document editing. The gallery is a separate synthetic application. `data::DataTable` and `data::PropertyInspector` are implemented; see [data editors](data-editors.md). Reusable Checkbox, SingleSelect and MultiSelect and their inspector integration are also implemented; see [selection controls](selection-controls.md). Charts remain roadmap. This page describes the retained ListView/TreeView contract, not a unified virtualized list.

## Launch the actual interactive gallery

From this checkout, with Rust/Cargo from `rust-toolchain.toml` and the platform's normal native Rust build prerequisites (Xcode Command Line Tools on macOS):

```sh
# Optional on this constrained Mac: reuse the existing compatible build cache.
# This is build output only; do not run commands in or edit that other worktree.
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh

# On a fresh machine, prepare all locked crates FIRST (public dependency downloads).
# The verified workshop already has them; no .deps snapshot or app checkout is needed.
cargo fetch --locked
cargo run --offline --locked -p rndr --example data_gallery --features observe
```

On another machine omit the optional `CARGO_TARGET_DIR` assignment (or point it at your own compatible Cargo cache). The script defaults to this checkout's normal `target`, limits build jobs, disables incremental compilation and unnecessary debug output. Do **not** run `cargo clean` on shared caches. On Linux the existing winit/wgpu/AccessKit native dependencies still apply; physical Linux/Windows launch is not certified by this milestone. `observe` is needed for screenshot evidence, not normal operation; omit it for an ordinary gallery build. The default renderer uses the existing bundled Inter/Parley engine. A no-Parley build deliberately retains the existing bitmap fallback.

Gallery options after `--`: `--dark`, `--narrow`, `--compact`, `--hc`, `--200`, `--empty`, `--filtered`, `--disabled`, `--large`, `--tree-focus`. They are combinable. Example:

```sh
cargo run --offline --locked -p rndr --example data_gallery --features observe -- --dark --narrow
```

Try:
- Type in **Filter**, Reset it, Add/Remove records, Reverse their order, Empty/restore the data.
- Click row content to select; arrows move focus **without** changing selection; Space selects on release; Enter emits activation.
- Click a tree `+`/`−`, or use Left/Right. Try collapsing an ancestor with a selected descendant.
- Toggle Theme, Dense and Disable; resize the native window. Narrow windows stack the two panels. The outer page and each collection scroll independently using core scrolling.
- **10K data** creates 10,000 application-owned records. Previous/Next page compose at most 128 records in each view (plus four tree grouping nodes). Filtering scans the application data, resets the page and updates both views. Page changes/removals intentionally reconcile off-page selection away. The status line shows actual collection events; no real data is read or saved.

## Composition API

Use explicit imports for `collections::Item`; core also has a legacy `Item` type. `UiId` identifies a component under its layout parent; `ItemId` identifies a data row **inside one view**. Never use the current row index as an ID.

```rust
use rndr_core::{App, Layout, Size, children_of, Label};
use rndr_ui::{UiPlugin, UiId, text, Typography};
use rndr_ui::collections::{Item, ItemId, ListView, RowSlot, ViewProps};

let mut app = App::new();
app.add_plugin(UiPlugin); // idempotent; Chrome/GenericInteraction, not Shell
// UiPlugin installs scrolling before canonical Input interaction.
// Older hosts may retain apply_scroll: consumed wheel/drag deltas are not doubled.
let parent = app.world_mut().spawn((Layout {
    width: Size::Fill, height: Size::Fill, ..Layout::column()
},));
let data = vec![Item::new(42, "Color studies"), Item::new(91, "Typography")];
let compose = |world: &mut rndr_core::World, slot: RowSlot, item: &Item| {
    if let Some(label) = children_of(world, slot.content).first().copied() {
        // Mutate text, not the entire Label: preserve style-managed foreground.
        world.get_mut::<Label>(label).unwrap().text = item.name.clone();
    } else {
        text(world, slot.content, &item.name, Typography::Label).unwrap();
        text(world, slot.content, "Custom secondary content", Typography::Secondary).unwrap();
    }
};
let list = ListView::mount(app.world_mut(), parent, UiId(1),
    ViewProps::new("Projects"), &data, compose).unwrap();
app.world_mut().get_mut::<Layout>(list.root).unwrap().height = Size::Px(280.0);
list.select(app.world_mut(), Some(ItemId(42))).unwrap(); // not focus/activation
list.focus(app.world_mut(), ItemId(91)).unwrap();        // not selection
```

Call `mount` again with the same parent/key, new metadata and the composition callback to reconcile. The callback runs once per supplied row per mount, **not every frame/scroll**. Update/reuse your passive child entities; don't append duplicate content on every reconcile. All application model data stays with the caller; the component holds only identity, accessible name, availability, hierarchy metadata and view state. Complex row rendering can use external app state captured by the callback.

The component owns its root, inner interaction/style/disclosure nodes **and the complete composed content subtree**. Filtering/removal/unmount destroys those row subtrees, cancels their armed/queued requests and invalidates old handles. This differs deliberately from passive Card borrowed/adopted lifetime rules: do not insert shared borrowed entities here. Keep external model data outside the row; retrieve slots with `handle.row(world, id)` when needed. Do not reparent or overwrite managed nodes/components. `unmount` removes only this collection's subtree. Different UI families sharing a parent/key return `IdentityConflict`; stale handles return errors rather than replacing other components.

Rows accept **passive composed content**, not nested interactive controls/editors in this milestone. A content subtree can contain labels, images and decorative layouts. Typography and explicit `UiStyle` overrides use the existing style cascade. The component supplies default semantic primary/secondary/disabled ink, surfaces, density and independent selected/focus states; explicit caller overrides remain the caller's contrast/availability responsibility. Managed row structure is private. The stable extension seam is the content slot, not its parent's exact hierarchy.

### Tree data

```rust
use rndr_ui::collections::{Item, ItemId, TreeNode, TreeView, ViewProps};
let nodes = vec![
    TreeNode { item: Item::new(1, "Design"), parent: None },
    TreeNode { item: Item::new(2, "Current"), parent: Some(ItemId(1)) },
    TreeNode { item: Item::new(3, "Color studies"), parent: Some(ItemId(2)) },
    TreeNode { item: Item::new(4, "Other root"), parent: None },
];
// TreeView::mount(world, parent, key, ViewProps::new("Design tree"), &nodes, compose)?;
// tree.set_expanded(world, ItemId(1), true)?;
// tree.set_expanded(world, ItemId(2), true)?;
// tree.focus(world, ItemId(3))?;
```

Supply a **preorder forest**: parents precede descendants; a subtree must be contiguous; IDs are unique across the entire view; depth is at most 32. Missing parents, cycles, duplicate IDs, non-contiguous subtrees, empty accessible names and oversized data are rejected before mutation. Empty datasets are valid. Branch status derives from the supplied children. There is no implicit filesystem discovery or asynchronous loading. Apps can fetch/update their own data and reconcile explicit nodes.

## Interaction and reconciliation contract

Initial modes: `SelectionMode::Single` (default: zero or one selected ID) and `SelectionMode::None`. Nothing is initially selected or activated. Multiple/range selection, toggle-to-deselect and selection-follows-focus are **not implemented**. `select(None)` explicitly clears Single selection. Click/Space in None mode focus but do not select; Enter still activates. No double-click activation, drag-and-drop, typeahead, PageUp/PageDown, editing cells, lazy loading or cross-page selection persistence is claimed.

| Input / operation | Result |
|---|---|
| Tab / Shift-Tab | One roving collection stop, then leave the view. Disabled/hidden rows are skipped. Empty enabled views have a focusable named viewport. |
| Up / Down / Home / End | Move focus among visible enabled rows; no selection or activation; no wrap. |
| Tree Right | Open a closed branch; otherwise move to its first enabled visible direct child. |
| Tree Left | Close an open branch; otherwise move to nearest available ancestor. |
| Primary pointer release / Space release | Select the row, separately from focus. Pointer press focuses. Crossing core scroll-drag slop, Escape, blur, retargeting or removal cancels pending release. |
| Screen-reader / AT Activate on a row | Canonical click action: selects (Single mode), not host `Activated`. In None mode it does not select. |
| Enter | One explicit `Activated(id)` per non-repeat key press; does not select. Enter/Space on a separately focused disclosure button uses canonical button expansion instead. |
| `set_expanded` | Retains subtree IDs/content/selection, hides descendants from layout/input/AT when collapsed. Focus inside that subtree moves to nearest visible enabled ancestor. |
| Reorder / rename | Stable row entities/content, selection and focus follow the ID, not an index. Expansion follows persistent node IDs while they remain branches; losing the last child retires expansion with an explicit event. Leaf indentation remains passive selectable row area. |
| Filtering / removal / page replacement | Omitted IDs are removed, not hidden. Removed or item/ancestor-disabled selection clears with an event. Removed focus chooses the old supplied index clamped into the new enabled visible sequence, or the named empty viewport. Reintroduced IDs have new entities and no restored selection. |
| Whole-view disable | Retains selection/expansion context but disallows focus, selection mutation and activation; canonical focus clears. Item/ancestor disable makes that subtree unavailable; disabled rows remain visible. |
| `focus(id)` / `reveal(id)` | Visible rows only; expand ancestors first. `focus` uses canonical reveal; `reveal` uses the same core path without changing focus/selection and requires measured rects. A following normal focus-reveal can bring the focused row back. |

Listen using core `events::<CollectionEvent>(world)`. Events carry the generation-checked view entity and `Focused(Option<ItemId>)`, `Selected(Option<ItemId>)`, `Activated(ItemId)` or `Expanded(ItemId, bool)`. Input transitions are available to the host in Update. Programmatic transitions become readable at the next core event commit (normally the next frame when called from Update); do not assume immediate `events` readback after a setter. Consume events once in your host's normal Update schedule, not repeatedly across custom schedules. Same-state selection/expansion is a no-op. Focus events describe actual canonical focus (None when focus leaves); don't substitute selection events for focus.

Core owns chronological input, pointer capture/release cancellation, focus eligibility and reveal. `KeyNavigation` is a small opt-in per-entity composite hook **inside** that canonical loop, not a competing input dispatcher. `TabStop` controls sequential traversal without denying arrow/pointer/AT focus. Collection rows and disclosure commands use `ActivationHandler` at core’s accepted-activation boundary: semantic effects complete before subsequent navigation, for keyboard, pointer and AT alike. The handler consumes only its own accepted intent after canonical eligibility and FIFO capacity checks; unrelated host commands retain FIFO order and canonical revalidation. Expansion cancels only newly hidden owned targets through `forget_interaction_targets`, never an unrelated armed host button. The existing FIFO64 action-request bound remains; the collection does not install a parallel request queue.

Accessibility projects List/ListItem and Tree/TreeItem roles, explicit names, bounds, separate selected/enabled/focused states, expanded state, logical tree parent/child relationships and one-based levels. Disclosure commands are separate named buttons, including through passive composition wrappers. The native adapter advertises only supported core actions. AT Activate on a row is the canonical click/**selection** action; it is not Enter/host activation. AT can expand using the named disclosure button. Full collection virtualization, native screen-reader interaction and platform certification are not implied.

## Large data: explicit budget, honest costs

Components retain all supplied rows, including collapsed tree nodes. `MAX_ITEMS = 1024` is a deliberate per-view budget; depth <=32. Mount/reconcile, layout, style and accessibility work scale with supplied rows; layout and native accessibility are **not bounded by viewport size**. Rows can have composed variable-height content. Core clipping/scrolling/reveal is reused, but clipping is not virtualization.

The gallery's pragmatic large-data strategy is **host-owned paging**: 10,000 logical records, at most 128 records per mounted page/view (+4 tree grouping nodes). The public tests measure retained entity counts, verify paging replaces the correct stable IDs while preserving the complete app model, and exercise a full 1,024-row budget to make the nonvirtualized cost visible. Timings are diagnostic measurements on one Mac/headless fallback engine, not portable performance promises. See evidence/report for measured numbers. Future real virtualization needs viewport-bounded mount/layout/style/paint evidence, identity-safe row recycling, variable-height measurement/reveal and accessible navigation—not a renamed Scroll container.

## Reuse audit and future extension seams

- Core `List` already pools uniform-height rows but selection/focus is index-based; composition callbacks run each frame on recycled entities. Reusing it unchanged would violate stable-ID composed subtree lifetime and independent selection. It remains unchanged, including its virtualization tests.
- Core `Disclosure` / `Shell` mix hierarchy navigation with filesystem expansion, saves, editor/global Selection behavior. They remain unchanged; collections use ordinary `Children`/`Parent`, `Hidden` and application metadata.
- Core `Scroll`, layout, clip/hit testing, GenericInteraction/Chrome, Focus, canonical release activation, accessible-name/style machinery are reused. `reveal_entity` extracts the existing core reveal path, rather than introducing a second scrolling engine.
- DataTable now supplies stable row/column identities, sort requests and declarative column widths; PropertyInspector supplies typed drafts, validation and explicit commit/cancel. Their contracts live in [data-editors.md](data-editors.md), with reusable boolean/choice controls in [selection-controls.md](selection-controls.md). Charts and unified virtualization remain future work.

## Verification and remaining gates

```sh
source scripts/data-gallery-env.sh
bash scripts/verify-data-components.sh
# Native evidence: binary is built by the script with observe enabled.
python3 tools/validate_observation_native.py \
  "$CARGO_TARGET_DIR/debug/examples/data_gallery" /tmp/NEW-data-light
python3 tools/validate_observation_native.py \
  "$CARGO_TARGET_DIR/debug/examples/data_gallery" /tmp/NEW-data-dark -- --dark
python3 tools/validate_observation_native.py \
  "$CARGO_TARGET_DIR/debug/examples/data_gallery" /tmp/NEW-data-narrow -- --narrow
```

Use fresh output directories for evidence, preserve logs, and inspect PNG pixels—not merely JSON/hashes. The capture tool observes the actual native rendered surface, checks idle frame identity, capture authorization rejection and clean shutdown; it does not automate mouse/key interaction. Public-component integration tests drive real input through core and render real paint frames. Physical OS input, live screen readers, Windows/Linux behavior, touch, real IME and physical DPI remain manual/platform gaps. No push, deployment or integration is performed by this milestone's implementation worker. See `evidence/data-components/REPORT.md` for exact commands, source hashes, visual inspection and results.

### Correction evidence / original fixture erratum

The original `final-tree-focus` fixture focuses **ItemId(1), a leaf**, and selects **ItemId(3), its sibling**, not a descendant of the focused row. Original implementation/review evidence is preserved historically; current correction source/binary/capture bindings and verification are in `evidence/data-components/corrections/REPORT.md`. This does not certify physical interaction or platform behavior.
