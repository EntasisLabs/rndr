# Independent refinement review and corrections

Review target: baseline `a21d716302bf58192729b69378ae8c8cefaa4190` → candidate `47ce454880c2ffcaef27fe0e132b86a7f4c562e0`. Separate reviewer, same isolated undertaking. No approval, integration or push. Original Council Specification and Review Guide are unchanged. Consumer guide and complete affected implementation/test diff were reviewed, not just the implementer's terminal receipt.

## Verdict

**Suitable for owner acceptance review after the corrections here**, as a bounded portable-solid library refinement. No remaining reproduced release-blocking defect in the reviewed scope. Not perfection, native OS parity, physical interaction or platform-accessibility certification.

### Findings and concrete corrections

1. **P2 — authored fit baseline lost before first layout.** `ModeHandle.fit()` saved group width/gap immediately, but `resolve_toolbars` only noticed later host changes when a previous adaptive write existed. A host composing `fit(); layout.width = Px(809); layout.gap = 9; update(); clear_fit(); update()` unexpectedly got `Fill` and the old gap. The failed independent reproduction is `repro-before-first-layout.log` (expected failure 101). Resolution now captures current authored geometry before its first write as well as after host takeovers. The regression fails on the candidate and passes with the fix. Existing pending-resize cancellation, detach, foreign-marker and later-takeover regressions still pass.
2. **P3 — consumer Code typography table drift.** Corrected `12/18` to implemented `13/20`; no typography change.
3. **P3 — image digest encoding ambiguity.** The implementation README's three image hashes identify viewer-rendered PNG encodings, not the original native capture files. Explicitly labelled those as rendition digests and pointed to the native-file manifest. Original 101 entries were verified before this edit; only that README's manifest entry was refreshed. No original capture, result, log or approved Council document was replaced.

Added an independent key-event regression: after wide→narrow→wide fitting, the inappropriate arrow does not move focus, the appropriate arrow moves focus, selection stays manual and panel visibility remains consistent. This exercises canonical core key events, **not physical OS keyboard input**.

## Independent source/contract assessment

- `LabelBox` is additive; no public legacy struct shape changed. Effective metrics are shared by hug measurement, shaped wrap/cache width and paint origin. Nonfinite/negative metrics fall back wholesale; editor, Field, Caret, TextSelection and Buffer remain on legacy geometry. New text-only dimensions do not become hit-target dimensions. Cache identity remains engine/face/style/span/width based; inset-y/min-height changes do not need to change glyph layouts. No editor/history implementation changed.
- Workspace composition is explicit/opt-in, width is caller-supplied; normal density does not silently become compact. Navigation primary labels are emphasis, metadata secondary, with existing accessible full-text composition, explicit-name precedence and non-color dirty/selection cues. Metadata ownership does not bypass explicit UiStyle slots or Layout ownership.
- Mode `fit/clear_fit` retains entity/panel identity, updates core keyboard orientation, and uses the existing toolbar final content-width proof and activation cancellation. Group/child width/gap/markers are released conservatively. Axis remains the latest orientation by documented contract. Invalid/stale handles are rejected. The pre-first-update baseline hole above was the correction needed.
- Challenged the two-device-pixel content reserve and dynamic chrome calculation: it is local to opted-in snug mode/action sizing, not a global magic margin. Inner dimensions include current Layout padding and effective LabelBox insets. The final allocation proof still checks actual allocated text width against unwrapped engine width; reserve cannot force a false row fit. Integer/fractional/device-scale/text-scale boundary tests remain active.
- Existing allocation-test edits update actual label metrics and density-specific boundaries; they retain one-line/glyph-content/containment assertions. Added fractional boundaries broaden coverage. Snapshot references intentionally move to new deterministic passive artifacts; old snapshots remain unchanged. Passing regenerated goldens alone was not treated as evidence.
- Independent core/paint/UI suites exercise resolved contrast, reading pairs, focus-vs-selection, disabled/readonly denial, in-place names, editor/IME/history, FIFO activation, malformed lifecycle and authored style retention. No broad cleanup or unrelated architecture change was introduced.

## Actual visual review

These paths were opened directly through `cognition_image_view`, with actual image pixels (not just a semantic tree or successful image receipt). Paths below are repository-relative. All original paths start with `evidence/refinement-1/`:

| Exact PNG suffix | Independent visual critique |
|---|---|
| `before-light/frame-1.png` | Rows/labels spend too much height on internal field spacing; the source action reads as a full-width field. This is a comparison baseline, not the accepted visual disposition. |
| `after-light/frame-1.png` | Clearer two-column sidebar/document hierarchy. Command buttons and mode selector are compact and visibly actionable; source action is appropriately sized. Heading/reading/supporting typography establishes hierarchy without inflated rows. Significant lower document content enters the initial 700-point viewport. |
| `after-dark-preview/frame-1.png` | Quiet charcoal navigation and raised content are distinct without simulated glass. Preview prose is legible. Selected tabs retain their marker; inactive tabs sit on their host plane. |
| `after-compact/frame-1.png` | Denser rhythm, still the same readable type sizes. Navigation metadata remains visibly subordinate. Action targets remain separately recognizable. |
| `after-hc-light/frame-1.png` | Explicit dark borders keep controls distinguishable on white; the selected marker is not fill-only. More assertive outlines than standard are intentional. |
| `after-hc-dark/frame-1.png` | Black/white separation and strong document heading remain legible. The numerous HC outlines are heavy but coherent, not an accidental glass/material effect. |
| `after-states/frame-1.png` | Selected first navigation row retains its edge marker while the second has the separate focus ring. Unavailable action remains legible but subdued. Neither ring nor label pretends disabled is active. |
| `after-pressed/frame-1.png` | Held secondary action is visibly filled; focus on the second nav row is separately outlined. Disabled action and selected first nav remain independent. |
| `after-narrow-200-long/frame-1.png` | At 320×700 and 200% text the commands become a deliberately tall vertical stack. Full long labels wrap, with no page-wide horizontal escape. Content below the viewport requires vertical scrolling; this is not a claim everything fits in 700 points. |
| `after-narrow-200-editor/frame-1.png` | Focus scroll brings the editor region into view. Its fixed-height nested viewport clips/scrolls its own content rather than shrinking type. Nearby document prose reflows. |
| `after-narrow-200-source/frame-1.png` | At 320px dark HC compact/200%, the full long source action and focus treatment are visible after focus reveal. Partial content above/below is a scroll boundary, not replacement of essential actions with icons. |

Residual aesthetic tradeoff: enlarged narrow layouts are long, and HC has many explicit outlines. I do not find these a reason to hide actions behind a nonexistent menu or reduce type. The normal wide hierarchy is materially better than the baseline; this is still a functional gallery, not an independently user-tested application design.

Fresh rebuilt correction PNGs were also opened directly:

- `evidence/review-1/fresh-light/frame-1.png`
- `evidence/review-1/fresh-dark/frame-1.png`
- `evidence/review-1/fresh-stress/frame-1.png`

All three fresh native PNG files are byte-identical to the corresponding original after-light/dark-preview/narrow-source captures. Expected: the correctness fix changes authored takeovers, not default gallery composition. Their actual native-file digests are in `SHA256SUMS`, unlike viewer rendition digests. Each capture has fresh two-revision semantic/PNG pairs, process log and result metadata.

## Independent commands and results

Single existing target cache. Environment: `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. All Cargo commands were offline/locked. No clean, downloads or duplicate targets. Final observed free disk~2.7GiB.

```sh
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui
# candidate 539 passes; final corrected 541 passes, including doc-tests
cargo test --offline --locked -p rndr_ui --test refinement_release
# four release/key-event regressions pass (also included in final 541)
cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe
#71 passes
cargo test --offline --locked -p rndr_render --lib --no-default-features
#25 passes
PYTHONDONTWRITEBYTECODE=1 python 3 -m unittest discover -s tools -p 'test_observe*.py'
#20 passes
cargo fmt --all -- --check
#pass
cargo build --offline --locked -p rndr --example ui_gallery --no-default-features --features parley,observe
#pass
python 3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/review-1/fresh-light
python 3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/review-1/fresh-dark --dark --preview
python 3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/review-1/fresh-stress --dark --hc --compact --narrow --200 --long --preview --focus-source
#all exit 0; authenticated capture/freshness/stale and invalid-auth rejection/cleanup pass
git diff --check a 21d 716
#pass
```

Authoritative final logs are `tests-final-core-paint-ui.log`, `tests-render-parley.log`, `tests-render-builtin.log`, `tests-python.log`, build/capture/fmt logs. Interim tests 539/540 logs are intentionally retained and labelled. `tests-release.log` predates a test-name-only rename; the final 541 log is authoritative for final names/counts. Failed reproduction is expected, not a failing final suite. Do not sum repeated runs as unique coverage.

## Reproducibility and limits

`audit.json` independently records 101 original evidence hashes verified, original candidate runtime sources matching the captured binary's source manifest, unchanged Council docs, final independent counts and fresh-file equality. Candidate-to-corrected runtime changes consist solely of `crates/rndr_ui/src/controls.rs`.

Corrected binary SHA256: `333d0ceecd2b8e7248e35bd36addd1d4efd76fc4f4b04c376bc792d99fe9a162`.
Corrected controls source SHA256: `c94a984789a4b25cffa2805a55f7e735dde946fecbcf18e6394967e57d8af691`.
`build-provenance.json` includes runtime source/example/Cargo hashes, exact build/environment and candidate/base IDs. All three new capture results match this binary. Tests/docs were finalized subsequently without changing compiled runtime sources. Original after captures were made with binary `0197a1d060edb3df70cadaa4972c36723d633677b1bb71265ee76980ff9f6f1f`; original before-light records `0a9f23ee96d7478ced613d7bba04ce896181f3fb47e8e345b851962224ec3fb1`. The original before build log/result are historical evidence, not an independently rebuilt baseline here.

These are Metal/Apple M4 Parley application-surface readbacks **before present**, device-scale 1, not compositor/window screenshots (`presented_confirmation:false`). Application 200% text is not a high-DPI monitor. Headless fractional/device-scale tests do not certify actual high-DPI hardware. `--states`/`--pressed` use synthetic internal pointer state; `--preview` uses canonical activation and focus fixtures use canonical focus. No physical mouse/keyboard/OS resize/scroll or real IME session was performed. Automated text/editor/source history invariants passed; live VoiceOver, Windows Narrator/UIA, Windows GPU/native settings/materials remain unverified/unavailable, not silently passed. Custom colors and explicit geometry remain author responsibility.

Owner retains final sender acceptance and native sealed review/integration. No executor can approve this undertaking by writing this verdict.

Test logs retain command output with trailing blank EOF lines normalized solely for repository whitespace checks.
