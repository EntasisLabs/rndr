# rndr_ui — foundations and contracted A2 components

An additive, opt-in, GPU-free library above `rndr_core`. The owner accepted exact04642's **HEADLESS A0/A1 core-contract checkpoint** and lifted the A2 gate. Operating A2 controls and a synthetic-host gallery are now implemented for fresh independent review. This is **not** native certification, integration or whole-project acceptance. Historical checkpoints/proposals remain unchanged; see `docs/design/implementation-tracking.md` and `docs/design/A2-components.md` for the current boundary.

## Install and compose

```rust
use rndr_core::*;
use rndr_ui::*;
let mut app = App::new();
app.add_plugin(UiPlugin); // idempotent; reuses Chrome/GenericInteraction, not Shell
let parent = app.world_mut().spawn((Layout { width: Size::Fill, ..Layout::column() },));
let card = Card::mount(app.world_mut(), parent, UiId(1), "Reading").unwrap();
let paragraph = text(app.world_mut(), card.content, "Quiet document content", Typography::Reading).unwrap();
card.adopt(app.world_mut(), paragraph).unwrap(); // explicit transfer of lifetime ownership
app.update();
```

`UiTheme` is explicit Light/Dark plus Standard/High contrast, independent UI/reading text scales and Comfortable/Compact density. Density does not reduce type size. Colors are semantic sRGB roles; only documented tested foreground/background pairs have a contrast guarantee. Strong/subtle strokes are distinct. Focus width **and gap** are independent tokens, not selection/accent; A2 operating controls now render a distinct reserved focus ring/gap and independent selected marker; native focus/capture behavior remains unverified. Opaque surfaces first; no native materials.

`reading_layout` / `reading_layout_at_measure` return ordinary core Layout values, with a nominal adjustable 55–75 character reading measure and narrow-width padding. Hosts supply available width on resize. These are passive recipes, not a second layout engine or window lifecycle. All text heights hug real wrapped content. Separator is decorative, not the sole boundary for a future interactive control.

## Ownership and precedence

- Existing authored core `Color` (including alpha-zero), `Label.color`, `TextStyle`, `Radius`, and nonzero padding/gap win. Unstyled entities and the core `Theme` resource are untouched.
- App `UiStyle` properties override component `UiRecipe` defaults. Use `UiColor::Explicit` for a persistent explicit override; explicit transparent is never treated as an absent token. App overrides survive Card/Callout recipe updates.
- Only still-owned primitive properties are rethemed. An observed direct primitive mutation/removal relinquishes that property until the semantic property is detached/readded. A direct write equal to the last owned value is intrinsically indistinguishable; use an Explicit UiStyle override or detach first when transferring same-value ownership.
- `detach_style` immediately removes both semantic layers and restores only still-owned primitive state. Removing layers manually releases on the next Layout. It never rolls back app takeovers, removes caller content, or rewrites a buffer/history.
- Stable `UiId` is unique under a parent. Repeated same-family mount updates in place; root/content IDs, caller state, focus and scroll survive. Family collisions and stale handles are errors, not replacement/despawn. Theme changes do not rebuild components.
- Direct content-slot attachments are **borrowed**. `adopt` explicitly transfers a plain subtree; adoption rejects cycles/existing owned UI components. Destroy deletes only owned nodes and detaches borrowed subtrees alive. It clears only owned focus/capture/intents through core teardown APIs. Hosts retain borrowed handles and are responsible for reparenting them after detachment.
- Installation lasts for the App lifetime. Plugin uninstall is intentionally unsupported; repeated installation is a no-op, not a duplicate set of bindings/systems or a reset of queues.

## Desktop foundation candidate

The first post-observation pass uses Inter UI/reading requests, a restrained neutral palette, separate neutral hover/pressed surfaces, rounded controls, transparent inactive focus layers, and compact supporting hierarchy. `Typography::Emphasis` is the small semibold supporting role; `UiStyle::editor()` supplies semantic ink/surface/code typography without replacing core input/history. A direct toolbar control can opt into `controls::ToolbarContentWidth` for measured action widths in rows and full-width stacked fallback. That marker owns `Layout.width` while attached; unmarked controls keep their authored sizing contract.

Native startup now upgrades only core's untouched implicit bitmap engine, even after an early `App::update`. Explicit/custom engines, customized fallback configuration and TextFace are preserved. Cached label UVs/metrics are invalidated on engine replacement. This is not a promise of smooth typography in a no-Parley build. See [pass details and remaining visual gate](../../docs/design/visual-polish-pass-1.md): live surface captures were obtained, but actual image inspection is blocked by the current tool surface and aesthetic approval remains pending.

## Fonts and honest limits

Families are requests to the already installed text engine. UiTheme now requests the already-bundled Inter for UI/reading and `monospace` for code. Build the native gallery with `parley` (plus `observe` when inspecting); `--no-default-features --features accesskit` alone still deliberately selects the bitmap path. The GPU-free default fallback is the existing public-domain 8×8 ASCII bitmap (font8x8 / IBM VGA provenance in `rndr_paint::font`), not a newly bundled system font or a native typography certificate. The existing renderer/text-engine adapters can supply richer faces. No new fonts/assets or platform font resolver were introduced. Correct CJK/Arabic/emoji shaping and native font fallback remain **unverified**; the localization fixture tests exact content retention, not native glyph correctness. Code font family requests do not guarantee a different face in the bitmap fallback.

AT `SetValue` is explicitly rejected via `UnsupportedSetValue`; no raw Label/Buffer mutation substitutes for a validated editor/history operation. This is a deliberate supported safety outcome. Other A1 APIs, cardinality limits and manual tab/panel prerequisites are documented separately in the decision record.

## Reproduce the headless gallery

From the isolated worktree, using the existing local cache:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
# NEW nonexistent output only: the A2 generator refuses an existing directory.
cargo run --offline --locked -p rndr_ui --example operating_gallery -- /tmp/rndr-a2-gallery-CHOOSE-A-NEW-NAME
cargo test --offline --locked -p rndr_ui
```

The new operating gallery includes all four Button variants, a named IconButton, NavigationRow selection/metadata, real manual tab/panel relations, responsive toolbar, reading content, passive Card/Callout, visible labelled source action, nested scroll and the existing Buffer editor. Its host consumes only core FIFO64 typed requests and uses synthetic command/locator validation; it never adopts Blooey or opens real documents. Native launch instructions and the per-platform matrix are in `docs/design/A2-components.md`.

The **preserved passive gallery** creates surfaces, Card/Callout, separators, typography, long text and localization fixtures. It writes full-document **actual rndr paint display lists, glyph UVs, colors, clips and layout rectangles** for six representative states. These are not native screenshots, an alternative renderer, or world-only style assertions. Golden tests compare the real paint output. A 32-state test matrix checks every visible Hug parent's containment and complete ASCII glyph coverage; nested scroll tests inspect actual paint scissors. Glyph geometry is checked at 100%/200% text and fractional/2× device scales.

The first display-list audit exposed a real nested wrapped-Hug clipping defect despite simpler world tests passing. The reproduced core fix resolves width-constrained heights before ancestor clips. Baseline/fixed logs and separate core commits preserve that evidence rather than silently blessing incomplete gallery output.

See historical `docs/design/A0-A1-checkpoint.md` for its exact checkpoint evidence. Current control purpose/props/slots, keyboard/pointer/AT behavior, overflow, lifecycle/state/unsupported contracts and native launch matrix are in `docs/design/A2-components.md`. New A2 display-list evidence is separate from all six old passive goldens and historical reports. Native windows/screenshots/capture-blur/OS AT/preferences/fonts/shaping/physical DPI remain **UNVERIFIED**.
