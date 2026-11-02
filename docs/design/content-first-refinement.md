# Content-first framework refinement — consumer guide

This is a bounded implementation increment after accepted `a21d716`, not a new design proposal or a claim of native platform parity. The approved Council Specification and Review Guide are unchanged. The design language is **solid, content-first and explicitly themed**: neutral navigation, a dominant document plane, restrained accent, clear focus and readable text. It does not emulate Liquid Glass, Mica, native preference integration, or OS widgets.

## Compose, then let core lay out and route input

```rust
use rndr_core::*;
use rndr_ui::{controls::*, *};

// Mount with stable UiIds, as before. Retain these handles across updates.
// `workspace` is a caller-owned Layout parent; navigation/document are Surface
// handles and modes is a ModeHandle already mounted in the document slot.
fn arrange(
    world: &mut World, workspace: Entity, navigation: UiHandle,
    modes: &ModeHandle, content_width: f32,
) -> Result<(), UiError> {
    let theme = world.resource::<UiTheme>().cloned().unwrap_or_default();
    let layout = WorkspaceLayout::resolve(content_width, &theme);
    layout.apply(world, workspace, navigation.root)?;
    let document_inner = layout.document_width - 2.0 * theme.space(Space::X16);
    modes.fit(world, document_inner.max(32.0))?;
    Ok(())
}
```

`content_width` is the actual host content width **after host padding**, not an unadjusted window width. Apply on resize and theme/text-scale changes. WorkspaceLayout reserves a 240-point, UI-text-scaled sidebar plus a 440-point scaled document minimum; otherwise it stacks in document order. Density changes rhythm, not font size. The returned metrics also let a host size its document toolbar/reading slot without duplicating breakpoints. `apply` owns only workspace axis/gap and navigation width. Invalid metrics/relations fail before mutation. This recipe does not own, reparent or destroy content.

`section_layout(&theme)` is an unfilled, density-aware column recipe when another filled card would be unnecessary. Its spacing is a resolved value: recompute on theme changes if used directly.

### Tabs that fit their actual labels

`ModeHandle::fit(world, available)` opts into the toolbar's existing **real TextEngine measurement and final content-box proof**. Short labels receive content widths; long/enlarged labels stack visibly. Arrow-key orientation follows the actual layout; selection, focus, panels, borrowed editor identity and history are unchanged. No label is hidden to force a row. Two device pixels of width allowance prevent fractional native advances from losing a pixel to nested snapping; final fit is still validated.

`clear_fit` releases its tab widths on the next Layout and restores still-owned group width/gap. It preserves pre-existing ToolbarContentWidth markers and caller width/gap takeovers, including a pending resize that has not reached Layout. Current orientation remains explicit; a subsequent mount may choose another. `destroy` retains the existing ownership rules. There is no second layout tree, renderer, focus manager or input engine.

For an ordinary toolbar, opt direct actions into `ToolbarContentWidth` as before. Unmarked widths and explicit core/Layout overrides are still honored. Supply a real available content width and do not hide overflowed essential commands.

## Type and text boxes

| Role | Logical size / line height | Purpose |
|---|---|---|
| Title | 26 / 34 | Window/document title |
| Heading | 20 / 28 | Section hierarchy |
| Reading | 16 / 24 | Content, not control labels |
| Label | 14 / 20 | Commands/navigation |
| Emphasis | 13 / 18 | Compact supporting headings/actions |
| Secondary | 12 / 16 | Metadata and supporting information |
| Code | 13 / 20 | Existing editor/source role |

UI and reading/code scales remain independent explicit UiTheme values. Density never scales down type. Inter is requested by the existing bundled Parley path; the no-Parley deterministic bitmap fallback remains deliberately different. This is not a new native font resolver.

`text(world, parent, value, role)` and owned Card/Callout headings now use core's additive `LabelBox::CONTENT`: text is measured, shaped and painted with no hidden legacy field inset or 36-point minimum. The containing section/surface owns rhythm. Controls use a small explicit label inset and preserve their separate minimum target and focus ring. Comfortable/compact control face padding resolves to two/zero points; core authored padding overrides still win.

**Legacy behavior is opt-in safe:** Label/Layout/TextStyle structs have not gained fields. Labels without LabelBox retain 12/8-point text insets and the 36-point legacy minimum. Removing LabelBox restores those metrics, including cached wrap width. Invalid boxes fall back to legacy metrics. Field, Editor, Buffer, Caret and TextSelection labels ignore LabelBox; existing selection, pointer coordinates, IME and history stay on the field contract. Do not attach display-only metrics to selectable/editable content and expect a new editor-layout API.

## Navigation and state recipes

NavigationRow renders its primary label and smaller Secondary metadata as separate owned display labels. Dirty state remains visibly written **Unsaved** and folded into the root accessible name. The selected prefix remains a non-color cue; focus is a separate ring. Metadata nodes are validated/destroyed with the control. Clearing metadata hides its owned line without affecting the primary label or action identity.

Quiet controls and inactive tabs resolve against their nearest solid host plane instead of stamping Canvas-colored tiles into content surfaces. The preview uses the same style ownership/cascade as the real style pass, including same-update theme changes and explicit Color takeovers. High contrast retains explicit boundaries. Selected rows/tabs still show press/hover feedback without losing their persistent selection cue.

Authored colors remain authored: framework contrast guarantees cover documented semantic pairs, not arbitrary custom/translucent backgrounds. Defaults are solid. No native-material claims are made.

## Run and inspect the operating gallery

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo build --offline --locked -p rndr --example ui_gallery --no-default-features --features parley,observe
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-refinement-new
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-refinement-stress-new --dark --hc --compact --narrow --200 --long --preview --focus-source
```

Use a fresh output directory. `--preview` activates the real selector. `--states` shows an unavailable action, hover, focus on a different navigation row from the selected row; `--pressed` holds the secondary action through the canonical core Pointer input path. These are deterministic **synthetic fixture inputs inside the native example**, not OS keyboard/pointer injection or physical-input certification. `--focus-editor` / `--focus-source` use normal focus reveal to inspect lower parts of the tall stress page. `--200` is application text scaling, not physical DPI. All source/document text is synthetic.

For user-driven native input/AT evaluation, build with `accesskit,parley,observe`, launch the example directly, and exercise the existing keyboard/pointer/editor path. Observation is explicit opt-in (`observe` feature **and** `--observe` flag); never publish the live session descriptor or capability. Saved PNGs are Metal application-surface copies before present, **not compositor screenshots**.

## Evidence and limits

See `evidence/refinement-1/README.md` for exact native before/iteration/after paths, visual critique, quality rubric, build hashes, test commands and platform gaps. The new passive goldens are bounded regression fixtures, not pixel/aesthetic approval. Historical evidence and approved proposals were not rewritten. This increment requires owner independent review and native integration; it was not pushed or integrated by the executor.
