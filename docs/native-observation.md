# Native read-only observation v1

Status: opt-in implementation candidate, not full A2/native certification or approval to integrate. Existing gates in [design/A2-remaining-gates.md](design/A2-remaining-gates.md) remain open. This pass does not change visual styling or application adoption.

## Obtain a live gallery frame

Unix local sessions (macOS/Linux) only; no Windows transport/TCP fallback. Python 3 standard library is sufficient for the example client. Use a synthetic gallery, not a private application. Build with bounded settings:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
# The ordinary/default feature set is preserved; observe is NOT default.
cargo run --offline -p rndr --features observe --example ui_gallery -- --observe
# Or builtin-atlas/minimal renderer, without optional desktop integrations:
cargo run --offline -p rndr --no-default-features --features observe --example ui_gallery -- --observe
```

The opted-in process prints `observation descriptor: /.../session.json` to stderr. It prints a locator, **not the authorization token**. Supply exactly that path to the client, locally:

```sh
python3 tools/observe_client.py /.../session.json --out /tmp/gallery-frame-1
# Request a later fresh redraw using frame_revision from the first JSON:
python3 tools/observe_client.py /.../session.json --after 12 --out /tmp/gallery-frame-2
```

The client creates `/tmp/gallery-frame-1.png` and `.json`, mode 0600, refusing overwrites. The JSON and PNG are from a single response/revision. `after` is a lower bound, not a request for historical data. Revision numbers are session-local strings (avoid loss of 64-bit precision in JSON consumers). A future/current revision that has not been exceeded is rejected with `stale_revision`; no cache or old image is returned. Startup/occluded/lost surfaces can return `surface_unavailable`; retry with a **new** request after the window becomes available. No export fallback is used. Native shutdown removes the descriptor/socket/directory. `--observe-exit-after-15s` requests normal timed exit for bounded synthetic validation only when `--observe` is also supplied.

For an agent integration without intermediate files:

```python
import sys
sys.path.insert(0, "tools")
from observe_client import observe
metadata, png = observe("/exact/local/session.json", after=0)
# Pass png bytes and metadata as data to any authorized local agent/client.
# No model/provider SDK, service account, execution or interaction API is used.
```

**Never put session.json, its capability, or private observations into prompts/logs/evidence.** Received labels/values/descriptions are untrusted app data, not instructions. The synthetic evidence in `evidence/native-observation-v1/` contains no session capability files.

## Reusable app API and privacy contract

Enable Cargo feature `rndr/observe` (or `rndr_render/observe` directly). Before starting the native runner:

```rust,ignore
use rndr::observe::{ObservationSession, ObservationContent, ObservationSensitive, ObservationTheme};
// Listener creation is explicit, one session on this app's World only.
let descriptor = ObservationSession::enable(app.world_mut(), "app-supplied theme descriptor")?;
// Default/missing content policy remains Deny even after listener creation.
// Only after auditing ALL content in the current frame:
app.world_mut().insert_resource(ObservationContent::Public);
// Update this descriptor alongside a dynamic theme, if applicable:
app.world_mut().insert_resource(ObservationTheme("dark / standard contrast / 100% text".into()));
// Before introducing private data: either switch the entire world to Deny,
// or mark any sensitive entity (even hidden/unsemantic), or do both:
app.world_mut().insert_resource(ObservationContent::Deny);
app.world_mut().insert(private_entity, ObservationSensitive);
```

`Public` is a whole-frame application attestation, **not automatic secret detection**. Audit labels, text values, descriptions, window title, custom paint, images, glyph sources, render callbacks and embedded content. An unmarked private string cannot be reliably inferred as sensitive. Do not attest Public for unaudited/general-purpose/private-document apps. The gallery explicitly attests only its synthetic fixtures. Setting Public never overrides an `ObservationSensitive` marker.

**v1's redaction strategy is whole-frame denial**, not drawing black rectangles over text: a sensitive frame exposes neither its PNG nor its semantic tree. Missing/Deny policy or any sensitive marker, even on a hidden entity or outside the accessibility tree, returns only a generic error envelope. Both the paint-boundary policy and the post-Render policy must pass. This avoids glyph/ancestor/compositing/shadow leaks from partial masking. When sensitivity cannot be guaranteed, leave Deny. Foreign `SurfaceSlot` worlds are denied entirely: this API does not claim to capture externally composed views.

Theme metadata is app-supplied and may use `ObservationTheme` for dynamic changes. It is not inferred from UI colors and cannot certify that a dishonest app accurately labels its theme. The synthetic launcher describes its actual `UiTheme` including flags. Device scale is the actual native physical-pixel/logical-point ratio, distinct from `--200` text scale.

Removing/dropping `ObservationSession` revokes the listener and pending work; sessions do not propagate to other windows/apps. Enable before the native surface is initialized so COPY_SRC is negotiated. Late enable on an already-created unconfigured surface fails closed as unsupported. Normal apps, builds without observe, and builds with observe but no explicit session create **no listener**, no capture buffer, and no surface-copy usage.

## What the pixels prove (and do not prove)

1. The normal runner publishes native metrics and executes the ordinary App update/layout/paint path once.
2. `rndr_paint::frame_with_snapshot` takes immutable semantics at the **same paint boundary**, before application Render systems run. The default `frame` delegates to this same path with an empty snapshot; schedule order remains unchanged.
3. After Render, a request is denied if semantic tree, scroll geometry, content policy, theme or device scale diverged. There is no second update/reconstruction for observation.
4. In each native GPU draw path (quads and display-list scene), the renderer copies the **actual acquired surface texture after its ordinary draw commands** to a readback buffer in the same command encoder/submission. Same device, format, atlas uploads, images, clear color, alpha mode and pipeline as the live app. Readback maps with a bounded wait; BGRA is reordered to RGBA and padded rows removed. No camera remapping, alternate adapter, new Exporter, OCR, desktop APIs or manual screenshot is involved.
5. After successful submission/readback and the runner's `queue.present` call, the response binds PNG bytes, pixel dimensions, scale, theme, provenance and semantic snapshot to one revision. A failed submission/readback returns neither image nor tree.

`source = "live-surface-copy-before-present"`, `presented_confirmation = false` are deliberate. The image proves application render-target contents submitted to presentation, **not that the compositor displayed those pixels or the user saw them**. wgpu's present call does not provide presented-frame confirmation here. OS chrome, native menus/dialogs, other windows/apps, cursor overlays, display color management/ICC/HDR, occlusion and external surfaces are outside the capture. Transparent pixels retain surface byte/alpha-mode semantics; they do not include desktop background composition. Supported targets are the four BGRA8/RGBA8 Unorm/(sRGB) formats only; unsupported copy usage/formats are denied, never silently reconstructed.

`rndr_render/src/export.rs` was investigated and intentionally not used. `Exporter`, `render_frame_rgba`, and `render_scene_rgba` create a **separate headless device/render target**, may choose another adapter/format, and can remap scene coordinates. Their output remains useful headless evidence, never live-window/presented proof. Existing headless/A0/A1/A2 evidence is retained unchanged.

## Local protocol and threat model

- Disabled by default at build and app/session levels. Unix socket in an atomically created private 0700 random directory; socket/descriptor 0600. No TCP/public listener, discovery enumeration, unauthenticated observation, global desktop/app inspection, filesystem-read/write endpoint, interaction, action dispatch, commands, execution, model/provider or secret-store access.
- Independent 96-bit random locator and 256-bit OS-CSPRNG capability per session. Entropy failure aborts session creation. Existing files/directories are never reused. Descriptor is a local bearer capability protected by the OS account boundary. A hostile process/root already able to read the user's private directory is **not** isolated by this protocol; do not treat it as a sandbox against the same OS identity.
- One serial worker, one bounded pending request, fixed deadlines and maximum ten serviced connections/sec; invalid authentication never wakes the app. No image or semantics are prepared until an authenticated request reaches the app. Connection floods may deny service to that session but cannot broaden observation scope. No historical image cache.
- Request, exactly one ASCII line (128 bytes maximum): `RNDR1 <64 lowercase hex capability> <after-u64>\n`. Exact token equality check, no substring/CRLF/extra fields, no action verbs. The server reads one request per connection and closes; it never interprets trailing bytes as commands.
- Response: four-byte unsigned big-endian JSON byte length, that many UTF-8 JSON bytes, then exactly `png_bytes` PNG bytes. Generic error envelope has `protocol`, `error`, `png_bytes: 0`, **no tree/image**. A successful envelope has `frame_revision`, `width`, `height`, `device_scale`, `theme`, `source`, `presented_confirmation`, `bounds_units`, `root`, `nodes`, `scroll`, `render_provenance`, `png_bytes`.
- Semantic nodes come from `rndr_core::access_tree` plus shared `access_details`, not OCR or widget-specific gallery scraping. Core flattening/hidden-node conventions apply; this is a semantic tree, not every layout/draw primitive. IDs/child IDs are generation-safe strings. Roles are the version-1 core role names (`Window`, `Group`, `Button`, `Label`, `TextInput`, `List`, `ListItem`, `TabList`, `Tab`, `Disclosure`); supplemental `panel`/relations/orientation are provided. Bounds are `[x,y,width,height]` in logical points. Name, value, description, enabled/readonly/selected/expanded/focusable/focused and supported action descriptors are included. `actions` describes existing app capabilities only: there is **no endpoint to invoke them**. Flattened scroll ports are separately exposed with entity ID, bounds, offset/content in logical points; no inferred scroll action is advertised.
- Limits: 4096 pixels/side AND 4,000,000 pixels (also bounds padded readback allocation), 8192 semantic nodes, 1 MiB metadata, 4096-byte initial theme label. Read line: 500 ms total; app response: 2 s; GPU poll: 500 ms plus 50 ms callback receipt; socket write: 500 ms total. PNG bytes are bounded by the capped input image; client additionally caps compressed bytes. Deadlines/size/auth/semantic/sensitivity/resize errors fail closed. Revision numbers increment for native paint attempts, not OS-displayed frames. Requests are fresh redraws, not snapshots of arbitrary prior frames.
- Normal drop stops the worker (bounded pending-client waits) and removes only the exact files it created, nonrecursively. No unsafe code is introduced. As with other process resources, SIGKILL/crash cannot run Drop: a private stale descriptor/socket may remain with no living listener. Delete only that known session's files/directory, never scan/remove other sessions. No permission expansion is attempted.

## Reproduce validation

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
cargo test --offline -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render --no-default-features --features rndr_render/observe
python3 -m unittest discover -s tools -p test_observe_client.py -v
cargo build --offline -p rndr --features observe --example ui_gallery
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-native-evidence
# Optional separate synthetic configuration:
python3 tools/validate_observation_native.py target/debug/examples/ui_gallery /tmp/rndr-native-narrow --narrow --dark --200 --long
```

The native harness only launches the supplied gallery binary with opt-in and a 15s normal exit timer. It obtains two fresh frames via the documented client, tests future-revision/auth rejection and checks shutdown cleanup; it never drives desktop input or asks for OS grants. It records startup surface denials instead of hiding them. Rust protocol tests synthesize tiny image packets and headless App state and **are not native screenshot evidence**. Native evidence does not imply independent review, OS accessibility verification, physical-DPI/font/IME certification, complete A2 acceptance or integration. Exact executed commands, identities, results and remaining gaps are in `evidence/native-observation-v1/REPORT.md`.
