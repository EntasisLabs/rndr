# Visual polish pass 1 — desktop foundations candidate

This is a substantive first **implementation candidate**, not visual approval or full A2 acceptance. It starts from isolated baseline `6e1f1d2630ce94f6fd7ee4b1088b584645b2865c`. No dependencies, font assets, host application adoption, interaction protocol permissions or other undertaking branches are changed.

## Evidence first, and the unresolved inspection gate

The actual native gallery was rebuilt and launched through `tools/validate_observation_native.py`, using the integrated opt-in authenticated observation path. The unmodified baseline was built **with Parley enabled** but observation reported `builtin_atlas`. Metal/Apple M4 supplied a 1000×700 application surface at device scale 1. The native harness saved two matching PNG/semantic pairs and exercised descriptor/authentication/stale-request/default-off checks. These are **live surface copies before present**, not headless recreations and not OS-compositor screenshots; `presented_confirmation` is false.

However, **no PNG has been visually inspected by this worker**. The admitted tool surface has no local image reader: capability image search returned no match; MCP discovery was unavailable; browser observation requires a browser-host client that is absent. Chat image-read can reopen an existing attachment ID but cannot register these local paths. The coordinator was informed promptly, investigated its own tools, and reported the same gap; the principal is adding a native file-image tool. Pixel statistics, semantic trees, display lists and passing tests are not substituted for visual review. No aesthetic sign-off is claimed. The exact baseline/after paths and inspection checklist are in `evidence/visual-polish/README.md`.

## Typography and rendering defects addressed before styling

1. **Early update selected the wrong native engine.** The gallery's shared constructor calls `App::update` before the desktop runner. Core installs an implicit bitmap fallback; the runner previously treated every installed engine as intentional and never installed Parley. Core now tracks the original temporary fallback by weak identity. Native startup may upgrade only that untouched fallback (epoch zero, no TextFace). Explicit builtin/custom engines, customized fallback configuration and legacy TextFace resources stay application-owned. No-Parley builds retain bitmap behavior. This applies beyond the gallery.
2. **Engine replacement could leave stale glyphs.** Label layout caching included an engine epoch but not engine identity. Two engines can both start at epoch zero with completely different atlas UVs, metrics and caret stops. The cache now binds identity as well. Independent correction review anchors each label with a weak engine identity, avoiding reuse of a freed engine's raw address without retaining its atlas. A strengthened regression compares the complete stored TextLayout against the replacement engine's layout; it failed before this correction and passes afterward. Merely changing observed engine provenance would not have fixed this.
3. **The editor was not on the semantic ink/surface path.** It relied on core's default label color, unrelated to UiTheme. The reusable `UiStyle::editor()` recipe supplies Text/Content, Code typography and spacing/corners while leaving Buffer, Editor, history, caret and selection with core.

Inter was already bundled in the existing Parley backend. UiTheme now requests it explicitly for UI/reading; code requests monospace. No new font assets or resolver were added. The GPU-free/no-Parley engine is still the ASCII bitmap fallback. Host font fallback, real CJK/Arabic/emoji shaping, IME and physical DPI are not certified by these changes.

## Reusable desktop system, and limited benchmark adoption

- Calmer neutral light/dark surfaces and a restrained blue accent; high contrast remains a separate palette.
- Neutral `ControlHover`/`ControlPressed` distinguish ordinary interaction from persistent selection and from keyboard focus. Primary controls retain accent-state fills. Disabled text remains readable. Existing state tests retain ≥4.5:1 text contrast and the two-color focus geometry.
- Rounded control layers and transparent **inactive** focus layers avoid stamping canvas-colored seams onto raised hosts. Focus still reserves its independent two-point ring and two-point gap, with the same hit target and ordering. Selected rows/tabs keep their independent marker and semantic selection state.
- Readable 14/20 control labels are retained, preserving the independently reviewed exact toolbar fit boundaries. Reading is 14/22, heading 18/26, title 26/34; supporting labels use Secondary ink and callout titles use a compact 13/18 semibold Emphasis role. Density never reduces typography.
- `ToolbarContentWidth` opts direct actions into measured row widths, rather than equal-width slabs, and restores Fill when stacked. It explicitly owns Layout.width while attached. On marker removal, reparenting outside a toolbar or toolbar destruction, the next layout restores the saved authored width unless the caller has taken over; caller width changes while attached are saved for release. Unmarked authored widths are not rewritten. Existing final-content-box validation and its full boundary regressions remain in force; no actions are hidden.
- The shared operating gallery uses only the new toolbar sizing marker and editor recipe. The system tokens and control/surface changes propagate to other opt-in framework consumers, without a broad page redesign or a separate gallery-only theme.

The six active passive-gallery goldens are in `evidence/visual-polish/passive-gallery`; **all historical `evidence/ui-gallery` files remain unchanged**. Updated goldens are rendering regression fixtures, not evidence that the new appearance was visually approved.

## Reproduce

From this isolated worktree, with existing caches (no target cleanup):

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo build --offline --locked -p rndr --example ui_gallery --no-default-features --features parley,observe
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-polish-CHOOSE-NEW
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-polish-stress-CHOOSE-NEW --dark --hc --compact --narrow --200 --long
```

For interactive keyboard/pointer/AT checks, include `accesskit` as well and run the example directly. Observation needs both the feature and the explicit `--observe` launch flag. Do not publish session descriptors/capabilities. `--200` changes application text scale, **not** device DPI. The old `--no-default-features --features accesskit` command deliberately omits Parley and will still show bitmap text; do not use it as the polished typography benchmark.

```sh
cargo run --offline --locked -p rndr --example ui_gallery --no-default-features --features accesskit,parley,observe -- --observe
```

## Independent correctness review

Review of `c235a715ba7fa2986b5219b504bc3c0f43194dab` against the baseline reproduced two ownership defects and corrected them: toolbar width release and legacy TextFace preservation before the first update. Raw cache-address identity was also hardened to a weak allocation identity. See `evidence/visual-polish/review/README.md` for severity, focused test receipts and limitations. The original candidate-range diff check failed on five log EOF blank lines despite a clean working-tree check; the logs and the misleading check claim are corrected. No dependency changes or new design scope were added. Existing captures remain evidence of their recorded source, not a fresh native run of these corrections. No image inspection or aesthetic sign-off occurred in independent review.

## Verification and remaining gates

Detailed commands, logs, counts, build hashes and artifact hashes live beside the evidence. Relevant suites cover core/paint, the entire UI suite, renderer with Parley+observe and without Parley, and Python observation clients. New coverage checks startup ownership, exact replacement-engine glyph geometry, toolbar reflow at fractional/device/text scales and both densities, focus/activation preservation, editor/secondary contrast across palettes, neutral hover and distinct focus/selection. The existing toolbar pixel-boundary tests remain unchanged. The device-scale paint test now compares exact logical TextLayout separately from <1pt physical pixel snapping: its former 16pt fixture happened to divide the tested pixel grids; the 14pt reading role does not.

Still required before visual acceptance:

1. Open the actual before/after PNGs in a supported image-view tool. Inspect type rendering, hierarchy, control density, seams/corners, editor contrast, clipping and unused space; revise based on actual images rather than approve by tests.
2. Compare those live-surface images to the real OS-visible window at matching viewport/theme/scale. No OS screenshot proof was taken.
3. Exercise pointer/keyboard/focus/resize/scroll and editor selection/Undo/Redo in the native window; assess responsiveness (the pre-existing extra text-layout-work caveat is not resolved by this pass).
4. Perform VoiceOver/OS accessibility, IME, native font fallback and physical-DPI checks; other platforms remain unverified.
5. Independent review and governed integration by the coordinator. This worker's completion is not acceptance; no push, deployment or integration is performed here.
