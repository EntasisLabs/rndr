# Visual polish pass 1 — evidence and review handoff

**Status:** implementation/rendering candidate delivered for independent review. **Actual PNG inspection and aesthetic acceptance are BLOCKED, not passed.** The current admitted Coder and owner tools cannot turn a local PNG path into model-visible pixels. The principal is adding that tool; this is the concrete remaining input/tooling gap. No style interview is needed.

## Exact coordinates

- Baseline: `6e1f1d2630ce94f6fd7ee4b1088b584645b2865c`.
- Implementation, tests, docs and active passive goldens: `a299e3aa09c6200f3b52cbb6a91cf398acc9640f`.
- The following evidence-only commit is the final review candidate; obtain its exact HEAD with `git rev-parse HEAD`. It changes no implementation/build configuration from `a299e3a`.
- Branch: `worktree/rndr-evidence-driven-visual-polish`.
- Isolated worktree:
  `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-evidence-driven-visual-polish`

`provenance-summary.json` binds each source/build coordinate, binary SHA-256, capture command, image/metadata hashes, viewport, device scale and engine provenance. `manifest.sha256` hashes every evidence file except itself. Historical evidence elsewhere in the repository is unchanged.

## Open these images first

Absolute base directory:

`/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-evidence-driven-visual-polish/evidence/visual-polish`

| Purpose | PNG under the base directory | Matching metadata | Viewport / device scale |
| --- | --- | --- | --- |
| BEFORE — unmodified source | `before-light/frame-1.png` | `before-light/frame-1.json` | 1000×700 / 1 |
| AFTER — identical benchmark configuration | `after-light/frame-1.png` | `after-light/frame-1.json` | 1000×700 / 1 |
| AFTER — dark, supplemental | `after-dark/frame-1.png` | `after-dark/frame-1.json` | 1000×700 / 1 |
| AFTER — stress, supplemental | `after-stress/frame-1.png` | `after-stress/frame-1.json` | 320×700 / 1; app text 200% |

Every run also has a second pair, `frame-2.png`/`.json`, and its `result.json` plus `native-process.log`. The matching light BEFORE/AFTER were built with the **same** offline/locked `--no-default-features --features parley,observe` configuration and no appearance flags. Both are Metal/Apple M4, Bgra8UnormSrgb, four scene samples. Dark and stress do not have corresponding fresh baseline captures and must not be presented as matched comparisons.

These are **live render-surface copies before present**, not reconstructed headless scenes, not window-chrome captures and **not OS-visible screenshot proof**. Every run reports `presented_confirmation: false`. The native process did run and shut down normally. Its observation path passed stale/authentication rejection and descriptor cleanup. An initial `surface_unavailable` denial before the first rendered frame is expected and recorded.

The `engine-only-light` pair is preserved solely as a diagnostic intermediate: only early engine installation had been changed, **before** the stale-layout cache correction and before final styling. It is not the AFTER candidate, has no committed source coordinate, and must not be used for visual acceptance.

## Honest comparative findings

These findings are supported by actual live provenance, source and layout/render regressions, **not by looking at the PNGs**:

- Before: `builtin_atlas`, despite Parley being compiled. After: `parley` in the actual painted frame's provenance. This fixes a real startup ordering defect rather than simply changing a font name.
- An exact replacement-engine TextLayout regression exposed a second defect: epoch-zero engine replacement retained old atlas UVs/metrics. `regression-layout-before-fix.log` preserves its failure. Final renderer suites pass exact replacement-engine glyph/line/caret geometry comparison and explicit/custom-engine preservation.
- Gallery actions now opt into reusable content-sized toolbar rows and stacked Fill fallback; new tests check differing final widths, scale/density reflow, focus and activation. The prior independently reviewed toolbar pixel-boundary tests are unchanged and still pass.
- Editor ink/surface is now explicitly styled through the framework semantic recipe, and real painted glyph/contrast tests pass in all four palettes. Supporting text has semantic Secondary ink. These address the contrast risk without claiming that every native editing state has been inspected.
- Restrained palette, roundness, spacing/type hierarchy and neutral interaction states are **design intentions pending actual image critique**. Do not infer "looks good", "smoother", "no clipping" or "polished" from these receipts. The next reviewer should judge the pixels and request revisions.

## Build and capture commands actually used

All Cargo commands used:

```sh
export CARGO_BUILD_JOBS=2
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
export CARGO_INCREMENTAL=0
```

Baseline on the unmodified baseline source (output `build-before.log`):

```sh
cargo build --offline --locked -p rndr --no-default-features --features observe,parley --example ui_gallery
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/visual-polish/before-light
```

Final on implementation `a299e3a` (`build-after.log` and the three `after-*.log` files):

```sh
cargo build --offline --locked -p rndr --no-default-features --features parley,observe --example ui_gallery
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/visual-polish/after-light
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/visual-polish/after-dark --dark
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/visual-polish/after-stress --dark --hc --compact --narrow --200 --long
```

The harness adds `--observe --observe-exit-after-15s`; exact launch arrays and build hashes are in each `result.json`. Output directories must be new. No credentials, descriptor capabilities or persistent observation endpoints are committed.

- Baseline binary SHA-256: `728c18e2ae06e16650be805e5081fc84083903a1a4492b933cae9ffef6149de9`.
- Final binary SHA-256: `bf70f469000b76960277a313b75be3767ac8b44412219fc7c22d1a3fb40cd9ef`.

## Tests actually run

Counts below include integration and doc tests where present; configurations repeat shared tests and are **not** unique-test totals.

| Command | Final result | Evidence |
| --- | --- | --- |
| `cargo test --offline --locked -p rndr_ui` | 44 passed, 0 failed | `test-ui.log` |
| `cargo test --offline --locked -p rndr_core -p rndr_paint` | 483 passed, 0 failed | `test-core-paint.log` |
| `cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe` | 70 passed, 0 failed | `test-render-parley.log` |
| `cargo test --offline --locked -p rndr_render --no-default-features --features observe` | 53 passed, 0 failed | `test-render-builtin.log` |
| `python3 -m unittest discover -s tools -p 'test_observe*.py'` | 20 passed | `test-python.log` |
| `cargo run --offline --locked -p rndr_ui --example passive_gallery -- evidence/visual-polish/passive-gallery` | 6 new active fixtures generated; full golden tests pass | `generate-passive.log`, `passive-gallery/*.txt` |
| `git diff --check` | working-tree-only check; not committed-range proof | prior checks did not establish that baseline-to-candidate diff passed; `git diff --check 6e1f1d2 c235a715` failed for extra EOF blank lines in five logs, corrected in independent review |

`test-engine.log`/`build-engine.log` are intermediate investigation logs; final authoritative suites are the complete renderer logs above. No strict-lint or native accessibility certification is claimed. No dependency/lockfile changes were made.

Disk was checked before builds: 5.5 GiB free at actual start, later 4.4 GiB, then 2.6 GiB after complete affected builds/captures. Builds were bounded as above. No broad cleanup or deletion of other targets/scratch work was performed; only Python bytecode produced by this assignment was removed. No further native/gallery or additional-feature builds were started after the low-space update. A final UI test did recompile core/paint/UI in 3.88 seconds with the same bounded settings; the final log records that explicitly. Final free space was 2.7 GiB.

## Independent correction review

See `review/README.md` for independent findings, severity, resolutions, exact commands and pre-fix failures. This follow-up preserves TextFace before the first update, restores toolbar-owned widths on release, and uses weak engine identities instead of reusable raw addresses. The five original test logs were normalized only by removing extra EOF blank lines; test outcomes were not rewritten. All manifest entries are regenerated after normalization and review evidence addition.

The original test totals above and all PNG/build provenance still describe the original implementation source, not the follow-up corrections. Independent final-source runs cover the full UI suite (45 passed), core text engine tests (12 passed), and native startup tests in each of Parley/bitmap configurations (3 passed each). No new native window/capture run was made in review. New source does not inherit visual acceptance from older captures.

## Reviewer order and remaining manual gates

1. Verify `manifest.sha256` (`shasum -a 256 -c evidence/visual-polish/manifest.sha256` from repository root), compare implementation to baseline, and separate:
   - correctness: `rndr_core` engine ownership/cache identity and renderer startup/tests;
   - reusable design: `rndr_ui` tokens/styles/surfaces/controls;
   - benchmark adoption: shared operating-gallery sizing marker/editor recipe;
   - evidence/docs: separate new active goldens and captures; no rewritten historical evidence.
2. Re-run the two `native_startup` tests in Parley and no-Parley configurations. Check preservation of explicit BuiltinText/custom Parley, interior fallback configuration and TextFace. Inspect the exact TextLayout equality regression, not just engine provenance.
3. Inspect `desktop_foundations.rs` and the unchanged `a2_toolbar_content_fit.rs` boundaries, full UI state/contrast, nested scroll, editing, selection/history, hidden panel and accessibility-contract tests. Check that app-authored overrides and unmarked widths still win.
4. **Use the incoming local-image tool to open BEFORE and AFTER pixels**, not metadata. Check font coverage/rendering at 100%, title/body hierarchy, control density, alignment, focus ring/corner compositing, selection, editor legibility, scrolling/clipping and unused space. Inspect dark/stress separately. Request changes as needed; visual sign-off is still open.
5. Compare live-surface captures with an actual OS-visible window. Native keyboard/pointer/resize/scroll, editing selection/Undo/Redo, focus/app switching and responsiveness need manual exercise. Existing extra-layout-work performance caveat is not resolved by this pass.
6. VoiceOver/OS accessibility, IME, native multilingual font fallback, physical DPI and other platforms remain unverified. App `--200` is not a device-DPI test. For native typography use Parley; the former no-default/accesskit-only launch deliberately remains bitmap.
7. Coordinator owns independent review and native integration. No manual merge/integration, push, publishing or deployment occurred. Worker completion is not sender acceptance.
