# Refinement 1 — content-first desktop framework candidate

Baseline: accepted local main `a21d716302bf58192729b69378ae8c8cefaa4190`. Work is confined to the new isolated undertaking. This is a substantive reusable implementation and a finite visual refinement pass, **not approval/integration/push**. The owner retains independent review and integration. No new user testing, runtime repair or image gate is requested.

The Council Specification and Review Guide were read in full and remain byte-for-byte unchanged (SHA256 `c4e6d10b0b44073631555ea663c4700df980ffd6ca8f7361c4d27d0c8322601b` / `8dc077adecc90ff0a8a7ad7284b7350744d45fe076370371038d0905312e35b0`). The historical approval header was not rewritten. A2 contracts, accepted visual-polish evidence and implementation were inspected, not replaced with another research/design-only exercise.

## Largest visible defects and implementation

1. **Repeated field padding was masquerading as UI rhythm.** Every small display label inherited legacy 12/8 text insets and a 36-point minimum before the containing control/surface added its own chrome. This bloated labels, navigation metadata and document groups. New additive core `LabelBox` metrics travel through existing measurement, final shaping/cache constraints and paint origins. UI display text now delegates rhythm to its parent; legacy labels and all editor/selection fields retain the previous contract. No legacy public struct was expanded.
2. **Document text lacked a sufficient hierarchy above controls.** Reading is now 16/24 and section headings 20/28; command labels remain 14/20, supporting metadata 12/16, title 26/34. Density does not shrink text. Controls use explicit modest text insets and comfortable/compact face padding without reducing minimum targets or focus geometry.
3. **Metadata competed with navigation titles.** NavigationRow now owns a separate smaller Secondary line, with a visible Unsaved cue and the existing folded root accessible name. Selection and focus remain independent. Metadata nodes participate in validation and destruction.
4. **Application-specific breakpoint math was the only way to compose a useful workspace and selector.** Public `WorkspaceLayout` gives content-width/text-scale-aware sidebar/document metrics and atomic apply. `ModeHandle::fit/clear_fit` reuses actual toolbar fit measurement, updates the real arrow-key orientation, preserves identity/selection/editor state, and correctly releases widths/markers and caller overrides. The operating gallery now consumes those APIs instead of guessed fixed tab widths.
5. **Quiet controls painted the wrong plane and selected controls swallowed transient feedback.** Quiet controls/inactive tabs resolve their solid host background through the same ownership cascade as normal styles. Same-update theme changes and authored Color takeovers are covered. HC keeps explicit outlines. Hover/pressed feedback on selected items no longer erases their persistent selection cue.

The consumer guide is `docs/design/content-first-refinement.md`. This is framework/core behavior, not a Blooey patch, gallery-only palette or second renderer/focus tree. Source/IME/history, legacy struct/override contracts, opt-in observation and FIFO64 behavior are unchanged.

## Actual native visual loop and critique

All listed PNGs were opened directly with `cognition_image_view`, not judged from statistics or a tree. The images are **native Metal application-surface copies before present** through integrated authenticated observation. They are not compositor screenshots; `presented_confirmation` is false. Parley was rebuilt explicitly, and every final capture reports `text_engine: parley`, Apple M4 / Metal / quads, BGRA sRGB surface, 4 scene samples, device scale 1. Application text scale is separate from device scale.

### Matched light comparison: 1000 × 700, comfortable, 100% text

- **Before:** `before-light/frame-1.png` (`9f2d6aa83d007c689e9ffd40911c0fb5571c51f543e826846f3f9b9a62322379`). Fresh baseline run; it is also byte-identical to the accepted prior light capture. Build log and exact binary hash/flags are in `build-baseline.log` / `before-light/result.json`.
- **Intermediate:** `iteration-1-light/frame-1.png` (`94b9052b5de0f4d3bee3b60a75fbd23f2edb45fcf0afb59a5cc71c4388f5ac33`). This is explicitly a failed intermediate, not the final result. The tighter text boxes exposed fractional native-fit loss: actions and modes stacked unnecessarily even in a wide viewport. It looked like a narrow form inside a desktop window and pushed useful content below the fold. This was corrected by deriving chrome from actual layouts/label metrics and reserving two device pixels for snug, marked content-width actions; the existing final content-box proof still rejects genuinely overflowing rows. Intermediate source is not a separately sealed candidate.
- **After:** `after-light/frame-1.png` (`0e4cee724226a0eb84f32be52241a1ee279e48dc4e656c3c454119924dc874da`). The compact action row, smaller Source/Preview affordances, two-level navigation, stronger document heading and normal-sized Show source button create a clear desktop hierarchy. Content cards and warnings enter the initial viewport instead of being dominated by label padding. The reading text is appreciably larger without oversized chrome. The remaining white space is on the document plane, not between every tiny control.

Final native executable SHA256: `0197a1d060edb3df70cadaa4972c36723d633677b1bb71265ee76980ff9f6f1f`. `build-provenance.json` records exact build command, baseline head, binary hash and source file hashes. `audit.json` confirms runtime sources still match that build and every final capture uses the same binary. Later test-only additions do not change the executable source. Each directory contains two fresh revisioned PNG/semantic pairs, the process log and exact result/provenance.

### Representative final images inspected

Every path below ends with `/frame-1.png` (and has a corresponding frame-2).

| Directory | Viewport / settings | Pixel-review disposition |
|---|---|---|
| `after-light` | 1000×700, comfortable | Balanced sidebar/document planes; readable type hierarchy; actions look like actions, not fields. |
| `after-dark-preview` | 1000×700, dark, Preview | Calm solid layers and clear reading measure; inactive tab host plane has no conspicuous canvas-colored tile. |
| `after-compact` | 1000×700, compact | Tighter rhythm without shrinking text; metadata remains legible, command targets remain distinct. |
| `after-hc-light` | 1000×700, HC light | Explicit outlines and selection cue, no dependence on a subtle fill for required boundaries. |
| `after-hc-dark` | 1000×700, HC dark, compact, Preview | Black/white boundaries and hierarchy remain clear; controls do not dissolve into the content plane. |
| `after-states` | 1000×700, light | Hovered primary, disabled labelled Unavailable, selected first navigation row and independently focused second row. The ring does not imply selection. |
| `after-pressed` | 1000×700, dark | Held secondary press with visible independent focus, unavailable action still readable. |
| `after-narrow-200-long` | 320×700, 200% UI/reading, long labels | Full command text wraps in vertical targets; heading and supporting copy wrap without horizontal spill. The tall page scrolls rather than reducing text or hiding essential commands. |
| `after-narrow-200-editor` | 320×700, same, focus editor | Core focus reveal brings the source editor and lower document/card composition into view. The editor remains a fixed-height scrollable viewport, not a newly substituted text engine. |
| `after-narrow-200-source` | 320×700, dark HC compact, long, Preview, focus source | Show source is fully reachable/visible with its focus ring. Content wraps; the action is not truncated into an icon or hover-only affordance. |

I found no remaining obvious major visual defect in this representative final matrix. This is a bounded candidate disposition, not an assertion of perfection or Apple/Windows certification. At 700-point height the page and fixed-height nested/editor viewports can show partial content at a scroll boundary; that is intentional scroll clipping, not hidden/truncated essential actions. The narrow page is long, deliberately keeping all commands and full enlarged labels instead of inventing a partially accessible overflow menu.

## Auditable quality rubric

| Criterion | Pass condition for this increment | Disposition / evidence |
|---|---|---|
| Content first | Document hierarchy dominates; no nested field padding on display text; bounded reading composition | Pass: before/after images, LabelBox layout/shape/paint/cache regressions. |
| Consistent rhythm | Token spacing/corners; density changes space, not type | Pass: comfortable/compact images and tests. Parent surfaces own display rhythm. |
| Solid surfaces | Coherent neutral light/dark; quiet controls respect host plane; no claimed native material | Pass for standard solid recipes: native images, ownership/cascade regressions. Custom colors remain authored, not automatically certified. |
| State clarity | Hover/pressed/focus/selection/disabled remain distinct; non-color selection/dirty cues; semantic names retained | Pass: native fixture images plus actual core interaction/state/contrast tests. |
| Responsive composition | Wide rows only when actual labels fit; narrow/200% text and long labels keep essential actions | Pass: native stress/focus captures, exact final glyph/content-box boundary sweeps at fractional/text/device scales. |
| Accessible basics | Existing semantic action path, named controls, target floors, visible focus, documented contrast pairs | Pass at core/paint/test level; platform AT remains unverified. |
| Ownership and ergonomics | Additive public API, stable identities, safe destroy/release, authored overrides preserved, no new engine | Pass: new fit-release/pending-resize/marker tests plus full existing suites. |
| Editing/source safety | Existing editor/selection/UndoRedo/IME/source contracts preserved, FIFO64 untouched | Pass in automated affected suites; no native OS IME certification claimed. |
| Platform intent | Portable solids and explicit settings; existing platform keyboard conventions, not arbitrary HIG/Fluent skin mixing | Implemented portable path here. Windows-native and OS preference/material/AT cells explicitly open below. |

## Verification and exact commands

Final receipts: **539 core/paint/UI tests**, **71 renderer tests/doc-tests with Parley+observe**, **25 builtin renderer library tests**, **20 Python observation tests**, all passing. Counts include Cargo's successful doc-test cases where present. Eight focused new tests cover display metric release/cache/paint, legacy field/editor guards, workspace atomicity/scaling, adaptive modes and ownership, same-update host/caller color overrides, navigation metadata and destruction, pending fit resize/cancellation, and pre-existing markers. Existing boundary tests now use the actual effective LabelBox oracle and cover both refined density thresholds; historical fractional boundary stimuli were retained and the new tight boundaries added.

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe
cargo test --offline --locked -p rndr_render --lib --no-default-features
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p 'test_observe*.py'
cargo fmt --all -- --check
git diff --check
cargo build --offline --locked -p rndr --example ui_gallery --no-default-features --features parley,observe
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-refinement-fresh-light
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-refinement-fresh-stress --dark --hc --compact --narrow --200 --long --preview --focus-source
```

Other capture flags are recorded individually in each `result.json` and summarized in `audit.json`. All ten sessions passed authenticated capture, fresh/stale revision handling, invalid-auth rejection, normal exit and descriptor cleanup. The audit checks the disabled native fixture and fully visible focused source action in the stress view. `--states` / `--pressed` feed **synthetic Pointer inputs inside the native gallery**, and `--preview` uses the normal activation path; they are not physical OS input certification. Passing headless tests are not used as a substitute for the inspected native pixels.

The new six passive gallery snapshots are under `passive-gallery/`; prior visual-polish snapshots remain unchanged. These are deterministic bitmap rendering regressions, not native visual evidence. Initial failing logs are retained and clearly named `*-initial.log`; the final `tests-core-paint-ui.log`, `tests-render-parley.log`, `tests-render-builtin.log`, `tests-python.log`, `fmt.log`, `diff-check.log` and `audit.json` are authoritative. The evidence set is bounded (~3.6 MiB before this text/manifest), with no repeated massive logs. One ordinary target directory was used, no downloads/clean/broad deletion/duplicate giant targets. Final observed free disk: ~2.6 GiB.

## Explicit remaining platform gaps / handoff

- Independent owner/reviewer acceptance and any corrections are still required; no integration/approval/push was performed by this executor.
- Captures prove native application-surface output, not compositor appearance, occlusion or physical OS presentation.
- Physical mouse/keyboard, continuous OS-driven resize/scroll responsiveness, VoiceOver/UIA, real IME sessions, native font fallback and actual high-DPI monitor behavior have not been certified. Automated core paths and deterministic native fixture states are distinguished from those checks.
- Windows remote is unavailable; Windows-native GPU/input/accessibility/theme-preference behavior is unverified. Portable code/tests were advanced here without stalling on it. Native Windows materials and system preference subscriptions are not implemented or claimed.
- Explicit UiTheme settings remain explicit. No automatic system theme, contrast, reduced-motion or material integration was added. No half-accessible menus/dialogs, parser, document mutation service or unrelated application changes were introduced.
