# RNDR visual polish — implementation and review evidence

## Scope and identity

Continues the existing `work-ee6e304371af15c82b5751ec30d79054` worktree from preserved candidate `a3d12351620978977082942e9b5f32501abf09f4`. No main-checkout edits, merge, push, publishing, deployment, secrets, or unrelated cleanup.

Authoritative image root:

```text
/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-evidence-driven-visual-polish
```

`source-commit.txt` records the exact final implementation commit. The only later commit is this evidence bundle. All original `evidence/visual-polish/` files remain unchanged.

## Actual source changes

- `crates/rndr_ui/examples/support/operating.rs`: distinct navigation and document surfaces; desktop sidebar, stacked workspace below the scale-aware breakpoint; title/subtitle hierarchy; compact content-sized mode controls that become vertical at constrained widths, including matching keyboard orientation; shorter editor (76 logical points at 100%, 152 at 200%, capped at 180); bounded source toolbar; preserved entity identities, content, selection/history, and host-intent validation. Uses the core editor's own offset, not a nested generic Scroll. The editor now explicitly participates in canonical Focusable reveal.
- `crates/rndr_ui/src/controls.rs`: reusable SourceAction is a bordered Secondary button with Emphasis typography, rather than a Quiet/secondary-text field-like action. Idle navigation controls use the Navigation plane instead of Canvas.
- `crates/rndr_ui/src/style.rs`: reusable editor inset reduced from 12 to 8 points; buffer/input/history ownership remains core's.
- `crates/rndr/examples/ui_gallery.rs`: coherent window title; optional `--focus-source` and `--focus-editor` synthetic review views use canonical focus reveal, with no automation endpoint.
- `crates/rndr_ui/tests/a2_operating.rs`: remount preservation regression follows the new document parent, retaining the same identity/history assertions.
- `crates/rndr_ui/tests/gallery_polish.rs`: resize matrix at 280/320/760/1000/1600 points, comfortable/compact density, 100%/200% type; horizontal control/label bounds, semantic title role, compact source affordance, target minimums, identity/content preservation, responsive keyboard orientation/manual tab activation, preview bounds, editor/source focus reveal.

Local implementation commits:

1. `99aff58` — workspace hierarchy and reusable affordances.
2. `7bec03f` — native synthetic focus views and window title.
3. `82ddbdc` — lower-content focus reveal and shorter headline.
4. `88bbf3a` — screenshot-driven enlarged-title correction.

## Before/after findings

The historical prior-candidate reference is `../visual-polish/after-light/frame-1.{png,json}`. It is preserved evidence, not a fresh rebuild of a3d1235. Final comparisons use `complete-light/frame-1.{png,json}` at the same 1000 x 700 viewport.

| Element | Prior reference | Final source |
| --- | --- | --- |
| Source / Preview tabs | 486 points wide each | 120 points each, with vertical reflow when required |
| Editor | 976 x 108 | 668 x 76, placed inside the document workspace |
| Show source | 944 points wide | 134 points wide, bordered command affordance |
| Navigation / document | full-width sequential fixture rows | separately grouped sidebar and document surfaces |

The important visual change is hierarchy and grouping, not a speculative palette replacement. Desktop source reveal is visibly a button; the editor no longer occupies an entire full-width slab. Narrow layout stacks rather than squeezing a sidebar. Large-type/long-label layouts intentionally require vertical scrolling rather than hiding actions.

Iteration found and corrected two concrete issues:

1. Canonically focusing the enlarged editor after revealing source could leave it offscreen. A new integration test reproduced this. The gallery editor lacked the Focusable marker used by ancestor reveal; it now has it, while buffer scrolling stays with Editor::offset.
2. The intermediate `verified-200/frame-1.png` visibly clipped the long word "Component" at the desktop Title size. Final source uses the Heading semantic role under the scale-aware narrow breakpoint, retaining the user's 200% multiplier. The new regression asserts that responsive role.

## Final-source capture set

Use **only `complete-*` for final visual review**. Each directory contains two native surface PNGs, matching semantic metadata, `result.json` with exact argv/binary SHA256/provenance, and native process log.

| View | Relative PNG path | Flags beyond observe |
| --- | --- | --- |
| Desktop light | `evidence/visual-polish-2/complete-light/frame-1.png` | none |
| Desktop dark | `evidence/visual-polish-2/complete-dark/frame-1.png` | `--dark` |
| Narrow light | `evidence/visual-polish-2/complete-narrow/frame-1.png` | `--narrow` |
| Narrow 200% long-label stress | `evidence/visual-polish-2/complete-200/frame-1.png` | `--dark --hc --compact --narrow --200 --long` |
| Same stress, source revealed | `evidence/visual-polish-2/complete-200-source/frame-1.png` | above plus `--focus-source` |
| Narrow 200%, editor revealed | `evidence/visual-polish-2/complete-200-editor/frame-1.png` | `--narrow --200 --focus-editor` |

Desktop captures are 1000 x 700; narrow captures are 320 x 700. `frame-2.png` is the second independently observed revision in each directory. Native provenance must report Metal, Apple M4, and Parley. `audit.py` checks the six capture results, binary identity, focusable horizontal bounds, and actual focused source/editor visibility; its output is `audit.json`.

The directories `light`, `final-*`, `review-*`, and `verified-*` are retained intermediate iteration evidence, **not final-source sign-off**. In particular, `review-200-editor` demonstrates the reveal failure and `verified-200` demonstrates the title clipping before the final fix.

## Verification

Environment used to limit rebuild footprint:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
```

- `cargo test --offline --locked -p rndr_ui`: **47 passed**, including all prior suites and two new integration tests. `test-ui.log` is the final run.
- `cargo test --offline --locked -p rndr_core --test text_engine`: **12 passed**, `test-text.log`.
- `cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe native_startup`: **3 passed**, `test-startup.log` (unrelated tests filtered).
- Same startup checks with `--features observe` and no Parley: **3 passed**, `test-startup-bitmap.log`.
- `cargo build --offline --locked -p rndr --no-default-features --features parley,observe --example ui_gallery`: passed, `build-final.log`.
- `git diff --check`: passed before each implementation commit; final range checked again for handoff.
- Native capture command: `PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery OUTPUT [FLAGS]`.
- Final geometry/provenance audit: `python3 evidence/visual-polish-2/audit.py`.
- Evidence integrity: `manifest.sha256` covers this bundle excluding itself. Original manifest is independently rechecked, not rewritten.

## Honest coverage and residual limitations

These PNGs are real native-rendered surface copies, **not an assertion of OS-visible/compositor-presented screenshots** (`presented_confirmation: false`). Harness startup `surface_unavailable` denials are transient pre-surface probes; the successful captures follow them. Focus-view flags establish synthetic canonical focus, not physical OS key delivery. Native keyboard/pointer, wheel scrolling, IME, selection/Undo/Redo, app switching, and responsiveness still merit manual OS exercise. The long-label 200% first viewport cannot show all controls at once; the separate lower-content views and regressions cover reachability rather than shrinking text to make everything fit.

No whole-workspace regression suite or performance benchmark is claimed. No integration has been performed. Sender retains independent source/image review, project acceptance, and native integration.
