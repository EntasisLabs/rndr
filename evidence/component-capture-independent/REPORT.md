# Independent component-capture review and correction

## Binding and outcome

Reviewed candidate `060083b0269f333ed878819d693ed50414aa054d` against baseline
`318edfba9d609a1aba088d53aacfb8cf0c0d0840`, on the admitted branch
`worktree/rndr-component-targeted-headless-capture`. The commit containing this
report is the returned correction commit; its parent is the candidate above.
`source.sha256` binds all 18 reviewed source/test/manifest/doc/example files,
including the two newly affected renderer resource files. `evidence.sha256`
binds this report, logs, manifests and retained PNG/JSON witnesses. No recursive
self-hash is used. Original `evidence/component-capture/` is unchanged.

**Outcome:** two reproduced capture correctness defects corrected; no further
blocking defect found within this bounded source/doc and macOS headless review.
This is a reviewer correction deliverable, **not owner acceptance/integration**.
No main edit, merge, approval, push, publication, cleanup, credentials or spend.

## Reproductions and corrections

- **Stale pixels on Exporter reuse:** `SceneGpu::ensure_image` treats an existing
  numeric ID as already uploaded. Replacing CPU pixels under that ID, or using
  another World with that ID, left stale GPU pixels. New capture-only
  `Exporter::render_capture_frame` rebuilds its image cache from paint-time CPU
  images each call, while retaining the device/pipelines. Ordinary/native render
  paths and cache policies are unchanged.
- **Budget omission reported as success:** ordinary image loading can evict an
  earlier image while preparing later images. Capture now verifies that every
  image referenced by the composite Frame remains GPU-resident; otherwise it
  discards the complete result as `ImageUnavailable` / `image_unavailable`.
  Missing CPU/native-only/unfinished images are also denied. This is conservative
  for the entire viewport composite, not merely the requested target subtree.
- Retained prior `reproduction-corrected.log` records both original failures.
  **Fresh independent reproduction:** `fresh-inverse-reproduction.log` uses a
  controlled inverse patch of only the capture resource call back to ordinary
  `render_frame`. All fixture CPU images are available, so the old availability
  precheck would pass. Both image tests fail (stale channel 0; budget omission),
  while the two other independent probes pass. The preserved fixed file was
  restored byte-for-byte in a finally block; no Git reset was used.
- The recovered privacy probes initially did not compile (`remove_resource` is
  not a World API; match arms returned mixed types). Repaired those probes,
  preserved their diagnostic logs, and finished regressions. Strict all-target
  Clippy initially found six constant `chunks_exact(4)` calls in capture tests;
  converted only those in-scope assertions to `as_chunks::<4>()`. Final lint passes.

## Source/doc review and contract coverage

Read the complete capture API/tests, CLI/schema/output code, mounted example,
UI identity adapter/test, all candidate source/doc diffs, frame snapshot callback
and native caller/observation guards, plus affected image-cache/export routines.
Checked identity delegation into existing controls/surfaces/selection/data/
collections: no second registry, renderer dependency in UI, or native listener.

- Privacy: missing/Deny and any hidden/off-target sensitive marker deny all
  channels before update, after update/layout at paint, and after Render. Added
  missing-policy, Layout-stage and Render-stage regressions; pre-update denial
  leaves the update counter untouched. Metadata is geometry/numeric identity
  only; errors and CLI stderr omit sentinel application text.
- Coherence: one update/layout/paint, exact Frame and immutable World at snapshot;
  GPU readback before Render. Same-frame update key/color and bounds/pixels at
  scales 1/1.5/2 pass. Post-Render identity/geometry/hidden/accessibility/details/
  scroll/theme/scale/text-engine identity/epoch changes drop both channels.
  Cosmetic changes outside those guards are documented paint-time snapshots.
- Identity: full generation Entity, missing/stale parent, global ambiguity,
  direct-parent disambiguation, duplicate same-parent key, live remount pin,
  dead/recycled Entity and actual mounted Card/control identity tested.
- Geometry/pixels: outward-rounded crop, clipping/outside/empty/hidden, invalid
  geometry/scale and caps/cycles tested. **Viewport composite crop** is retained:
  blue overlapping sibling pixels remain visible but sibling metadata is excluded.
- Resources: replaced and cross-World images, unavailable/released CPU images,
  insufficient image budget, hidden/off-target foreign SurfaceSlot denial and
  new post-Render foreign surface all covered. No placeholder success.
- CLI: strict bounded scene/options, private/no-output errors, output dimensions
  matched to decoded PNGs, expected center color, create-new/no overwrite, metadata
  collision rollback, directory collision, missing parent, spaces/dots, final
  .png/.json symlinks and dangling symlinks all pass on this Unix host. Parent
  directories are trusted; hostile same-user races and crash-atomic two-file
  output are explicitly not promised. Windows ACL/reparse behavior is untested.
- Docs now state cache freshness, budget denial, ordinary downsampling policy,
  final-file symlink refusal and trusted-parent limits. Original evidence is kept
  separate rather than rewritten to claim the corrected source was tested then.

## Executed gates (fresh logs in this directory)

All Cargo commands used `--offline --locked`, only the existing shared
`../rndr-refined-cross-platform-ui-framework/target`, jobs=2, incremental=0,
dev/test debug=0, via `scripts/data-gallery-env.sh`. No new broad cache or cleanup.
`environment.log` records rustc 1.98.0 / aarch64-apple-darwin, refs, binary hash,
and ~1.6 GiB free. `gates.tsv` retains initial diagnostics and final exits.

| Command / evidence | Final result |
| --- | --- |
| `cargo test -p rndr_render --features observe` / `renderer-verified.log` | 105 passed across 5 suites, zero failed/ignored; includes **19 capture tests**, actual adapter/readback required, not skipped |
| `cargo test -p rndr_paint -p rndr_ui` / `affected.log` | 246 passed across 43 suites, zero failed/ignored |
| `cargo build -p rndr_render --features observe --bin rndr-capture` / `cli-build.log` | exit 0 |
| `python3 scripts/test-component-capture.py <shared-target>/debug/rndr-capture` / `cli-contract.log` | **34 subprocess checks**, real PNG decoding and expected pixel assertions |
| `cargo clippy -p rndr_render --features observe --all-targets -- -D warnings` / `clippy-final.log` | exit 0 |
| `cargo fmt --all -- --check` / `format-final.log`; `git diff --check` / `diff-check.log` | exit 0 |
| `cargo check -p rndr_render --no-default-features [--features observe]` / `no-observe.log`, `no-default.log` | both exit 0 |
| `cargo test -p rndr_paint -p rndr_ui -p rndr_render --features rndr_render/observe --doc` / `docs.log` | 4 passed, zero failed; additional doc reruns, not added to 351 primary Rust executions |
| `cargo run -p rndr --example component_capture --features observe -- <new-prefix>` / `mounted-example.log` | exit 0; scoped mounted Card capture 320x136 |

`sample-decoded.log`: retained mounted-card and CLI-sample PNGs independently
decoded with the stdlib PNG decoder, dimensions matched to metadata and nonempty
alpha; CLI sample 128x64 center color checked. These are rendered-pixel assertions,
not human visual/native presentation acceptance. Historical failed probe/compiler/
format/lint logs are intentionally retained and are not final success claims.

## Remaining gaps and owner boundary

No Windows or Linux execution/cross-compilation, MSRV 1.95 run, native window/
compositor presentation, physical input, IME, screen-reader/native accessibility,
DPI/font fallback or platform startup certification. This review does not close
Wayland overlays, native observation transport on Windows, foreign-view rendering,
remote app capture, markdown slots/theming or arbitrary custom-engine concurrency.
Image cache rebuilding trades repeated upload work for correctness; captures are
synchronous and have no cancellable wall-clock GPU deadline. Ordinary image-budget
mipmap/downsampling rules remain, so captures are not lossless source-image export.

Owner retains the final sealed review and integration. Existing authorization
was used only for this isolated correction commit; main remains the baseline.
