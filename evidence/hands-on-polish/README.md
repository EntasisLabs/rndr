# Five-item hands-on polish — review evidence

Baseline: **8ecd94c42036986a204330223514e5909d700702**. Isolated undertaking: `work-f9ff0e9826eb12008144432185591a00`. This directory contains synthetic, publicly attested gallery data only. Original user screenshots are not copied into Git. Owner turn 342 was read and all five media attachments reopened; initial tool replies were metadata-only, so the before/fixed claims below rely on the fresh reproduced frames and tests, not on treating screenshots as diagnoses.

Implementation/behavior/manual guide: [docs/ui-polish.md](../../docs/ui-polish.md). No merge, approval, push, publish or deployment is part of this candidate.

## What was reproduced and visually checked

`rendered/` has **13 matched before/fixed PNG + same-frame JSON pairs**, plus a final nested-wheel probe. These are actual GPU offscreen viewport-composite readbacks with bundled Parley text shaping, not native-window screenshots. Captures include the normal scene composite; they are not isolated subtree repaints. JSON deliberately excludes text/value content. Original privacy/attestation/limits remain active.

For selection, data and editors, each side covers:

* light, normal 1000×960 logical points, 1x;
* dark, normal 1000×960, 2x;
* light, narrow 380×960, 2x;
* dark, narrow 380×960, 1x.

This is pairwise coverage, **not every theme × width × scale combination**. A separate focused Disable capture is light/normal/2x. The checkbox additionally has real-GPU tests for 18/27/36-point faces at 1x and 2x, plus light/dark/high-contrast corner-geometry tests.

Inspected comparison crops (before **left**, fixed **right**):

* `crops/checkbox-8x-before-left-after-right.png`: nearest-neighbor 8× enlargement, not native-scale evidence. The original text `x` is visibly baseline-low. Decoded white-ink centroid in the actual 18×18 face changes from **(8.2960, 10.8240)** to **(8.5, 8.5)**; face center is (8.5, 8.5). See `decoded-pixels.log`.
* `crops/focus-2x-before-left-after-right.png`: actual 2x-pixel crop of Disable. Former thin corner coverage becomes a smooth, concentric two-point ring. Shared logical radii are 10/8/6/5 for insets 2/2/1, rather than 10/6/6/4. Existing shader AA was investigated and left unchanged.
* `crops/tree-dark-2x-before-left-after-right.png`: focused collection before has a blue gutter/viewport plane behind normal row surfaces. Fixed view keeps its normal surface and uses an outline above children, without changing disabled access semantics.
* `crops/table-light-1x-before-left-after-right.png`: header/body text insets and declared alignment now agree. Compact density's additional three-point track-origin error is independently reproduced in the baseline test log.

Also inspected full fixed editor captures at light/normal/1x and dark/narrow/1x. Narrow table clipping is intentional: columns retain their widths, and the passive horizontal thumb remains on the visible clipped edge. A partially clipped control is not granted a larger hit region. The dedicated `final-scroll-dark-narrow-1x` capture uses a synthetic 500-point wheel event: **inner consumes 246, parent consumes 254**, rather than losing residual motion or routing everything outward (`scroll-probe.log`).

The GPU tests assert subpixel ink centroids, no filled outline interior, equal straight-edge inset coverage, antialiased corners, outline z-order above opaque descendants, and sensitive-marker capture denial. Individual reflected round-cap edge samples can differ under tessellation/MSAA (up to 71 byte levels here); the tests measure integrated ink and centroid rather than claiming byte-identical mirrored edge samples. Reported centroids are exactly centered in this run (`pixels.log`).

## Root causes / intended contracts

See the issue-by-issue table and nested-scroll contract in the guide. In particular:

* A disabled click exposing fallback **view focus** is not evidence that the disabled child inherited an ancestor's color. The reproduced bug was the view's own filled-focus recipe. View focus remains available; its ink is now an outline.
* Header/body track origins already agreed at comfortable density. The pass fixes the different content/chrome insets and missing header alignment as well as the distinct compact-density three-point track drift. It does not pretend every header boundary was always wrong.
* The innermost port still owns legitimate matching-axis movement. Only bounded residuals chain through actual hit ancestry. No new axis conversion, momentum, acceleration, drag policy or “outer always wins” rule was introduced. Passive overflow thumbs are not draggable controls or tab stops.
* The related oversized-focus jump is fixed without changing ordinary keyboard reveal. Existing focus/tab, editor selection, virtualization/recycling, theme, identity, accessibility and capture contracts remain covered by their suites.

## Exact primary validation (all passed)

Environment for commands below:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR='/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target'
source scripts/data-gallery-env.sh
export CARGO_BUILD_JOBS=1
```

| Command | Result | Receipt |
| --- | --- | --- |
| `cargo test --offline --locked -p rndr_core --tests` | 415 passed, 0 failed/ignored | `core-suite.log` |
| `cargo test --offline --locked -p rndr_paint --lib` | 27 passed, 0 failed/ignored | `paint-final.log` |
| `cargo test --offline --locked -p rndr_ui --tests` | 209 passed, 0 failed/ignored; includes nine new polish tests | `ui-suite.log` |
| `cargo test --offline --locked -p rndr_render --features observe --lib` | 96 passed, 0 failed/ignored, including targeted capture/privacy/image contracts | `renderer.log` |
| `cargo test --offline --locked -p rndr_render --features observe --test hands_on_polish_pixels -- --nocapture` | 2 passed, real GPU, 0 failed/ignored | `pixels.log` |

**749 passing test executions in the primary suite set.** Separately rerun targeted UI tests (nine) are in `targeted.log`; they are already included in the 209 and are not added again to that total. The earlier `core-paint.log` is an overlapping library-only run, not another 41 distinct tests.

Other passing checks:

```sh
cargo clippy --offline --locked -p rndr_core -p rndr_paint -p rndr_ui --lib -- -D warnings
cargo clippy --offline --locked -p rndr_ui --test hands_on_polish -- -D warnings
cargo clippy --offline --locked -p rndr_render --features observe --test hands_on_polish_pixels -- -D warnings
cargo build --offline --locked -p rndr --features observe --example ui_polish \
  --example selection_gallery --example data_gallery --example data_editors_gallery \
  --example markdown_gallery
python3 scripts/check-ui-polish-pixels.py evidence/hands-on-polish
git diff --check
```

Logs have only redundant EOF blank lines trimmed for Git whitespace checks. Receipts: `clippy*.log`, `gallery-builds.log`, `decoded-pixels.log`. Changed Rust files were rustfmt-checked; no unrelated cleanup was included. This is not a claim that all inherited repository-wide clippy/rustdoc warnings were resolved.

## Reproduce captures and genuine baseline failures

`after-source.sha256` and `after-binary.sha256` bind the final fixed capture source/binary. `baseline-source.sha256` and `baseline-binary.sha256` bind the baseline source and helper used for its captures. `artifact-sha256.txt` covers evidence files; `candidate-source.sha256` covers changed source/tests/scripts/docs. PNG comparisons are regenerated by the dependency-free Python script, never hand-edited.

From this candidate checkout, with the environment above:

```sh
# Requires a fresh output directory: capture files use create_new, not overwrite.
scripts/capture-ui-polish.sh target/polish-recapture-after

# Baseline source only, inside this checkout's ignored target/ directory.
# Compiled artifacts still use the SAME exported shared CARGO_TARGET_DIR.
base="$PWD/target/polish-recapture-baseline-src"
mkdir -p "$base"
git archive 8ecd94c42036986a204330223514e5909d700702 \
  Cargo.toml Cargo.lock rust-toolchain.toml crates examples tools docs | tar -xf - -C "$base"
cp evidence/hands-on-polish/baseline-fixtures/ui_polish.rs "$base/crates/rndr/examples/ui_polish.rs"
cp evidence/hands-on-polish/baseline-fixtures/hands_on_polish.rs "$base/crates/rndr_ui/tests/hands_on_polish.rs"
RNDR_POLISH_MANIFEST="$base/Cargo.toml" scripts/capture-ui-polish.sh target/polish-recapture-before
cargo test --offline --locked --manifest-path "$base/Cargo.toml" -p rndr_ui --test hands_on_polish
# Expected baseline exit 101: all four input/grid regressions fail.
# Compare before-regressions.log, which records those genuine failures.
```

The baseline runner contains only the original four input/grid regressions (no references to new primitives). Source mtimes from `git archive` can incorrectly reuse newer local package artifacts in this shared cache. The capture script deliberately touches **only local package entry points** before building; it never cleans dependency caches. A first cache-reused attempt was rejected rather than presented as baseline evidence. Final baseline logs show actual baseline package recompilation and four reproduced failures. After baseline work, rebuild from the candidate before launching native tests; the shared executable name may otherwise still refer to baseline code.

The capture script checks for at least 1 GiB free before building and keeps jobs at one. At dispatch only ~988 MiB was free; a later external capacity change made ~15 GiB available (~13 GiB at the final capacity check). This worker did **not** delete other work or clean the shared cache. Baseline source snapshots remain in the isolated checkout's ignored `target/`, not another artifact target directory.

## Open certification gaps

Native pointer/trackpad wheel phases, momentum, diagonal gestures and OS event batching are **not certified** by these headless tests. Neither are real macOS/Windows/Linux screen-reader sessions, native IME, platform desktop behavior or MSRV. The guide provides practical rebuild/relaunch commands and an issue-by-issue native checklist. Fresh headless screenshots and successful native gallery compilation are not a substitute for that testing. Owner independent review and native integration gates remain required.
