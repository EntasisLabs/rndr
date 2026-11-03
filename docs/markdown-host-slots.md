# Markdown host slots and the explicit theme bridge

Core still parses markdown and streams incremental tails. A host can replace a
**last-synced block's complete visual body** with a retained ECS container using
`mount_markdown_slot`. Real `rndr_ui` Buttons, core editors/fields and application
labels can be children. There is no new markdown syntax, Liquid interpreter,
markup-to-command execution, raster proxy or dependency.

## Mount, update, remove

```rust
use rndr_core::*;
let mut app = App::new();
app.add_plugin(MarkdownPlugin);
let view = markdown_view(app.world_mut(), "Value\n\nOrdinary markdown");
app.update(); // Bind against parsed blocks, never guessed source indexes.
let expected = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
let slot = mount_markdown_slot(
    app.world_mut(), view, 0, &expected, MarkdownSlotId(1), 48.0,
).unwrap();
let live = app.world_mut().spawn((
    Layout { width: Size::Fill, height: Size::Px(32.0), ..Layout::column() },
    Label::new("Live value: 42"),
));
set_parent(app.world_mut(), live, slot.root);
app.update();
assert!(slot.is_mounted(app.world()));
app.world_mut().get_mut::<Label>(live).unwrap().text = "Live value: 43".into();
slot.set_height(app.world_mut(), 60.0).unwrap();
slot.detach(app.world_mut()).unwrap(); // Destroys root + descendants NOW.
app.update(); // Ordinary markdown body returns.
```

For an actual semantic control, use `rndr_ui::controls::Button::mount(world,
slot.root, UiId(...), ButtonProps::new(...))`. Install `UiPlugin` for ordinary
core focus/activation/styling. Application code drains `take_action_intents` and
validates `controls::host_intent`; neither markdown nor core executes commands.
The executable [gallery fixture](../crates/rndr_ui/examples/support/markdown_gallery.rs)
shows the complete path.

### Identity and ownership contract

- `MarkdownSlotId(u64)` is **view-local**, independent of `UiId` and the private
  parser's retained block entities. An ID is never reused in that view's
  lifetime, even after detach. The view remembers used IDs (memory grows with
  unique mounts); use a new view for unbounded document sessions.
- The binding is **positional and content-checked**: index plus last-synced
  kind, nesting, text, language, links and semantic inline formatting. Theme
  colors are excluded. Identical blocks at distinct indexes are independent.
  An identical replacement at the same index is intentionally indistinguishable
  and preserves state. This is NOT a source-position ID or content-search API.
- Appending/streaming, a full reparse with that block unchanged, changes before
  it that preserve its index, theme changes and width-only resizing preserve
  root/child identities, host state and focus. An insertion/removal before it
  retires the binding only if the resulting block at the bound index differs;
  an identical block left at that index is indistinguishable, even if it came
  from a different source location. Changing that index's content/kind/formatting,
  removing the index or externally deleting the root retires the binding on the
  next sync. There is no automatic migration to a new index.
- Pending source edits reject new mounts conservatively (`StaleBlock`). Wrong
  expected content, missing view/index, duplicate block/ID and invalid height
  reject without partial mutation. Use a new ID to deliberately remount.
- `MarkdownSlot` carries a generation-bearing root plus view/ID; its methods
  validate them. `is_mounted` is false during pending parsing. Query after sync
  before updating descendants that might have been retired. There are no removal
  callbacks or hidden application-state writes. `detach` may cancel a live
  binding even while parsing is pending; a dead/removed handle is rejected.
- The markdown view **owns the root and all descendants**. Attachments are
  transferred, not borrowed: detach, replacement/removal and view `despawn`
  cascade through them. A descendant explicitly reparented outside the root
  before removal is no longer in the owned subtree and survives, per the normal
  core hierarchy contract. Keep durable models/resources outside the subtree.
  This is deliberately different from UI Card's borrowed/adopted-slot contract.
  Do not reparent the root, replace its Layout/Children, remove MarkdownView
  directly, or attach unrelated direct children to the markdown view.
- Hosts own child entities/components and their in-place updates. Core owns
  the root's reading-column width, fixed height and document order. Height must
  be finite/positive; `set_height` reserves new logical points. The normal block
  trailing gap is added. No automatic Hug/measurement negotiation or virtualized
  embedded content is provided in this increment.
- The whole block body is replaced, including code backgrounds, quote/list
  decoration. Host content must fit the reserved rectangle; ordinary ancestor
  clips and scrolling apply. Overlays/portals parented outside the subtree are
  not owned by this slot. Hosts must use their component's teardown contracts
  for such external resources; this increment adds no overlay/IME ownership.

### Interaction and copy

Embedded controls use the **existing** core hit testing, Tab/Shift+Tab order,
focus, activation FIFO, Enter and Space release behavior. No second dispatcher.
A press in a host root does not start markdown text selection/link activation.
Markdown copy/select-all shortcuts yield while focus is within a host subtree.
A press on ordinary markdown text releases the previous control focus, notifies
its normal focus boundary and cancels armed activation; subsequent copy/select-all
belong to that text view, even if a control in another view was focused. Editor
left-drag selection owns its gesture instead of dragging the ancestor scrollport;
wheel scrolling over it still works. Outside hosts, normal markdown selection,
copy, links and streaming remain intact.
Text copied across a replaced block is its original markdown **fallback**, not
embedded control labels/values. Selection does not introspect host controls.
Host-side direct programmatic activation still goes through core admission.

## Explicit UiTheme -> core bridge

The dependency remains `rndr_ui -> rndr_core` only. `UiTheme::core_theme()` and
`UiTheme::markdown_theme()` are pure conversions. `apply_theme_bridge(world,
theme)` explicitly replaces **four world resources** together:

| Resource | Mapping / update contract |
| --- | --- |
| `UiTheme` | Exact supplied theme, retained for semantic recipes/controls |
| core `Theme` | Canvas/Navigation/Content/Raised surfaces, SubtleStroke seams, ControlHover rows, Text ink, Focus caret, Selected highlight (35%-alpha Focus in High contrast), WarningText dirty mark; legacy radius 8 |
| `MarkdownTheme` | Content/Text/Secondary/Accent plus Raised code fills, StrongStroke quote, SubtleStroke rule, Info/Success/Warning token accents |
| default `TextStyle` | `Typography::Reading`, including reading family/scale |

Call the bridge **again on each theme change, before the next frame**. No plugin
watcher, process-global state, OS preference inference or automatic reverse
synchronization exists. `UiPlugin` by itself still leaves core resources alone.
Changing `UiTheme` directly updates semantic components only until the host
explicitly reapplies the bridge. If you prefer custom core themes, use the pure
conversion and edit fields before inserting resources yourself.

Core `style_of` now accepts an explicit world `TextStyle`; when absent it still
uses the existing TextFace/bitmap default. Per-entity TextStyle/Label colors and
host-copied background Colors retain their precedence. Update static root fills
explicitly or use a UiStyle recipe. The bridge intentionally overwrites custom
world Theme/MarkdownTheme/TextStyle resources: don't call it when those have a
different owner. Retained markdown labels/fills now refresh on palette OR default
typography changes, even for unstyled paragraphs. Core Pane's owned row fills
refresh without rebuilding editable rows; observed explicit Color overrides are
preserved (same-value ownership writes remain intrinsically indistinguishable).
Editor/default label glyphs, caret, selection and dirty marks read the core
resources through their normal layout/paint path.

Limits: core has one default text style and one radius, not separate UI/reading/
code roles. The bridge chooses Reading; semantic controls retain UI typography.
Markdown's existing monospace family request stays. Font families are requests
to the existing engine, not native font installation/certification. Typography
changes do not rewrite buffers, undo history or application models.

## Run and reproduce

From this checkout, reuse the established cache; don't create a second target:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
cargo run --offline --locked -p rndr --example markdown_gallery --features observe
# Optional -- --dark --narrow; --complete starts with the whole streamed tail.
cargo test --offline --locked -p rndr_ui --test markdown_slots --test markdown_gallery
cargo test --offline --locked -p rndr_core --test markdown --test pane --test text_engine
cargo test --offline --locked -p rndr_render --features observe --lib capture::tests
```

In the native gallery: watch tokens append while values/controls stay mounted;
Tab/Shift+Tab between Increment and Toggle; Enter activates immediately, Space
activates on release. Resize to narrow, scroll controls out of view/back, change
light/dark without resetting the value. The host owns all synthetic state.

Choose **new, nonexistent** output prefixes for bounded headless evidence:

```sh
cargo run --offline --locked -p rndr --example markdown_gallery --features observe \
  -- --complete --capture-prefix evidence/markdown-slots/light-NEW
cargo run --offline --locked -p rndr --example markdown_gallery --features observe \
  -- --complete --dark --narrow --capture-prefix evidence/markdown-slots/dark-NEW
```

These write a small PNG + same-frame text-free geometry/UiIds, assert real
embedded button identities, and perform nine privacy denial checks. They use the
normal offscreen renderer and builtin headless text engine (not native startup's
Parley upgrade). Files are create-new, not overwritten. A successful first file
may remain if writing its companion fails; the files are evidence, not an atomic
bundle API. Source/evidence hashes and exact checked commands/results are in
[evidence/markdown-slots/report.md](../evidence/markdown-slots/report.md).

## Privacy and verification boundaries

Slots do not weaken [capture](component-capture.md) or
[native observation](native-observation.md). The application must audit **all**
content before setting ObservationContent::Public; new live values do not become
public merely because fallback markdown was public. Missing/Deny or any
ObservationSensitive marker anywhere (including hidden host descendants) denies
the whole frame, even for an unrelated target. No pixel masking or partial
metadata release. Mark sensitive host roots/controls or keep observation denied;
core/UI stay renderer-independent and do not infer secrets. Ordinary copy is a
user command, not a public-content attestation.

The regression suite covers incremental/full parsing, identity/state retention,
shape changes before a host, duplicate/stale/invalid requests, detach/replacement/
cascade cleanup, real focus/activation, clipping/resize, theme mapping and default
editor/label/pane update/override behavior. The capture regression checks embedded
control identity/text-free metadata and denial before identity lookup; the gallery
checks actual GPU capture denial. Offscreen pixels are not proof of native event
routing, OS accessibility, IME or compositor presentation. Windows/Linux, MSRV,
native AT/IME/complex scripts and platform certification remain unverified here.
