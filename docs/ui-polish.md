# Hands-on UI polish: behavior and retest guide

Scope: the five reports in owner turn 342, against `8ecd94c42036986a204330223514e5909d700702`. This is a review candidate, not native/platform certification or release readiness. No capture permissions or redaction rules were relaxed.

## Reproduced causes and corrections

| Report | Reproduced cause | Shared correction |
| --- | --- | --- |
| Checkbox X sits low | The indicator was a lowercase `x` laid out as text, with a baseline/label inset inside an 18-point face. Its ink centroid is not the face center. | `CrossMark` emits symmetric vector geometry relative to the face, not font metrics. Checkbox name, checked value, host-owned state, focus and activation remain on the original semantic control. This also fixes boolean property-editor checkboxes. |
| Disabled/unselectable click seems to highlight ancestors; blue gutters/bottom fill | Collection fallback focus legitimately lands on its view, but the view recipe used the focus token as its **entire background**. Rows also layered a focus-colored outer fill behind a differently colored inner surface. Disabled status was not inherited paint; it exposed the view-focus recipe. | A view keeps its normal surface. Focus is a clipped, inset outline, painted above descendants. Rows have one fill, with a separate eligible-row outline; disabled/read-only access and navigation remain canonical. Parent-view focus is still allowed where appropriate, but never fills the viewport blue. |
| Header/body mismatch | Comfortable-density track origins already matched, but header button chrome and body content used different text insets; column alignment did not apply to header text. Compact collection density additionally shifted body tracks three points relative to the header. | Table-owned density-invariant track insets, exact existing column widths, matching header/body content insets and declared alignment. Header/body share the table's horizontal scroll; the backing row view retains legitimate vertical scrolling. Narrow columns are clipped rather than silently redistributed. |
| Nested scrolling feels trapped / regions unclear | Wheel input went only to the deepest scroll ancestor, discarded overflow at its boundary, and fell back to an unrelated root on a miss. Focus reveal could also jump a partially visible oversized row to its trailing edge. | Per-axis bounded consumption and residual chaining along the actual clipped hit ancestry; no unrelated-root fallback. Oversized visible focus targets remain stable, with actual clamped movement propagated. Theme-aware passive thumbs identify overflowing axes, docked to visible clipped viewport edges. |
| Focus corners become thin/rough | Control nested radii were `10 / 6 / 6 / 4`, despite successive insets `2 / 2 / 1`. Those arcs are not concentric: a nominal two-point focus border becomes thinner around its corners. The existing rounded-distance AA was not the cause. | Shared control radii are `10 / 8 / 6 / 5`, matching their insets. Collection outlines use the ordinary clipped path renderer and existing AA. No demo-only position offsets or global AA/gesture-policy rewrite. |

## Intended nested-scroll contract

* The innermost **visible hit scrollport that can move on an axis** gets first use of that axis. Legitimate inner scrolling is intentional; hovering an inner list does not mean “always scroll the outer page.”
* Consumption is bounded to available distance. If an event would move the inner port 230 points but only 200 remain, it consumes 200 and the same event sends the remaining 30 to its parent. At a boundary, the full matching-axis delta can chain. Reversing direction scrolls the inner port first again.
* X and Y are independent. A horizontal-only inner table does not trap vertical input. There is no new Shift-wheel mapping, diagonal axis lock, horizontal-to-vertical conversion, acceleration, momentum generation or “route everything outward” policy.
* Only the hit's real ancestors participate. Hidden/clipped content cannot hijack a sibling's event; a miss does not scroll an unrelated root. Residual input beyond the outermost boundary is not reassigned elsewhere.
* Contact/drag capture, splitter ownership, editor drag-selection, keyboard focus/tab and focus-reveal suspension keep their existing contracts. The only focus-reveal adjustment is avoiding trailing-edge jumps for oversized targets already overlapping the viewport, and carrying actual clamped movement.
* `UiPlugin` supplies theme-aware `ScrollIndicator` decoration to its world's `Scroll` viewports. Overflow thumbs are **passive position/extent indicators**, not draggable scrollbar controls, hit targets or extra accessibility/tab stops. Core-only hosts opt in with that component. Indicators do not reserve layout space or change input ownership. A partially clipped port keeps its indicator at its visible clipped edge.

## Rebuild and relaunch the candidate

Until the owner integrates this candidate, build **from its isolated worktree**, not unchanged `main`. Shared-cache executable names can be overwritten by builds from another checkout; rebuild from the intended checkout immediately before testing.

```sh
cd '/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-hands-on-visual-and-nested-scroll-polish'
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
df -h .
cargo build --offline --locked -p rndr --features observe \
  --example ui_polish --example selection_gallery --example data_gallery \
  --example data_editors_gallery --example markdown_gallery

# Open one at a time. Close it before launching the next scene.
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene selection
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene data --dark
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene editors --dark --narrow
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene scroll --narrow
```

After owner integration, the same build/run commands work from `/Users/theelevators/rndr`. Do not launch a stale shared binary and assume it belongs to that checkout. Native DPI comes from the actual screen; `--2x` is a **headless** scale probe, not a way to certify a native Retina/non-Retina window.

## Hands-on checklist

1. **Checkbox:** use `selection`; toggle repeatedly, Tab/Space, disable/reenable, change theme/text scale. The X stays centered in the square, including in boolean property editors. Checked state and accessible name must not reset.
2. **Surfaces:** use `data`; focus Current projects, click its disabled row and empty space, hover disclosure/indent areas, move selection, collapse branches. View focus may show a quiet outline; no blue viewport fill or filled gutters. Disabled rows must remain non-activatable/non-focusable, while read-only navigation follows the established contract.
3. **Grid:** use `editors`; compare Project/Progress/Enabled header **track boundaries and text alignment** with the body. Sort, resize narrow, horizontally scroll both directions, then vertically scroll the body. Header/body X movement must match. Clipped header/cell content must not receive clicks outside its visible area. Cells may be wider than a narrow viewport; the horizontal indicator makes that explicit.
4. **Scroll:** use the dedicated `scroll` scene. Wheel/trackpad over the inner region: it moves first; at its top/bottom the parent moves without hunting for a tiny edge gutter. Reverse direction. Move over the labeled parent-only region: only the parent moves. Use the narrow editors table for horizontal/vertical mixed-axis testing. Test a large event that crosses a boundary and small repeated events; diagonal and momentum behavior need real devices. Passive thumbs show which axes overflow; they are not draggable handles.
5. **Borders:** Tab to Disable in `editors`, or use `--focus-label Disable`. Inspect straight edges and corners in both themes on real 1x/2x displays; check that the outline is not covered by child content. Repeat with text scaling/high contrast.
6. **Preservation:** edit/select/copy text, drag-select in an editor inside markdown, tab through nested controls, scroll a large virtual list, reorder/filter then scroll back. Focus, values, host selection and theme should survive. These paths have headless regressions, but require native hands-on confirmation too.

## Automated evidence and limitations

See [`../evidence/hands-on-polish/README.md`](../evidence/hands-on-polish/README.md) for exact commands, counts, source/binary hashes, before/fixed PNGs, same-frame clipped geometry and visually inspected comparison crops.

Fresh PNGs are actual headless GPU readback with real text shaping, not screenshots from a native surface. The matrix is pairwise (light normal 1x, dark normal 2x, light narrow 2x, dark narrow 1x), not every combination. The shared indicator is additionally pixel-tested at three sizes and both scales. No native pointer/trackpad, momentum, native accessibility/screen-reader, OS IME, Windows/Linux desktop or MSRV certification is claimed. All remain owner/platform checks; rebuilding native galleries is not evidence that those interactions passed.
