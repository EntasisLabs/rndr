# Virtual semantic lists and the real palette

This increment adds **`ListView::mount_virtual`**, not a second index-selected
collection API. It uses the same `Item`, `ItemId`, `ViewProps`, `RowSlot`,
`SelectionMode::{None, Single}` and `CollectionEvent` vocabulary as retained
ListView. Its handle is `VirtualViewHandle`, because an offscreen row has no
entity to return. See the runnable [100,000-record gallery](../crates/rndr/examples/virtual_list_gallery.rs)
and [its shared application](../crates/rndr_ui/examples/support/virtual_gallery.rs).
The existing retained ListView/TreeView APIs remain available and unchanged.

## Shared engine and lifetime tradeoff

Both core `List` and semantic virtual ListView now call **core `list_window`**
for uniform-height range calculation. Semantic rows use core `Scroll`, clipping,
layout, hit testing, release admission, focus and accessibility. Core
`ViewportRows` is a small content-reconciliation hook **inside canonical layout**,
including the scroll refresh before generic hit testing. A resize gets one
bounded second measure/place pass. There is no `rndr_core -> rndr_ui` dependency.
The legacy `ListRows` index-selected engine remains compatible for existing users;
the palette no longer instantiates it.

We deliberately do **not** pass semantic rows through legacy `List`'s index-based
input or recycle retained composed subtrees behind their owners' backs. Sharing
the uniform window calculation and canonical Scroll geometry, with a small
reconciliation hook, permits different row lifetime/input contracts without a
parallel layout/scroll engine. Semantic realization retains overlapping keys and
reuses retired row roots, replacing their entire caller-populated content entity
and descendants before rebinding. Content components (not just children) never
carry over to another key or renderer.

* `mount_virtual` validates metadata then indexes/copies it: **O(n) time/storage**,
  including names. Call it when metadata changes, not every frame. Maximum count
  is `u32::MAX`; finite positive stride and finite total height are required.
* Mounting/laying out/painting widgets is bounded by the visible window. A window
  realizes at most `ceil(viewport_height / row_height) + 1` rows (partial edges
  included), plus two spacers and an empty label. Index maps are shared with Arc
  during frame processing; frames never clone all logical records.
* The renderer is a function pointer `VirtualCompose = fn(&mut World, RowSlot,
  &Item)`. It runs during viewport reconciliation, potentially **twice per layout
  propagation**. Update/upsert children in place. Do not append each invocation,
  recurse into layout/scroll/focus, remove managed nodes, or mutate the model.
  Read application state from World resources. Newly created styles resolve
  through the existing property-ownership cascade on just this bounded subtree.
* Every row is **uniform-height**. Choose a stride that fits the complete content
  (including touch-target minima and text scale). Set viewport width/height, but
  leave its axis, padding, gap and private children alone. Do not give a virtual
  viewport unconstrained content-hug height if bounded realization is required.
* `row(world, id)` returns `None` offscreen. A realized entity/embedded-control
  handle is **not** a persistent identity. Keep model/editor/checked state keyed
  by `ItemId` outside the row. `item_for_entity` resolves a currently live nested
  request target to its stable key. Do not save a raw entity for later model work.
* Removing/disabling a record retires armed/queued subtree requests. Recycling
  replaces the content entity and all embedded descendants before binding the
  row root to a different key.
  Replacing the renderer also destroys old composed content. Same-key overlapping
  rows retain their content/focus across metadata replacement and reorder.
* This is not variable-height virtualization, lazy/remote data loading, virtual
  TreeView or virtual DataTable. Those retained APIs still have their documented
  budgets. Core geometry is f32: extremely large total heights eventually lose
  sub-row precision; `u32::MAX` is a type limit, not a precision/performance claim.

## Selection, focus, activation and embedded controls

The default contract intentionally matches tested semantic collection behavior:

| Input / operation | Effect |
|---|---|
| Arrow, Home/End, PageUp/PageDown | Move focus/reveal enabled logical rows; **do not select**. Modified/synthetic/IME-owned/blurred input does not navigate. |
| Enter | Non-repeat `Activated(ItemId)`; **does not select**. |
| Passive row click, Space down/up, AT Activate | Canonical selection request, **not host activation**. |
| `select` | Changes selection only; never moves focus/reveals/activates. |
| `focus` / `reveal` | Resolve stable key, realize and reveal; only focus changes canonical focus. Invalid/disabled keys return errors. |
| Filter/reorder/replacement | Surviving selection/focus follow keys, not indices. Removed/disabled selected key clears with an event. Focus falls to an enabled record at/after the old position, then wraps. |
| Deliberate wheel/drag away from focused row | Park canonical focus on viewport, keeping the logical key. No per-frame snap-back. Next navigation reveals that key's neighbor. A genuine viewport resize reveals a still-mounted focused row. |
| Nested Button/Checkbox/other control | **Exclusive** pointer/keyboard/AT activation and focus; never selects or activates the parent. Arrow/Space/Enter/Tab reach that control's own contract. |
| Recycling a focused nested control | Retire armed input, destroy subtree, park on viewport preserving the owning row's key. The exact nested control is not automatically recreated/refocused later. Host-owned model state survives. |

The core canonical key hook is on the row/viewport **only**, never inherited by
children. Disabled row ancestry blocks nested controls too. Nested editor drafts,
open popups and selection ranges are ephemeral: keeping them alive offscreen is
**unsupported**. Commit/cancel them in application-owned state before recycling;
this increment does not claim a persistent offscreen editor/popup lifecycle.
Retained ListView/TreeView continue to promise only passive composition.

`focused` reports logical row focus only while this collection owns canonical
focus (including parked focus). `active` also reports the navigation key when an
external owner, such as a search field, has canonical focus. Event `Focused` is
not an alias for selection or for external-owner navigation.

## Explicit palette policy and external focus ownership

`VirtualListPolicy` is an optional root component. Its default has no hover
navigation, selection-follow-navigation, double-click activation or external
owner. A host can deliberately opt into each independently. The **real palette**
opts into its existing hover/arrow selection and double-click activation
conventions; this does not silently change ordinary semantic lists.

`focus_owner: Some(search_field)` synchronously delegates only passive row and
viewport focus to that valid external owner. It records the row's logical key;
embedded controls still own their own focus. Such rows/viewport leave Tab order.
If the owner is stale/disabled, ordinary row focus is used instead. Core's small
`FocusTarget` component implements delegation in canonical `request_focus`, with
admission of the original target and revalidation of the returned target. It is
not inherited and its callback must not recursively request focus.

The palette installs `KeyNavigation` on its existing `CapturesKeys` search field,
forwarding only navigation/Enter through `handle.forward_key`. Explicit editor
hooks run before ordinary field editing; unhandled keys still type normally.
No hook is installed on other editors by default; transactional ownership runs
first. The palette's Ask prompt declines forwarding, preserving its existing
Enter/Shift+Enter submission/draft behavior. Metadata keys are command IDs rather
than filtered indices, and rendering uses direct command lookup, not a scan of
all matches per visible row. Debouncing, stale-reply rejection, shortcut labels,
fuzzy match highlights, icons, Scroll geometry and focus reveal remain exercised
by the actual palette tests.

## Run and verify

Use the admitted isolated checkout. To preserve the existing build cache:

```sh
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
export CARGO_TARGET_DIR="/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target"
. scripts/data-gallery-env.sh
cargo run --offline --locked -p rndr --example virtual_list_gallery
# Other synthetic presets, one window at a time:
cargo run --offline --locked -p rndr --example virtual_list_gallery -- --dark
cargo run --offline --locked -p rndr --example virtual_list_gallery -- --compact --hc --200
# Actual application (includes its normal tray/hotkey behavior):
cargo run --offline --locked -p palette
```

The gallery includes Filter even, Reverse, Replace labels, Jump last and Disable
first; every realized row has operating Inspect and Marked controls. Its host
acknowledges checkbox requests into a stable-key model, including after recycling.
It is not the old host-paged data gallery, which remains a retained API example.

[Evidence and exact results](../evidence/unified-list/report.md) distinguish
headless display-list/geometry tests from native pixels. Native desktop visual,
physical input/IME, screen reader, Windows/Linux and MSRV verification remain open.
Only realized rows have accessibility entities; a full offscreen AT collection
navigation/position/count protocol is not implemented or certified here.
