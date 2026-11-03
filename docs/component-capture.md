# Component-targeted headless capture

This is a separate `rndr_render::capture` API, enabled by **`observe`**, and the
cross-platform `rndr-capture` CLI. It creates an offscreen wgpu device/texture,
not a window, event loop, native surface or socket. It never captures other apps.
It is **not evidence of native presentation**. The existing
[native observation protocol](native-observation.md) still captures the actual
whole-window surface on Unix and is unchanged.

## In-process API (Entity or UiId)

```rust,ignore
use rndr_render::{capture::{capture, Target}, Exporter};
use rndr_render::observe::{ObservationContent, ObservationSensitive};

// Audit ALL content, not just this component, before opting in.
app.world_mut().insert_resource(ObservationContent::Public);
let mut exporter = Exporter::new()?; // Reuse pipelines for repeated captures.
let exact = capture(&mut app, &mut exporter, Target::Entity(handle.root), |_, _| None)?;

// rndr_ui remains GPU-free; the renderer and core do not depend on rndr_ui.
let keyed = capture(&mut app, &mut exporter, Target::UiId {
    id: ui_id.0,
    parent: Some(parent), // Direct-parent scope, not an ancestor search.
    expected: Some(handle.root), // Optional remount/generation pin.
}, |world, entity| rndr_ui::identity(world, entity).map(|id| id.0))?;
let png = rndr_render::encode_png(&keyed.image)?;
let json = keyed.metadata.to_json();
```

`rndr_ui::identity` reads the actual keys used by mounted surfaces, controls,
collections, data editors and selection controls. It does not create a second
registry/tree, mutate components, or assign keys to internal decorations/borrowed
content. List item IDs, property IDs and ephemeral recycled row identities are
**not** UiIds: use the component root and application-owned logical item identity
when working with a virtual list. An Entity identifies its current ECS lifetime,
not the historical logical record in a recycled slot. The resolver must be pure
and stable for the immutable World; changing interior state/concurrent mutations
without versioning is not supported.

`capture` delegates to the **same frame path as `frame_with_snapshot`**. The new
`rndr_paint::frame_with_snapshot_frame` callback exposes the exact just-painted
Frame beside its immutable World, so GPU atlas/image reads and readback can finish
*before* Render systems run. There is one update/layout/paint, no second scene
reconstruction. The original API and ordinary/native frame order are preserved.
After Render, the result is discarded if privacy is revoked, foreign surfaces
appear, the target's resolution/subtree geometry/identity/hidden state or the
accessibility tree/details, scroll state or declared observation theme changes,
or the text engine is replaced/its paint epoch
changes. Both channels are returned together or neither. Render-only cosmetic
changes not represented by these guards do not retroactively alter the already
rendered paint-time result; this is a snapshot, not a promise that the World is
unchanged in every component after the call.

A missing/Deny policy rejects before advancing the app. If a later paint-time
capture check fails, the normal Render systems still run. The API is synchronous
and can block waiting for GPU readback; it uses Exporter's existing synchronous
wgpu wait, **not a wall-clock deadline/cancellable native observation request**.
No prior-frame capture is cached. For deterministic scenes, supply the existing
`Sim`/manual clock and deterministic input/state; capture does not freeze timers,
background feeds, caret animation or application code for you.

Run the mounted `rndr_ui` example (uses the ordinary builtin headless text engine,
not native startup's Parley upgrade), choosing a new output prefix:

```sh
# Preserve an explicitly configured, compatible shared target cache.
# In constrained checkouts, set CARGO_TARGET_DIR to the EXISTING shared target first.
source scripts/data-gallery-env.sh
cargo run --offline --locked -p rndr --example component_capture --features observe \
  -- /private/output/card-NEW
```

The example checks ambiguous versus parent-scoped UiId resolution, generation
pinning, subtree identity, nonempty real RGBA output and absence of labels in
metadata. Calling the API does not install a window, IPC listener, image loader,
extra App systems or a default font/engine upgrade.

## Pixels, coordinates, clipping and scale

- Output is the **headless viewport composite in the target's visible layout
  box**, not an isolated subtree repaint. Siblings/overlays may occlude it. This
  preserves actual composition and inherited clips. Metadata contains only the
  target subtree, so an occluding sibling can appear in pixels without a node in
  this subtree. Use an Entity/UiId belonging to the containing surface if that
  context is needed.
- It renders the complete capped viewport offscreen, then extracts the target
  box from those headless pixels. This is deliberately not called a native
  window crop. Even a tiny target requires the whole viewport to fit the cap.
- `Rect`/`Clip` and all metadata bounds use **global logical points**, origin at
  the viewport top-left. They are stored layout geometry, not glyph outlines,
  shadow extents, a union of descendants, or a renderer-inferred visual hull.
  A non-layout descendant has null bounds/clip. Hidden descendants are recorded
  as hidden; any stored bounds on them may be from a previous layout and must
  not be treated as current visible geometry. A hidden target/ancestor is empty.
- The target extraction is `target Rect ∩ effective Clip ∩ viewport`. Overflowing
  children outside that box and shadows outside it are not included. The full
  composite keeps ordinary renderer clipping/paint order. Foreign SurfaceSlot
  worlds are denied in their entirety, **including CanvasRegistry slots** for
  now; no external view/canvas placeholder is misrepresented as captured.
- `Viewport.scale` is positive finite physical pixels per point, independent of
  UI text scaling. Invalid raw scales are rejected, not normalized to 1.
  Viewport physical dimensions are `ceil(points * scale)`. Extraction left/top
  are floored and right/bottom ceiled, then capped to the physical viewport.
  `pixel_rect = [x,y,width,height]` identifies those exact physical pixels.
  At fractional edges, the outward-rounded boundary pixel can contain composite
  coverage from just outside the logical box; no resampling or synthetic padding
  is done. The PNG origin is this physical rectangle's top-left.
- Exporter uses an RGBA8 sRGB-capable offscreen format and transparent clear;
  bytes follow the existing `RgbaImage`/Exporter contract. It does not reproduce
  native window clear/alpha mode, desktop background, color management, chrome,
  cursor, menus, external views or compositor behavior. Adapter and font/backend
  differences can affect rasterization; compare tolerant regions, not arbitrary
  cross-platform byte-identical text goldens.
- CPU-retained `load_rgba` images work. An image with no retained CPU pixels
  returns `image_unavailable`, even if a native GPU cache or a prior Exporter
  happens to hold it. Unfinished async decodes and CPU pixels released by the
  native presenter cannot be silently replaced with transparent pixels. For
  headless fixtures, finish decoding and store pixels with `load_rgba` first
  (or consume `DecodedImage` in an application system and retain its bytes).
  Capture rebuilds its image cache each call, so replacing a numeric image ID or
  reusing an Exporter across Worlds cannot reuse stale pixels. If the configured
  image budget cannot keep all painted images resident, the entire capture returns
  `image_unavailable` rather than silently omitting images. Ordinary renderer
  budget/downsampling rules still apply; this is not lossless source-image export.

## Identity, geometry and privacy failures

| Result | Meaning |
| --- | --- |
| `target_not_found` | No key matches at the paint boundary. |
| `target_ambiguous` | Multiple matches; global keys are not assumed unique. Supply a direct parent. |
| `stale_identity` | Dead Entity/parent/expected generation, or a live key now resolves to a different pinned Entity. |
| `invalid_geometry` | Target lacks Rect; viewport, scale, or any stored subtree Rect/Clip is missing where required, negative or nonfinite. Non-layout descendants are allowed. |
| `empty_target` | Hidden ancestor/target, zero area, fully clipped, or entirely out of viewport. No 1×1 placeholder. |
| `capture_limit` | Viewport exceeds 4096 physical pixels/side or 4,000,000 total pixels; traversal exceeds 4096 nodes, is cyclic/duplicated, the accessibility tree exceeds 4096 nodes, or serialized metadata exceeds 1 MiB. |
| `post_paint_mutation` | Post-Render resolution/geometry/identity/accessibility/text-engine validation failed. Retry a new frame after settling app state. |
| `sensitive_frame` | Missing/Deny public attestation or **any** sensitive entity in the World. |
| `foreign_surface_unsupported` | Any SurfaceSlot exists, including hidden/off-target slots. |
| `image_unavailable` | A painted image has no retained CPU pixels or cannot remain resident within the image budget. |
| `headless_render_failed` | Adapter/device/size/readback failed; the typed API retains `ExportError`. |

Privacy is the existing observation policy, **not an option to disable redaction**:
missing/Deny rejects; Public is a trusted application attestation covering *all*
painted data, images, custom paint and metadata identifiers. Any
`ObservationSensitive` marker, even hidden, unsemantic or outside the target,
denies the entire frame and **all metadata**. No mask/crop can override it.
Checkpoints before update, at paint and after Render must pass. Metadata is
whitelisted to generation-safe entity/parent IDs, caller numeric keys, bounds,
clips and hidden state: it never serializes labels, values, descriptions, source
paths or arbitrary components. Numeric keys can themselves encode private data;
Public must audit them too. Mark data before introducing it; arbitrary unmarked
secrets cannot be discovered automatically. All custom engines must version
invalidating interior state and avoid concurrent mutation during capture.

## Cross-platform CLI

`rndr-capture` drives **bounded, declarative public scenes**, not arbitrary running
processes or arbitrary Rust callbacks. An application can use the library API
above for its real mounted UI; the CLI's JSON fixture is intentionally a smaller
portable input surface with no code execution, IPC, filesystem/image loading or
plugin discovery from scene data.

```sh
source scripts/data-gallery-env.sh # Set existing CARGO_TARGET_DIR first if needed.
cargo build --offline --locked -p rndr_render --features observe --bin rndr-capture
# Use that target directory's debug/rndr-capture (rndr-capture.exe on Windows):
"$CARGO_TARGET_DIR/debug/rndr-capture" --scene examples/component-capture.json \
  --ui-id 42 --parent 0:0 --output /private/output/cli-NEW
# Or --entity 1:0; full index:generation pair, never a bare index.
```

The executable uses platform-neutral Rust filesystem/argument parsing and the
same `capture` function and Exporter as the in-process path. No Unix sockets,
Python client or display server is required. A working wgpu headless adapter is
still required (software rendering is backend/environment-dependent).

Scene version 1 (`examples/component-capture.json`):

- UTF-8 JSON, at most 64 KiB; `version: 1`, optional `public: true` (default deny),
  `viewport: [width_points,height_points,scale]`, `nodes: [...]` (at most 256).
- Nodes are ordinary core column Layouts: required `size: [width,height]`, optional
  nonnegative finite `padding`/`gap`, `color: [r,g,b,a]` in 0..1, `label` (≤4096
  UTF-8 bytes), `hidden` and `sensitive` booleans. Layout sizes are nonnegative;
  viewport dimensions and scale must be positive finite.
- Optional `id` is a **decimal u64 string**, not a JavaScript number; optional
  `parent` is an earlier node's array index. The latter is only a scene loader
  index, not a target selector. New fixture Entity indices/generations start at
  `0:0`; output metadata confirms actual handles. Unknown fields/unsupported
  versions/types/cyclic or forward parent references are errors.
- `--ui-id N` resolves globally unless `--parent INDEX:GEN` scopes the direct
  parent. `--entity INDEX:GEN` selects an exact fixture Entity. Exactly one target
  selector is required; duplicate/unknown CLI options are rejected.
- Successful return writes `PREFIX.png` plus `PREFIX.json`, refuses overwrites,
  and prints dimensions. Only consume a pair after successful exit. Ordinary
  write failure rolls back only this invocation's new files. This is **not a
  crash-atomic two-file transaction** or a sandbox against hostile same-user
  filesystem races; a crash can leave a partial pair. Use a private directory.
  Final output-file symlinks (including dangling links) are refused rather than
  followed. Parent directories must already exist and are trusted; parent symlinks
  are not a sandbox boundary. Unix files are created mode 0600; Windows uses the
  output directory's ACL.
- Exit codes: `0` success/help, `1` output I/O, `2` usage/scene input,
  `3` target/geometry/limit/coherence/unavailable-image, `4` privacy denial,
  `5` adapter/render/PNG failure. Failures emit generic content-free stderr,
  never a partial capture payload or application text.

CLI nodes with ids are a fixture identity adapter, not an assertion that a plain
core Layout is an `rndr_ui` component. `component_capture` demonstrates real
mounted UI with the exact same API.

## Verification and explicit gaps

```sh
source scripts/data-gallery-env.sh
cargo test --offline --locked -p rndr_render --lib --features observe capture::tests -- --test-threads=1
cargo test --offline --locked -p rndr_render --bin rndr-capture --features observe
python3 scripts/test-component-capture.py "$CARGO_TARGET_DIR/debug/rndr-capture"
```

The capture tests require a real adapter/readback for pixel cases and **fail, not
skip**, if it is unavailable. Geometry/identity/privacy tests also run without
constructing a device. Pixel tests check same-frame update identity/color, scales,
visible clipping, overlays, retained images and resource invalidation. The CLI
suite decodes the actual PNG and checks pixels as well as error codes/no-output
privacy and overwrite behavior. Source-bound commands/results and sample captures
are in `evidence/component-capture/REPORT.md`; independent review, image-cache
corrections and expanded filesystem probes are in
`evidence/component-capture-independent/REPORT.md`.

This increment does not certify native Windows/Linux/macOS presentation, input,
accessibility, screen readers, DPI, font fallback, IME or startup. No Wayland
layer-shell/overlay, markdown host slots/theme bridge, GPU-only foreign views,
application-process remote capture, sandbox, arbitrary component serialization,
or publication/deployment is added. These remain separate tracks.
