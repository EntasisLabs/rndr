# Independent review and scoped corrections

## Scope and status

Reviewed candidate `c235a715ba7fa2986b5219b504bc3c0f43194dab` against baseline `6e1f1d2630ce94f6fd7ee4b1088b584645b2865c`, including all implementation/test changes, root/UI documentation and the visual evidence handoff. No AGENTS.md was found in the checkout or its immediate parent search. This is a correctness review/correction delivery, not visual approval, sender acceptance or integration. The correction commit can be identified with `git log -1 --format=%H -- evidence/visual-polish/review/README.md`.

## Findings and resolutions

| Severity | Finding | Resolution and evidence |
| --- | --- | --- |
| Medium | Content-sized toolbar actions retained their last measured/Fill width after ownership ended. Removing the marker or reparenting a borrowed control could silently replace an authored width. | Save baseline and last owned write. Restore on marker removal, departure from a toolbar or toolbar teardown on the next layout; preserve later caller takeover. Save caller changes made during active ownership for release. `toolbar-before.log` reproduces 136pt instead of authored 117pt. Final UI regression additionally covers hidden children, toolbar destruction, active-owner edits and release-time takeover. |
| Medium | A TextFace installed before the first update was bypassed by native Parley startup when no engine resource existed. This startup-order hole existed in the baseline too, but contradicted the candidate's preservation contract. | The no-engine installation branch now also checks TextFace. `text-face-before.log` reproduces the failure. All three native-startup tests pass in both Parley and no-Parley configurations, including exact replacement-engine TextLayout equality and existing explicit-engine/interior-fallback/TextFace preservation cases. |
| Medium, latent | Raw engine-address hashing was not durable allocation identity: dropping an explicit engine before a subsequent update permits address reuse, potentially retaining stale epoch-zero layout. No allocator-dependent stale-frame reproduction is claimed. | Each stored label layout now has a separate weak engine identity, checked by Arc identity, so a dead engine cannot match a replacement and its allocation address cannot be reused while referenced. The cache does not retain the engine/atlas. New core regression verifies the weak anchor, strong lifetime, and replacement layout; existing exact Parley replacement geometry test passes. |
| Low | Committed candidate-range diff check failed on extra EOF blank lines in five evidence logs; the prior unqualified diff-check statement was not supported by a clean working-tree check. | Remove only extra trailing blank lines from test-core-paint, test-engine, test-render-builtin, test-render-parley and test-ui logs, regenerate hashes and qualify the original claim. No recorded test result or image is altered. |

No additional substantive theme/override/contrast/focus-composition correctness defect was reproduced in this review. Existing full UI tests cover authored overrides across theme changes, detach, editor/secondary contrast across all four palettes, focus/selection, activation, exact toolbar fit boundaries, nested scroll, hidden panels and editor selection/history. Those tests are not a visual judgment of rounded corners, alpha seams or aesthetics.

## Independent commands and results

All Cargo invocations used `CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`, offline/locked dependencies, and existing target caches.

| Command | Result | Log |
| --- | --- | --- |
| `cargo test --offline --locked -p rndr_ui --test desktop_foundations content_width_releases` before production fix | expected failure, exit 101 | `toolbar-before.log` |
| `cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe native_startup_preserves_text_face_before` before production fix | expected failure, exit 101 | `text-face-before.log` |
| `cargo test --offline --locked -p rndr_ui` on final source | 45 passed, 0 failed | `ui.log` |
| `cargo test --offline --locked -p rndr_core --test text_engine` on final source | 12 passed, 0 failed | `core-text.log` |
| `cargo test --offline --locked -p rndr_render --no-default-features --features parley,observe native_startup` on final source | 3 passed, 0 failed; other tests filtered, not counted | `startup-parley.log` |
| `cargo test --offline --locked -p rndr_render --no-default-features --features observe native_startup` on final source | 3 passed, 0 failed; other tests filtered, not counted | `startup-builtin.log` |

Two initial mistakes in newly authored regression code (an Arc import and an unsupported remove_resource call) were corrected before these final runs; no production compile failure is concealed. Before-fix logs preserve behavioral assertion failures, not those test-authoring compile attempts. No full core/paint/renderer/Python rerun, strict-lint certification or new native capture/build is claimed by this reviewer.

Disk was checked first (4.8 GiB free) and repeatedly before further builds. It fluctuated down to 310 MiB and later recovered externally to 4.3 GiB. Work was restricted to the listed tests; no target cleanup, broad rebuild, native launch, dependency changes, push, deployment, publishing or integration was done. Only scoped source formatting was applied.

## Integrity and final gates

All 54 incoming manifest entries were independently validated before changing evidence. After normalization and adding this review, the manifest covers 61 files (everything below evidence/visual-polish except itself). Candidate-range check is `git diff --check 6e1f1d2630ce94f6fd7ee4b1088b584645b2865c HEAD`; this must be run after committing, not replaced by `git diff --check` on a clean tree. The final handoff reports the exact commit, manifest result and committed-range result.

Local image capability discovery returned no match. No actual PNG was opened, and no geometry/statistics proxy is used for aesthetic acceptance. Matched before-light/after-light plus supplemental after-dark/after-stress pixel inspection remains BLOCKED. Existing captures retain their original source/build provenance and are not fresh native evidence of these corrections.

Remaining gates: actual image critique; final-source native/OS-visible comparison and keyboard/pointer/focus/resize/scroll/editing/history/responsiveness checks; VoiceOver/OS accessibility, IME, native multilingual fallback, physical DPI and other platforms. The existing extra-text-layout-work performance caveat remains. The sender retains final review and has not accepted or integrated this work.
