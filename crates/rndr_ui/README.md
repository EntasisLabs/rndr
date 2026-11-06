# rndr_ui — semantic components and data editors

An additive, opt-in, GPU-free library above `rndr_core`. The integration/hygiene baseline `0b997b4` includes semantic foundations, operating controls, ListView/TreeView, DataTable, PropertyInspector, Checkbox, SingleSelect and MultiSelect. The owner integrated the data editors and selection controls after review/corrections and hands-on gallery testing. That is not cross-platform native certification. [Current status and boundaries](https://github.com/EntasisLabs/rndr/blob/main/docs/ui-status.md) supersede stale candidate/gate prose in historical A0/A1/A2 records; archived evidence and canonical proposals remain unchanged.

Use this crate directly for headless composition, or enable the facade's `ui` feature and import `rndr::ui`. No semantic names are glob-exported into `rndr`'s legacy root. 0.4.0 publishes this crate. `ui` stays off by default on the facade. The version is a 0.y release, not a 1.0 stability promise, and the checks below are not cross-platform native certification.

## Data browsers (first milestone)

Composable, application-owned `collections::ListView` and `collections::TreeView`
now provide stable-ID rows, passive composition slots, independent focus/selection/
activation, themed scrolling and hierarchical navigation. The standalone interactive
`data_gallery` demonstrates mutation, filtering, resizing and application paging over
10,000 records. It does not own filesystem or database data.

See [API, exact dependency prep/launch commands, limits and roadmap](https://github.com/EntasisLabs/rndr/blob/main/docs/data-components.md).
Selection modes are None/Single only; retained rows are budgeted, **not virtualized**. `ListView::mount_virtual` adds viewport-bounded uniform rows, and `ListView::mount_variable` adds supplied variable heights on the same handle, with stable logical keys, explicitly ephemeral composed subtrees, exclusive nested-control input ownership, and shared core scroll geometry. The real palette uses the uniform mount. See [virtual lists](https://github.com/EntasisLabs/rndr/blob/main/docs/virtual-list.md) and [headless evidence](https://github.com/EntasisLabs/rndr/blob/main/evidence/unified-list/report.md).
[DataTable/PropertyInspector](https://github.com/EntasisLabs/rndr/blob/main/docs/data-editors.md) compose application-owned data and draft → validate → explicit commit. [Selection controls](https://github.com/EntasisLabs/rndr/blob/main/docs/selection-controls.md) are reusable independently and inside the inspector; selecting a draft does not silently write the application model.

On collection rows, screen-reader **Activate selects**, like clicking; keyboard **Enter emits `Activated(id)` without selecting**. These are deliberately separate contracts, not interchangeable action names.

## Install and compose

```rust
use rndr_core::*;
use rndr_ui::*;
let mut app = App::new();
app.add_plugin(UiPlugin); // interaction + scrolling + styling; idempotent, no Shell
let parent = app.world_mut().spawn((Layout { width: Size::Fill, ..Layout::column() },));
let card = Card::mount(app.world_mut(), parent, UiId(1), "Reading").unwrap();
let paragraph = text(app.world_mut(), card.content, "Quiet document content", Typography::Reading).unwrap();
card.adopt(app.world_mut(), paragraph).unwrap(); // explicit transfer of lifetime ownership
app.update();
```

`UiTheme` is explicit Light/Dark plus Standard/High contrast, independent UI/reading text scales and Comfortable/Compact density. Density does not reduce type size. Colors are semantic sRGB roles; only documented tested foreground/background pairs have a contrast guarantee. Strong/subtle strokes are distinct. Focus width **and gap** are independent tokens, not selection/accent; A2 operating controls now render a distinct reserved focus ring/gap and independent selected marker; native focus/capture behavior remains unverified. Opaque surfaces first; no native materials.

`reading_layout` / `reading_layout_at_measure` return ordinary core Layout values, with a nominal adjustable 55–75 character reading measure and narrow-width padding. Hosts supply available width on resize. These are passive recipes, not a second layout engine or window lifecycle. All text heights hug real wrapped content. Separator is decorative, not the sole boundary for a future interactive control.

`rndr_ui::identity(world, entity)` reads a mounted component root's parent-scoped
UiId without a registry or renderer dependency. Pass it as the read-only identity
adapter to [component-targeted headless capture](https://github.com/EntasisLabs/rndr/blob/main/docs/component-capture.md)
for real offscreen pixels and same-frame subtree geometry. Internal/borrowed
nodes and virtual row logical ItemIds are not component UiIds.

## Markdown and core theme integration

[Host-filled markdown blocks and the theme bridge](https://github.com/EntasisLabs/rndr/blob/main/docs/markdown-host-slots.md) keep parsing/streaming in core while trusted hosts mount real controls/live values. `UiTheme::core_theme()` / `markdown_theme()` are pure mappings; `apply_theme_bridge(world, theme)` explicitly replaces this world's UI/core/markdown/default-reading-style resources. Reapply on each theme change; installing UiPlugin alone still leaves core themes untouched. The bridge updates retained markdown/editor/default core consumers without a reverse dependency or global watcher. Explicit per-entity styling remains an override. Markdown slots own attached descendants and cascade on removal, unlike Card's borrowed/adopted-content contract. See the `markdown_gallery` example for streaming, real focus/activation and theme toggling.

## Ownership and precedence

- Existing authored core `Color` (including alpha-zero), `Label.color`, `TextStyle`, `Radius`, and nonzero padding/gap win. Unstyled entities and the core `Theme` resource are untouched.
- App `UiStyle` properties override component `UiRecipe` defaults. Use `UiColor::Explicit` for a persistent explicit override; explicit transparent is never treated as an absent token. App overrides survive Card/Callout recipe updates.
- Only still-owned primitive properties are rethemed. An observed direct primitive mutation/removal relinquishes that property until the semantic property is detached/readded. A direct write equal to the last owned value is intrinsically indistinguishable; use an Explicit UiStyle override or detach first when transferring same-value ownership.
- `detach_style` immediately removes both semantic layers and restores only still-owned primitive state. Removing layers manually releases on the next Layout. It never rolls back app takeovers, removes caller content, or rewrites a buffer/history.
- Stable `UiId` is unique under a parent. Repeated same-family mount updates in place; root/content IDs, caller state, focus and scroll survive. Family collisions and stale handles are errors, not replacement/despawn. Theme changes do not rebuild components.
- Direct content-slot attachments are **borrowed**. `adopt` explicitly transfers a plain subtree; adoption rejects cycles/existing owned UI components. Destroy deletes only owned nodes and detaches borrowed subtrees alive. It clears only owned focus/capture/intents through core teardown APIs. Hosts retain borrowed handles and are responsible for reparenting them after detachment.
- Installation lasts for the App lifetime. Plugin uninstall is intentionally unsupported; repeated installation is a no-op, not a duplicate set of bindings/systems or a reset of queues.

## Desktop foundation

The first post-observation pass uses Inter UI/reading requests, a restrained neutral palette, separate neutral hover/pressed surfaces, rounded controls, transparent inactive focus layers, and compact supporting hierarchy. `Typography::Emphasis` is the small semibold supporting role; `UiStyle::editor()` supplies semantic ink/surface/code typography without replacing core input/history. A direct toolbar control can opt into `controls::ToolbarContentWidth` for measured action widths in rows and full-width stacked fallback. That marker owns `Layout.width` while attached; unmarked controls keep their authored sizing contract. The next layout after marker removal, leaving a toolbar or toolbar teardown restores the saved authored width, without undoing a subsequent caller takeover. Caller width changes made while attached are saved for eventual release.

Native startup now upgrades only core's untouched implicit bitmap engine, even after an early `App::update`. Explicit/custom engines, customized fallback configuration and TextFace are preserved. Cached label UVs/metrics are invalidated on engine replacement. This is not a promise of smooth typography in a no-Parley build. See [historical pass details](https://github.com/EntasisLabs/rndr/blob/main/docs/design/visual-polish-pass-1.md). The later owner gallery checks and integrations are summarized in [current status](https://github.com/EntasisLabs/rndr/blob/main/docs/ui-status.md); historical tool image-delivery failures are not a claim that the saved PNGs were invalid.

## Fonts and honest limits

Families are requests to the already installed text engine. UiTheme now requests the already-bundled Inter for UI/reading and `monospace` for code. Build the native gallery with `parley` (plus `observe` when inspecting); `--no-default-features --features accesskit` alone still deliberately selects the bitmap path. The GPU-free default fallback is the existing public-domain 8×8 ASCII bitmap (font8x8 / IBM VGA provenance in `rndr_paint::font`), not a newly bundled system font or a native typography certificate. The existing renderer/text-engine adapters can supply richer faces. No new fonts/assets or platform font resolver were introduced. Headless IME cases for CJK and Arabic preedit, commit, cancel, and caret keys are in [the complex-script note](https://github.com/EntasisLabs/rndr/blob/main/docs/ime-complex-scripts.md). `reading_layout` is still half an em times the character count, which is a poor column estimate for CJK and Arabic. OS input methods, glyph outlines, and physical DPI remain an open manual matrix. Code font family requests do not guarantee a different face in the bitmap fallback.

AT `SetValue` is explicitly rejected via `UnsupportedSetValue`; no raw Label/Buffer mutation substitutes for a validated editor/history operation. This is a deliberate supported safety outcome. Other A1 APIs, cardinality limits and manual tab/panel prerequisites are documented separately in the decision record.

## Where to start

Read [the status note](https://github.com/EntasisLabs/rndr/blob/main/docs/ui-status.md) first. Then, depending on the
surface:

- Collections and the virtual window: [data components](https://github.com/EntasisLabs/rndr/blob/main/docs/data-components.md) and [virtual lists](https://github.com/EntasisLabs/rndr/blob/main/docs/virtual-list.md). The `data_gallery` and `virtual_list_gallery` examples exercise them.
- Editors and selection: [data editors](https://github.com/EntasisLabs/rndr/blob/main/docs/data-editors.md) and [selection controls](https://github.com/EntasisLabs/rndr/blob/main/docs/selection-controls.md).
- A host-filled markdown block: [markdown slots](https://github.com/EntasisLabs/rndr/blob/main/docs/markdown-host-slots.md) and the `markdown_gallery` example.
- Pixels of one component: [component capture](https://github.com/EntasisLabs/rndr/blob/main/docs/component-capture.md). `capture_isolated` paints the target subtree on its own.

Headless checks:

```sh
cargo test --offline --locked -p rndr_ui
```

Write any new paint dumps under `local-captures/` (gitignored) or another
directory outside the repo. Do not add them under `evidence/`.

## Historical galleries

`operating_gallery` and `passive_gallery` are the older A2 and passive
fixtures. They still run, and the passive golden tests still compare real
paint display lists. They are not the map of the crate. Their write-ups are
`docs/design/A2-components.md` and `docs/design/A0-A1-checkpoint.md`.

Native OS assistive technology, preferences, input methods, and physical DPI
were not certified by those galleries. The live `observe` probe records what a
Unix window submitted. Headless component capture is the separate
[capture](https://github.com/EntasisLabs/rndr/blob/main/docs/component-capture.md) path. Complex-script limits are in
[the IME note](https://github.com/EntasisLabs/rndr/blob/main/docs/ime-complex-scripts.md).
