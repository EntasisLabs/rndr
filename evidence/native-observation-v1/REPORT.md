# Native observation v1 — implementation/evidence report

## Outcome and authority boundary

Implemented a reusable, opt-in, authenticated, read-only native observation API and Unix transport, gallery opt-in, Python standard-library client, regression tests and documentation. Actual live synthetic gallery capture succeeded locally without desktop automation, manual screenshots, OS permission prompts/new grants, private-app launches, model/provider dependencies, publishing or integration. This is an implementation candidate for sender review, **not independent approval, full A2 acceptance, OS-accessibility certification, or merge**. Existing A0/A1/A2 code/evidence and remaining gate records are preserved.

### Exact identities

- Starting baseline: `ff397cd9ef10678194068107043fcdbb3803d52a` (prior evidence/doc reconciliation).
- Observation implementation: `04a64a72fa75951c136d12534043b7fb6c452502`.
- Final tested implementation/hardening: **`f6648f16a7bcc75194bbc05ddabae3fd1527b0ec`** (parent `04a64a72…`). Adds explicit revocation, supplemental-semantic binding, timeout-configuration fail-closed behavior and actual active text-engine provenance.
- This directory is sealed in an **evidence-only child** of the final tested implementation. That child's exact ID is returned in the handoff; its tree must not change implementation/source inputs. The report cannot contain its own containing commit hash without self-reference.
- Native executable (not committed): `target/debug/examples/ui_gallery`, SHA-256 **`b96be3081726d73afb683466fa10e02555370c3ebb159311e8fa900c025c9263`**. Both final native runs record this hash independently.
- `implementation.diff` is the complete baseline-to-final implementation diff. `source-git-identities.log` records Git blob identities; `artifact-integrity.json` records all **13 changed source/document/client inputs** with SHA-256 and Git blobs, and PNG/raw-RGBA/JSON hashes. `MANIFEST.sha256` covers the evidence payloads (excluding itself).

## Exact implementation files/diffs

| Path | Scoped change |
|---|---|
| `Cargo.lock` | Two dependency-list entries for already-locked optional `getrandom 0.3.4` / `serde_json`; no package version churn. |
| `crates/rndr/Cargo.toml` | Additive, nondefault `observe` facade feature. Existing defaults unchanged. |
| `crates/rndr/examples/ui_gallery.rs` | Explicit `--observe` opt-in for audited synthetic fixtures, actual UiTheme descriptor, optional 15s normal-exit timer for bounded validation. No UI styling changes. |
| `crates/rndr_paint/src/lib.rs` | Reusable immutable `frame_with_snapshot` hook at the actual paint boundary. Ordinary `frame` delegates with an empty snapshot; update/layout/paint/Render schedule order unchanged. |
| `crates/rndr_render/Cargo.toml` | Nondefault `observe` feature and optional agent-neutral local serialization/CSPRNG dependencies. |
| `crates/rndr_render/src/lib.rs` | Bind a native request's semantic snapshot to the normal frame; validate after Render; arm only that surface's capture; complete only after native submission/present call. Negotiate COPY_SRC only for an explicitly opted-in app. |
| `crates/rndr_render/src/gpu.rs` | Copy the actual acquired surface texture after ordinary draw commands in BOTH quad and scene paths. Same encoder/device/configuration, padded bounded readback, RGBA/BGRA handling, timeout/format/extent checks and actual render provenance. |
| `crates/rndr_render/src/observe.rs` | Public session enable/shutdown, default-deny whole-frame classification, sensitive marker, dynamic theme label; private local capability transport and bounded request lifecycle; shared core accessibility tree/details/scroll snapshot; coherent response framing and fail-closed errors. |
| `crates/rndr_render/src/observe_tests.rs` | 18 security/coherence/transport regressions. |
| `docs/native-observation.md` | Usage, app API/privacy contract, exact wire protocol, limits, threat model, live-vs-headless guarantees and exclusions. |
| `tools/observe_client.py` | Importable `observe()` plus CLI; Python stdlib only. Bounded response validation; local descriptor checks; returns PNG + matching machine-readable tree; private output files/refuses overwrite. |
| `tools/test_observe_client.py` | 8 client framing/security/coherence/output regressions. |
| `tools/validate_observation_native.py` | Bounded synthetic-gallery-only native validation harness. Two fresh requests, stale/auth probes, normal shutdown cleanup, no desktop input. |

`crates/rndr_render/src/export.rs` was investigated and **not changed/used by observation**. It creates a separate headless device/render target, may choose another adapter/format/camera, and is not proof of a live/presented window. Neither the protocol nor client silently falls back to it.

## Security design and guarantees

1. **Two explicit opt-ins, default deny.** Feature disabled by default; app must create its own `ObservationSession` before native startup. Even then, content remains denied unless the application explicitly attests `ObservationContent::Public`. No other World/window inherits the session. No generic environment auto-enable flag, discovery service, global desktop/app inspection, TCP listener or unauthenticated observation.
2. **Scoped authorization.** Unix-domain socket in an atomically created, independently randomized 0700 directory; descriptor/socket 0600. Independent 96-bit locator plus 256-bit OS-CSPRNG bearer capability. Entropy/setup failure aborts. Capability never printed, echoed in errors or included in frame metadata. Invalid authorization never queues/wakes app observation. Same-user/root attackers able to read the private descriptor are outside this account-based boundary; this is not a same-UID sandbox.
3. **Read-only surface.** Only one request verb, a fresh frame after a revision lower bound. No input/interaction/action-dispatch endpoint, execution, commands, filesystem-reading endpoint, arbitrary app target or provider/model dependency. Supported actions are semantic descriptors from shared framework infrastructure only. A request wakes an ordinary app redraw/update; it does not inject user interaction.
4. **Sensitive frames expose neither channel.** Missing/Deny classification or ANY `ObservationSensitive` marker (including hidden/unsemantic entities) denies the entire image AND tree. Whole-frame denial avoids unsafe partial pixel masking. Both pre- and post-Render policy must pass; newly sensitive content is rejected before readback. **Public is an application audit assertion, not automatic secret detection.** Arbitrary unmarked private text/custom drawing cannot be inferred reliably; leave Deny for unaudited/private/mixed content. Only synthetic fixtures were authorized/attested here. Foreign SurfaceSlot content is denied.
5. **Bounded resources/lifetime.** One serial worker, bounded pending channel, at least 100ms between serviced connections; 128-byte request line, 500ms total read, 2s app deadline, 500ms GPU poll + 50ms callback receipt, 500ms absolute socket-write deadline. Max 4096/side AND 4,000,000 pixels, bounding padded GPU rows as well as RGBA/PNG; 8192 nodes, 1MiB metadata. Slow/truncated/oversized/expired/stale/unsupported requests fail closed. Socket timeout-configuration failure does not proceed with an unbounded read.
6. **Coherent binding.** Semantics are captured from the same immutable world used to paint, before Render systems. Post-Render changes in core tree, supplemental descriptions/actions/relations, scroll, content classification, theme or scale deny the request. Native surface extents must match metadata; image dimensions/buffer size are rechecked. No historical image cache. Active text engine is identified from the actual runtime type (`builtin_atlas`, `parley`, `custom`), separately from compiled `parley_feature`; custom internals are not inferred.
7. **Shutdown/revocation.** Explicit idempotent `shutdown()` revokes pending capture and stops the worker; the resource stays disabled. World/session drop performs the same bounded cleanup. Only the exact known files are removed, nonrecursively. Forced termination/crash cannot execute Drop and may leave a private stale descriptor/socket with no live listener; normal shutdown cleanup is tested. No unsafe code introduced.

The trusted boundary is the opt-in app and its public-frame classifier. The framework cannot certify a dishonest app's Public/theme assertion or unknown custom rendering internals. It does fail closed for all marked/denied/unsupported states; it never claims automatic redaction of unmarked secrets.

## Real/native versus headless verification matrix

| Contract/path | Evidence in this pass | Scope / remaining gap |
|---|---|---|
| Actual live native app-surface pixels + matched tree, light | `final-native-light/frame-{1,2}.{png,json}`, `result.json` | **PASS:** 1000×700, device scale 1, 28 nodes, revisions 3→4, same actual native executable. |
| Actual live native app-surface pixels + matched tree, narrow/dark/200% text/long synthetic fixture | `final-native-narrow/frame-{1,2}.{png,json}`, `result.json` | **PASS:** 320×700, device scale 1, 28 nodes, revisions 4→5. `--200` is text scale, not physical DPI. |
| Native render provenance | Both final run records and frame JSON | Metal / Apple M4 / Bgra8UnormSrgb / AutoVsync / opaque clear / **quads**, active builtin-atlas, parley feature false. Scene sample configuration is 4 but this run's quad path is not claimed to be scene/MSAA coverage. |
| Auth, future/stale lower bound, normal shutdown | Both final run records | **PASS:** bad capability denied with no PNG/tree; future revision rejected; app exits 0 after normal 15s timer; session directory gone. One unavailable startup surface per run was denied, followed by a fresh retry—not a cached/export fallback. |
| PNG integrity / dimensions / tree references | `artifact-check.log`, `artifact-integrity.json` | **PASS:** four PNGs fully decompressed/unfiltered and CRC-checked; dimensions match metadata; all child/root IDs valid; action descriptors bounded. Raw/color hashes retained. No visual-correctness or OS-presentation verdict inferred. |
| Default deny, sensitivity, coherent semantics, resize/scale binding, limits, expiry, slow clients, revocation | 18 Rust tests in `final-suites.log` | **PASS at automated App/protocol level.** Synthetic tiny test images are explicitly **not native screenshot evidence**. |
| Existing core/paint/UI/headless contracts | `final-suites.log` | **PASS:** full affected Cargo command. Includes existing conformance/paint/headless evidence tests; does not become native evidence. |
| Non-observe/default compatibility | `final-disabled.log`, `final-default-disabled.log`, `final-default-observe.log` | **PASS:** disabled renderer unit path; default feature compilation with and without observation. Default feature values unchanged. Not all optional runtime paths executed. |
| Separate headless Exporter | Existing retained evidence/tests; source investigated | Not used for live observation. Existing headless guarantees remain headless only. |
| Native display-list scene capture | Both GPU paths instrumented; source/compile checked | **Not exercised live by this gallery (quads only).** Needs independent follow-on runtime coverage. No claim of scene-path/native exhaustiveness. |
| Native Parley/custom engine, fonts/shaping/IME | Feature compile checked, metadata handles runtime kind honestly | **Not runtime-certified here.** Existing font/shaping/IME gaps remain open. No claim of automatic custom-font/configuration inference. |
| Native resize, scrolling/focus interaction, physical DPI/multi-monitor | Automated resize/scroll/focus/scale binding regressions only | No desktop automation/manual interaction used. Real interactive/native transitions and physical-DPI behavior remain unverified. |
| Actual compositor-presented proof / OS chrome, menus/dialogs, other windows/apps, cursor, ICC/HDR | Not available/in scope | Explicit **`presented_confirmation:false`**. Source is `live-surface-copy-before-present`: actual app render-target contents submitted for presentation, not proof that the OS displayed them or the user saw them. External SurfaceSlots denied. |
| OS accessibility integration / Linux / Windows | Core semantics reused; default accesskit compiles | No OS accessibility/native cross-platform certification. Unix transport implemented; Windows returns Unsupported, no network fallback. Existing native/A2 gates remain open. |

Pixel diagnostics are retained rather than interpreted aesthetically: light frames have 10 unique RGBA values; narrow/dark frames have 2. Repeated raw images within each run match exactly. These are observed render-target contents, not a legibility/typography/styling acceptance. No visual polish was performed or inferred from this pass.

## Automated test results and exact commands

Final source: `f6648f16…`. Environment: aarch64-apple-darwin, rustc 1.98.0 (`88d9e12ae…`), Cargo 1.98.0; full versions/UTC timestamps/disk state in `final-environment.log` / `final-after.log`.

`verify.sh` preserves the executed command sequence; its distributed copy adds a post-seal guard refusing to overwrite this committed evidence/manifest. To rerun, use the recorded commands with fresh output paths. `final-commands.log` records every executed command and exit status. `check_artifacts.py` pins the tested implementation from `final-environment.log`, so it verifies the evidence-only child as well as the pre-seal state and checks all historical evidence blobs remain unchanged. These post-seal evidence-tooling guards do not change any implementation/test/native inputs. All Rust work used:

```sh
CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
```

All final Cargo commands used `--offline --locked`; no dependency fetch/push/deploy/spend. The driver checked disk and refused another native build below 1GiB free. Initial disk free was ~2.3GiB; final ~1.3GiB; target grew from ~1.3GiB to ~2.4GiB. No unrelated/principal artifacts were deleted to make room.

Final commands (all exit 0):

```sh
rustfmt --edition 2021 --config skip_children=true --check crates/rndr_render/src/observe.rs crates/rndr_render/src/observe_tests.rs crates/rndr_render/src/gpu.rs crates/rndr_render/src/lib.rs crates/rndr_paint/src/lib.rs crates/rndr/examples/ui_gallery.rs
python3 -m unittest discover -s tools -p test_observe_client.py -v
cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe
cargo test --offline --locked -p rndr_render --no-default-features --lib
cargo check --offline --locked -p rndr --example ui_gallery
cargo check --offline --locked -p rndr --features observe --example ui_gallery
cargo build --offline --locked -v -p rndr --no-default-features --features observe --example ui_gallery
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1/final-native-light
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1/final-native-narrow --narrow --dark --200 --long
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1/check_artifacts.py
```

- **572 passed / 0 failed / 0 ignored** across 80 Cargo result groups in the affected-suite command (including doc-tests). This includes all **18 new Rust observer regressions**; it is not 572 newly added tests.
- **8 Python client tests passed** separately.
- **22 disabled-path renderer unit tests passed** separately; these overlap existing tests in the larger run, not 22 additional unique regressions.
- Both default-feature compilation checks passed.
- **Two bounded native runs / four final native PNG+JSON observations** passed, with native auth/stale probes and graceful cleanup.
- Changed renderer/facade/gallery compilation is recorded with actual `rustc` invocations in verbose build logs. Exact code/blob/source hashes and executable hash are retained. Unchanged dependencies and some unchanged suite binaries reused Cargo cache; this is **not a claim of independent full-source rebuild of every dependency/UI package**. Prior baseline source-rebuild evidence/cache caveats remain untouched for sender review.

New Rust coverage: default disabled/exact auth; missing/Deny policy; hidden/unsemantic sensitivity; stale/future/expired requests; invalid/oversized/extreme-aspect viewports; roles/names/bounds/actions/states/focus/scroll; revision/dimension/scale PNG binding; surface/readback/size failures; snapshot-before-Render; post-Render sensitivity/scale/theme/scroll divergence; foreign surfaces/node cap; metadata cap; supplemental description/action divergence; explicit revocation; Render-stall expiry; real local permissions/auth/response/cleanup; slow/truncated/oversized requests; absolute slow-reader/pending-shutdown deadlines.

## Chronology / diagnostics retained honestly

- Initial compiler/test diagnostics (entropy error conversion, moved theme ownership, malformed newline acceptance, incomplete button-name fixture) are retained in pilot logs. The final candidate fixes them; final results above are the acceptance evidence for this implementation, not pilot logs.
- First native pilot (`native-minimal/`) denied its unavailable startup surface. The initial harness terminated it rather than retrying, leaving a private stale session directory. The harness was corrected to retry only fresh requests and await normal timed shutdown. Only that exact owned session's two files and parent were cleaned; see `cleanup-initial-session.log`. No capability values were retained. `native-minimal-retry/` succeeded normally.
- `checkpoint-04a64a72/` and `checkpoint-b5fb7a0a/` preserve earlier exact-source runs/diffs/identities. `b5fb7a0a…` was an unshared provisional hardening commit amended to final `f6648f16…` to report the actual active engine rather than infer it from a feature flag. **Only root `final-*` artifacts and the final identities describe the delivered candidate.**
- No real/native screenshot is described as headless, and no headless reconstruction is described as live/presented.

## Preservation and remaining review

Before the evidence-only seal, entire committed trees matched baseline:

- `crates/rndr_core`: `c799a2b11f7e958b4315b6de8421a2593dc1a1d0`
- `crates/rndr_ui` (including all existing UI tests/galleries/goldens): `3b8c4d9b1792234a837298ab8f88a1ebcd98c0fb`
- `docs/design`: `11a07dc72ad61c18fb77b1207c994e35d7867898`
- Historical `evidence`: `9b91384d1893552f2289983e4c6538f11c3b661e` (the seal adds only this new directory; historical paths must remain byte-identical).

No Blooey/Learn Anything/Medousa code, styling/typography, unrelated cleanup, publishing, push, deploy, spend or integration. Sender retains independent follow-on review/fixes and native integration. Outstanding full-A2/native/OS-accessibility/font/shaping/IME/physical-DPI gates remain open; this work does not rewrite their status. No implementation blocker remains for this read-only v1 within its documented platform/format/public-content bounds. Independent review and the unexercised runtime/platform matrix above remain necessary before broader acceptance/integration.
