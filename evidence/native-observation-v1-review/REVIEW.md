# Independent review — native observation v1

## Verdict: CHANGES REQUESTED (v1 only)

**One medium-severity paint/provenance coherence defect should be corrected and independently re-reviewed before accepting this reusable v1.** Two low-severity client robustness issues are also reproduced below. No implementation was changed, approved, integrated, pushed or deployed. Sender review remains pending. This verdict neither accepts nor reopens unrelated A0/A1/A2 work, and is not full A2/project/native certification.

Reviewed exact candidate:

- Baseline `ff397cd9ef10678194068107043fcdbb3803d52a`.
- Implementation `04a64a72fa75951c136d12534043b7fb6c452502` and `f6648f16a7bcc75194bbc05ddabae3fd1527b0ec`.
- Evidence seal `1079de03f41609319d9d666be29168fb5006c424` (clean at entry).
- Original manifest: **111 payloads**, SHA256 `9034ec3ec28db8aa007ccd6faef65ba5139368e9fb0a549e5a4cbe7d2360b30a`: independently recomputed; every row matches.
- Entire `implementation.diff` equals the Git baseline-to-final diff byte-for-byte. All 13 changed implementation/config/document/tool inputs inspected, together with `docs/native-observation.md`, the full original report, tests, actual test/build logs, native results and PNG/tree payloads. Seal changes no source/config/docs/tools relative to final implementation.

## Prioritized findings

### F1 — P2 / medium / acceptance-blocking: text-engine state is not bound across the paint boundary; provenance can name an engine that did not paint the frame

**Evidence:** `crates/rndr_render/src/observe.rs:304-310` stores tree/details/scroll, not the paint-time text engine or its epoch. `observe.rs:384-388` validates semantics/scroll/scale/theme but does not validate the engine. The runner calls Render before atlas upload (`crates/rndr_render/src/lib.rs:645-668`), and then derives `text_engine` provenance from the **post-Render** World at `lib.rs:689-700`. `lib.rs:852-872` also selects Parley atlas uploads from that post-Render engine. The existing trait explicitly defines `cache_epoch()` as indicating that stored glyph UVs are no longer valid (`crates/rndr_core/src/text.rs:239-243`).

**Independent reproduction:** `run_rust_probes.py`, test `review_post_paint_text_engine_replacement_is_accepted` (results in `rust-probes.log`). Paint uses the builtin engine to shape a label (old engine's shape counter is asserted nonzero). A Render system replaces the World's `Arc<dyn TextEngine>` with a distinct, builtin-delegating custom engine. The original and current Arcs are confirmed different. `validate()` nevertheless succeeds, and completion using the runner's provenance-selection rule reports `text_engine: custom` for that already-builtin-painted frame. This is an exact-source App/protocol reproduction, **not a claim to have reproduced native Parley atlas corruption**. Tiny synthetic response pixels are not native evidence.

**Impact:** provenance is demonstrably wrong on this supported lifecycle seam. With actual differing atlas/epoch state, Render-time resource changes can also invalidate painted glyph references before submission. Semantic equality cannot establish font/atlas coherence. This is a reusable API correctness defect, not proof of an authorization bypass or unmarked-secret detection failure. The unchanged gallery engine does not trigger it in the native runs below.

**Requested correction, not applied:** retain paint-time text-engine provenance and a stable identity/version binding; reject detectable engine replacement/UV-epoch changes before capture/atlas upload, or otherwise keep the exact paint resources alive through submission. Cover same-kind engine replacement as well as kind changes, epoch changes, and ordinary unchanged-engine/default-off behavior. Keep custom internal font/configuration limits explicit: the framework cannot certify arbitrary unversioned custom interior mutation. Do not solve this by reconstructing/exporting another frame. Re-review the correction before v1 acceptance.

### F2 — P3 / low / nonblocking hardening: descriptor read can hang before the client timeout exists

**Evidence:** `tools/observe_client.py:45-52` only checks symlink status and reported size before `Path.read_bytes()`; it does not require a regular file or use a bounded read from a checked fd. The deadline is created later at line 59.

**Independent reproduction:** `client_probes.py` supplies a FIFO named `session.json` in a current-user-owned 0700 directory. It passes these guards and blocks with `timeout=.05`; the independent subprocess watchdog kills it at about .8s. No socket is contacted. `client-probes.log` records the result.

**Impact/boundary:** malformed local descriptor input can hang a client/job outside its requested time bound. Genuine server-created descriptors are regular files, and a same-UID attacker is already outside the advertised isolation model; this is not a remote capability bypass. Recommend a no-follow/nonblocking open where supported, fd-based regular-file/ownership/mode/size checks and a bounded descriptor read, with permanent FIFO/nonregular-file regressions.

### F3 — P3 / low / nonblocking output consistency: a second-file collision leaves a mixed partial pair

**Evidence:** `tools/observe_client.py:87-92` creates PNG and then JSON independently. If JSON already exists but PNG does not, it creates the new PNG before raising `FileExistsError` on JSON.

**Independent reproduction:** `client_probes.py` verifies the new PNG remains next to unchanged old JSON after the exception. Existing files are not overwritten, permissions remain private, and **the call reports failure**, so this is not a mismatched pair returned on success. Consumers must not glob both files and assume a failed call produced a coherent pair. Consider publishing via a fresh output directory/manifest or rolling back only files created by the failed call; add a JSON-only-collision/write-failure test. Do not remove any preexisting user file.

## Independent verification and commands

All commands below ran locally in the authorized existing worktree, offline/locked where Cargo is involved. Cargo used **2 jobs, dev/test debug=0, incremental=0**. Disk was checked: about 1.1GiB available at entry, about 1.3GiB at final check; no prior artifacts were removed. The direct rustc probe driver invokes commands serially with debug=0 and no incremental output. No desktop input/automation, screenshots of other apps, new OS grants, network dependency fetch, styling, unrelated changes, push/deploy or spend.

| Verification | Independent result | Saved evidence |
|---|---|---|
| Original payload seal and full diff | 111/111 hashes pass; exact diff; evidence-only seal | `integrity.log`, `check_review.py` |
| Author's artifact checker | exit 0 (supporting, not sole verification) | `artifact-check.log` |
| Affected Cargo suite | **572 passed, 0 failed, 0 ignored; 80 result groups** | `suites.log` |
| Python client suite | **8 passed** | `python.log` |
| Observer-disabled renderer unit suite | **22 passed** (overlap, not 22 new unique tests) | `disabled.log` |
| Default feature gallery check, without/with observe | both exit 0 | `default-disabled.log`, `default-observe.log` |
| Direct exact-source observer compilation and fresh paint compilation | 18 original observer tests + 3 reviewer probes = **21 passed** | `run_rust_probes.py`, `rust-probes.log` |
| Independent malformed descriptor / output-collision probes | both behaviors reproduced, driver exit 0 | `client_probes.py`, `client-probes.log` |
| Two actual native synthetic runs | four fresh PNG/tree responses; auth/stale denial; normal exit 0 and cleanup | `native-{light,narrow}/`, corresponding `.log` files |
| Independent PNG CRC/decompression/unfiltering and semantic tree references | all four author + four reviewer PNG/tree pairs pass; repeated raw pixels match original captures | `check_review.py`, `integrity.log` |

**Interpretation of the green adversarial probes:** the font probe intentionally asserts the defective *current* behavior to preserve an executable reproduction; its passing status does **not** mean that behavior is acceptable. The two other new Rust probes independently confirm Render-stage revocation denies both channels, and a sensitivity marker present at paint cannot be bypassed by removing it during Render.

Executed command sequence (each final command exit 0):

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1/check_artifacts.py
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s tools -p test_observe_client.py -v
cargo test --offline --locked -v -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-review/native-light
PYTHONDONTWRITEBYTECODE=1 python3 tools/validate_observation_native.py target/debug/examples/ui_gallery evidence/native-observation-v1-review/native-narrow --narrow --dark --200 --long
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-review/run_rust_probes.py
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-review/client_probes.py
cargo test --offline --locked -p rndr_render --no-default-features --lib
cargo check --offline --locked -p rndr --example ui_gallery
cargo check --offline --locked -p rndr --features observe --example ui_gallery
PYTHONDONTWRITEBYTECODE=1 python3 evidence/native-observation-v1-review/check_review.py
```

Run native replay commands with **new output directories**, not the sealed directories shown above. Probe scripts write generated Rust/build output only under `target/native-observation-review`; the observation source is copied exactly except its test-module path. The paint crate is compiled directly from its original source. Source hashes and compiler commands are in `rust-probes.log`. Dependencies, PNG encoder and original-suite paint dependency are existing pinned rlibs, not a full fresh dependency rebuild. Original Cargo suites used cache; the native executable was **not freshly rebuilt by this reviewer**. Its SHA256 matches the author's sealed executable `b96be3081726d73afb683466fa10e02555370c3ebb159311e8fa900c025c9263`, and it was exercised in fresh processes through the real local socket. Thus live repetition is independent runtime evidence, not independent provenance attestation of every compiled dependency. Initial direct-rustc harness diagnostics (wrong dependency identity/module paths, then missing lyon) are retained separately; the final corrected driver and tests succeed, and no such diagnostic was “fixed” in production code.

## Native evidence and guarantees

Both independent runs: Metal / Apple M4 / Bgra8UnormSrgb / AutoVsync / alpha mode Auto with opaque clear / **quads** / builtin-atlas / parley feature false / device scale **1**. Two frames per run, 28-node trees, source `live-surface-copy-before-present`, **`presented_confirmation:false`**. Light is 1000x700, narrow/dark/200% text/long is 320x700. Each initially denied one unavailable surface, then succeeded on a new request. Light revisions 3→4; narrow revisions 3→4 (session-local numbers need not equal the author's 4→5). Both tested wrong capability and future revision, exited normally via the existing 15s synthetic timer, and removed their exact session directories.

All four repeated raw images equal the original corresponding images: light raw SHA256 `7921fa8dc92de1336b06eca2e8c79773fd48a99efd5113c289a83121991427d3`; narrow raw SHA256 `c704670129dac30925b25e51d15ed038b4e44696aa9b13d3762d0799ecd3b209`. PNG chunks/CRCs, RGBA scanlines, dimensions, packet lengths, revision progression, tree root/child IDs and action descriptors were checked directly. Light has 10 distinct RGBA values; narrow has 2. This is structural/pixel-content inspection, **not a visual legibility, glyph correctness or polish acceptance**. No OS/compositor observation was performed or inferred.

Both renderer draw paths copy the acquired live texture **after** ordinary draw commands, in the same encoder/submission (`crates/rndr_render/src/gpu.rs:707-714`, `772-779`). The scene render resolves into that surface view before copy. Format, copy-usage, extents, pixel count/side, padded rows, map deadline and BGRA conversion guards are present (`gpu.rs:438-552`). No headless Exporter fallback is called. Successful completion follows the present call; this proves app surface contents submitted toward presentation, not compositor visibility. Scene readback, real native resize/DPI/interaction and all platforms are not thereby runtime-certified.

## Security/coherence audit disposition

- **Default/off:** observe feature is additive/nondefault; listener requires explicit World session; content remains default Deny. COPY_SRC negotiated only with an opted-in session. No-request/no-session path arms no readback and allocates no capture buffer; ordinary frame scheduling is unchanged. Both default compilation checks and disabled renderer tests pass.
- **Authority/transport:** independent random locator/capability; atomic private directory and exclusive descriptor; socket/descriptor 0600; directory 0700. One scoped Unix listener, one serial worker and bounded pending channel. Exact token/auth grammar, invalid auth does not queue or wake app. No TCP/global app enumeration or execution/input/action-dispatch endpoint was found. The `actions` arrays are descriptors only.
- **Privacy:** missing/Deny policy or ANY sensitive marker denies both image and semantics, including hidden/unsemantic entities. Both paint-boundary and post-Render checks are necessary and present; independent before/after sensitivity/revocation probes pass. Whole-frame Public remains an explicit **application audit assertion**, not inference/redaction of unmarked private strings. Custom paint/images/render callbacks require application audit; a dishonest Public classifier is outside this guarantee. All SurfaceSlots are conservatively denied. No separate marked-sensitive/auth bypass was found in the inspected paths/tests.
- **Lifecycle/bounds:** stale/future/expired requests fail closed; no historical image cache. Side/pixel/readback, node/metadata caps, serial-rate limit, absolute request/socket-write deadlines, bounded pending shutdown and GPU wait guards inspected. Slow/truncated/oversized/auth/revoke/drop tests pass. Crash/SIGKILL cannot run Drop and stale private files may remain, as documented. Client network reads are bounded; descriptor preprocessing has F2.
- **Paint coherence:** immutable semantics captured after normal update/layout/paint and before Render; tree/supplemental semantics/scroll/theme/scale/sensitivity are compared after Render. Native extent/image dimensions are bound. **Text-engine/atlas provenance is the remaining F1 gap**. App-supplied theme is not renderer-certified color interpretation; custom unversioned font internals are not automatically inferable.

## Explicit remaining gaps / authority

Live **scene** capture, Parley/custom engine rendering, font/fallback changes and actual atlas/epoch transitions, shaping/IME, physical DPI/multi-monitor and native resize, scrolling/focus/input interaction, transparent desktop composition, lost/occluded surface transitions beyond startup, alternate surface formats/backends, Linux/Windows and OS accessibility integration were **not independently exercised live**. Core/protocol synthetic tests and default-feature compilation do not close those gates. No compositor/OS-presented confirmation, desktop/chrome/menu/dialog/cursor/ICC/HDR/other-app proof exists.

Preserve all prior implementation and evidence. Requested next action is a targeted, separately authorized correction of F1 plus permanent regressions and fresh review; F2/F3 can be addressed in that bounded pass or explicitly tracked. **Do not integrate/approve from this review.** No styling/Blooey/Medousa/unrelated work was performed. Only this new review evidence directory is eligible for the review-only commit.
