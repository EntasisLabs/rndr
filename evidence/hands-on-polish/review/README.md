# Independent review / correction of the five-issue polish candidate

Reviewed input: `4ea2ac49d900724d5809b2d781c0108e220c87ba`, baseline `8ecd94c42036986a204330223514e5909d700702`. Same isolated undertaking; no native approval, merge, push, publication, deployment, dependency changes, cache cleaning or unrelated source changes. Owner retains sender review and integration. This report describes the reviewed correction commit containing it; `reviewed-source.sha256` binds its source without a self-referential commit hash.

## Findings and disposition

**One reproduced shared-paint defect was corrected.** The new passive scrollbar used the full scrollport length to size its thumb even when an ancestor hid part of that port. Leading clipping also left the position describing the hidden prefix instead of the visible content. A 100-point port with only 70 points visible and 300 points of content failed the independent extent assertion. `before-fix.log` preserves the failure.

`record_scroll_indicator` now uses the visible interval length, adds the clipped leading prefix to the content origin, and maps that interval onto the clipped track. The existing 18-point legibility minimum remains explicit; tiny thumbs therefore are not a literal pixel-proportional scale drawing. Indicators are suppressed where the cross-axis visible strip cannot contain their ink. No hit, layout, focus, accessibility or scrolling components are added by painting. No wheel or focus algorithm needed a further correction in this review.

New/strengthened regressions cover:

- Both axes, unclipped/leading/trailing/both-end clipping, interior offsets and both boundaries: 24 direct painted-thumb configurations, plus a too-narrow cross-axis clip. Hit results remain identical before/after paint; indicator ink never broadens a clip.
- Fractional residual consumption and boundary reversal, with explicit higher-layer unrelated roots blocking underlying ports. An initial equal-layer test incorrectly assumed creation order guaranteed ECS paint order; the final fixture uses `Layer(10)`. That initial failure was a test assumption, not a second framework defect.
- Focused-editor wheel ownership: the actual `Scroll` ancestor moves fractionally; editor offset, canonical selection and focus remain stable, including the next idle frame. This preserves the existing distinction between editor caret/drag scrolling and `Scroll` wheel ownership; it does not invent editor wheel policy.
- All three new primitives (`Border`, `CrossMark`, `ScrollIndicator`) under missing public attestation, sensitive content elsewhere in the composite, and a sensitive descendant. Whole-frame privacy denial remains active.

The first editor fixture used the wrong constructor and then initialized drawing `Caret` instead of canonical `Buffer.caret/anchor`; both were corrected before final validation. The first privacy fixture exceeded supported bundle arity; it was corrected to insert the last component separately. None of these test-construction errors required product changes.

## Five-issue review

| Area | Independent assessment |
| --- | --- |
| Checkbox | Vector mark is face-relative, theme-colored and hidden when unchecked; no replacement semantic control. Fresh GPU tests cover 18/27/36-point faces at 1x/2x. Original/fresh decoded 18-point ink moves from `(8.2960, 10.8240)` to `(8.5, 8.5)`. The enlarged comparison visibly confirms a low text x versus a centered, larger symmetric X. Text-scale and disabled/checked identity checks passed. |
| Blue fill / disabled / read-only | The removed plane was the collection view's own filled focus recipe, not color inheritance by a disabled child. Focus remains on the canonical view/eligible row and draws above child surfaces. Fresh core/UI suites cover disabled/read-only access, keyboard activation/navigation, selection, tab, embedded controls and virtualized identity/recycling. The tree comparison visibly removes the blue gutters and bottom plane while retaining the selected-row fill. |
| Table | Comfortable/compact tests independently pass at 800/220-point widths, with minimum-width long labels, declared start/center/end alignment, horizontal offsets 0/-100 and clipped header labels. Header/body tracks and widths agree. Body cell padding and header chrome now provide the same content inset. The comparison also differs in focus-reveal scroll origin and row heights; it is **not an isolated same-origin pixel proof** of alignment. Geometry/contract tests, not the crop alone, support that claim. |
| Nested scrolling | Actual clipped hit ancestry, separate axes, bounded residuals, non-scrollable inner axes, misses, unrelated/overlapping roots, boundary reverse and fractional deltas passed. The genuine narrow scene probe freshly reports inner `-246`, parent `-254` for a `-500` event. Passive indicators required the correction above. Editor drag/selection, ordinary focus reveal, nested keyboard reveal and virtualized identity tests passed in the fresh broader suites. Native wheel phases/momentum remain unverified. |
| Focus corners | Shared radii 10/8/6/5 match 2/2/1 insets. Fresh GPU tests confirm uniform integrated straight-edge width, transparent interior, AA corners and parent outline above opaque descendants at 1x/2x; theme/high-contrast geometry tests pass. The actual 2x Disable comparison shows more consistent corner thickness. This is not a claim of pixel-identical reflected tessellation samples or native display certification. |

## Pixel inspection and provenance

The original human turn 342 was read. One original attachment was reopened; metadata-only responses were not treated as original screenshot inspection. Early `cognition_image_view` responses likewise supplied only metadata; later runtime-attached actual pixels were used for the visual observations above.

Actual pixels inspected include the checkbox/focus/table/tree comparison crops and the freshly corrected `captures/editors-dark-narrow-1x.png`. The dark narrow view keeps headers/body on the same clipped horizontal region and shows a passive horizontal thumb at the visible lower edge; some controls/columns remain intentionally clipped. New crops in `review/crops/` were generated from **freshly rebuilt** baseline and reviewed-source PNGs, not painted by hand. Checkbox crop is nearest-neighbor 8x enlargement; focus crop uses actual 2x pixels. Full narrow editor capture is 1x.

There are **13 fresh baseline PNG/JSON pairs and 13 fresh corrected pairs**, plus unscrolled and genuinely wheel-scrolled final narrow scene captures. Each is a real GPU viewport-composite readback using the gallery's normal Parley text engine, not a native-window screenshot. JSON omits content values. Existing public/sensitive/foreign-surface gates are unchanged. `verify-evidence.py` decodes every matrix PNG and checks matching JSON presence; it is not a replacement for a visual reviewer.

The original implementer manifests were first verified unchanged. `baseline-source.sha256` was independently compared with exact baseline Git blobs plus the disclosed synthetic helper/test fixtures; `after-source.sha256` was compared with `4ea2ac4` Git blobs. Original `artifact-sha256.txt` still verifies. Historical `candidate-source.sha256` and `after-source.sha256` describe **4ea2ac4**, not this later correction; use `review/reviewed-source.sha256` for the final candidate.

Fresh baseline was archived into ignored `target/reviewer-polish-baseline-src`, with only the disclosed gallery/test fixtures copied in. The capture script touches local package entry points before compiling so archived timestamps cannot reuse candidate artifacts. Baseline logs show actual compilation from that archive and **four expected test failures** (exit 101). The candidate was then explicitly invalidated/rebuilt from this worktree, restoring all five native gallery executable names. Both fresh matrix directories contain source/binary hashes. Historical implementer binaries were not retained as independent executable proof; this review relies on its own recompilations, source manifests, logs and captures.

A first final-scroll invocation used unrecognized `--wheel-probe` (the demo ignores unknown flags), so `scroll-final.*` is **unscrolled**, not residual-motion evidence. The corrected `--wheel-inner-end` invocation and `scroll-boundary-final.*` / `scroll-boundary-probe.log` are the valid motion probe.

## Exact fresh verification

From the candidate checkout:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
# Used before initial verification and again after baseline compilation:
for c in rndr_core rndr_paint rndr_ui rndr_render rndr_std rndr; do
  touch "crates/$c/src/lib.rs"
done
```

The final non-overlapping primary suite set is **790 fresh passing executions, zero failed/ignored**:

| Command | Passed | Receipt |
| --- | ---: | --- |
| `cargo test --offline --locked -p rndr_core --tests` | 415 | `core.log` |
| `cargo test --offline --locked -p rndr_paint --tests` | 64 | `paint.log` |
| `cargo test --offline --locked -p rndr_ui --tests` | 212 | `ui.log` |
| `cargo test --offline --locked -p rndr_render --features observe --lib` | 96 | `render.log` |
| `cargo test --offline --locked -p rndr_render --features observe --test hands_on_polish_pixels -- --nocapture` | 2 | `gpu.log` |
| `cargo test --offline --locked -p rndr_render --features observe --test polish_review_privacy` | 1 | `privacy.log` |

Targeted/indicator/editor runs overlap those totals and are **not** added again. Paint was expanded from implementer's library-only 27 to the full 64-test suite. The corrected UI suite adds three review tests to the former 209. This is a different, freshly verified set, not a repetition of the implementer's 749 claim.

Also passed:

```sh
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --lib -- -D warnings
cargo clippy --offline --locked -p rndr_paint --test scroll_indicator_review -p rndr_ui --test hands_on_polish -- -D warnings
cargo clippy --offline --locked -p rndr_render --features observe --test polish_review_privacy -p rndr_ui --test polish_review_editor -- -D warnings
cargo build --offline --locked -p rndr --features observe --example ui_polish \
  --example selection_gallery --example data_gallery --example data_editors_gallery --example markdown_gallery
# Fresh output directory required, not an overwrite:
scripts/capture-ui-polish.sh NEW_REVIEWED_CAPTURE_DIRECTORY
# Baseline uses the exact archive/fixture procedure in ../README.md, with:
RNDR_POLISH_MANIFEST="$PWD/target/reviewer-polish-baseline-src/Cargo.toml" \
  scripts/capture-ui-polish.sh NEW_BASELINE_CAPTURE_DIRECTORY
cargo test --offline --locked --manifest-path target/reviewer-polish-baseline-src/Cargo.toml \
  -p rndr_ui --test hands_on_polish  # expected exit 101 / four failures
# Rebuild candidate afterward before launching any shared executable.
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene scroll --dark --narrow \
  --wheel-inner-end --capture-prefix NEW_SCROLL_CAPTURE_PREFIX
python3 evidence/hands-on-polish/review/verify-evidence.py
shasum -a 256 -c evidence/hands-on-polish/review/artifact-sha256.txt
git diff --check
```

Redundant EOF blank lines in fresh logs were trimmed solely for Git whitespace checks; test output content/counts were preserved. Changed Rust files were rustfmt-checked; strict test/library clippy scope is explicit above. No claim of inherited repository-wide lint/doc cleanup. Capacity remained about **13 GiB free**; all compilation reused the requested shared target with jobs=1 and debug/incremental settings from `data-gallery-env.sh`. No cleanup or other work deletion occurred.

## Owner retest / remaining gaps

Use `docs/ui-polish.md` from this candidate and rebuild from **this** worktree before launching shared executables. In particular:

```sh
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene selection
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene editors --dark --narrow
"$CARGO_TARGET_DIR/debug/examples/ui_polish" --scene scroll --narrow
```

Retest on native devices: centered checkbox at display/text scales; disabled-row clicks and read-only keyboard access; Tab/Shift+Tab/Enter/Space focus and activation; narrow header/body scrolling; nested diagonal/boundary/reverse gestures and momentum; selection drag/copy in nested editors; virtualized filter/reorder/recycle identity; high-contrast and rounded borders at actual 1x/2x displays. Passive thumbs are **not draggable** and do not add accessibility nodes/tab stops.

No further in-scope defect is known from the inspected source, fresh edge tests and rendered evidence. This is a reviewed headless correction candidate for owner consideration, **not native acceptance or self-approval**. Real pointer/trackpad event phases, momentum/OS batching, native screen readers/accessibility, IME, Windows/Linux desktops and MSRV remain uncertified. The pairwise matrix is not an exhaustive theme × width × scale product. The table comparison's differing focus origin is a documented visual-evidence limitation. Final sender review/integration belongs to the owner.
