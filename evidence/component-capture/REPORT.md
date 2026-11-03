# Component-targeted headless capture — candidate evidence

## Scope and binding

- Isolated branch: `worktree/rndr-component-targeted-headless-capture`.
- Baseline **HEAD and main independently resolved and matched**:
  `318edfba9d609a1aba088d53aacfb8cf0c0d0840`.
- `source.sha256` binds all 16 changed source/test/manifest/doc/example paths.
  `evidence.sha256` binds this report, command logs, source manifest and PNG/JSON
  witnesses. The candidate commit containing this report is returned by the worker;
  its source hashes are independently checkable without a self-referential hash.
- Host: macOS 27.0.1 arm64; rustc 1.98.0, aarch64-apple-darwin. Exact host/compiler,
  baseline, shared target and built binary hashes: `environment.log`.
- Existing shared target reused throughout:
  `/Users/theelevators/Library/Application Support/medousa/forge/worktrees/fbb77c0fa9c2/rndr-refined-cross-platform-ui-framework/target`.
  `scripts/data-gallery-env.sh`: jobs=2, incremental=0, dev/test debug=0. Offline,
  locked commands; no new target directory, cache cleanup or copied build caches.
  Free disk: **2.2 GiB initially, 1.7 GiB after verification**. No other source,
  worktree, history, evidence or shared cache was removed.
- This is a **worker candidate**, not sender acceptance or native integration.
  No main modification, merge, push, publication, deployment, credentials or spend.

## Implemented contract / changed files

- `crates/rndr_paint/src/lib.rs`: ordinary `frame_with_snapshot` delegates to new
  `frame_with_snapshot_frame`; same update/layout/paint order, exact Frame plus
  immutable World at the paint boundary. GPU submission/readback can finish before
  Render mutates resources; no second paint or synthetic scene reconstruction.
- `crates/rndr_render/src/capture.rs`, `src/lib.rs`: observe-gated synchronous
  `capture` API. Exact generation-bearing Entity or adapter-resolved UiId numeric
  key, direct-parent scope, optional previous Entity pin, same-frame RGBA and
  target/subtree Entity/parent/UiId/Rect/Clip/hidden metadata. Metadata deliberately
  contains no names, labels, values, descriptions, source paths or arbitrary data.
- `crates/rndr_render/src/observe.rs`: **only five helper visibilities** changed
  to crate-visible (public policy, engine epoch, semantic details, scroll state,
  declared theme). Native protocol/authorization/redaction logic is unchanged;
  headless capture reuses those same policy/coherence checks.
- Fail closed on missing/Deny attestation or any sensitive entity (even hidden,
  off-target, unsemantic); checks before update, at paint and after Render.
  Changed identity/geometry/hierarchy/hidden state, accessibility/details, scroll,
  observation theme/scale or text-engine identity/epoch drops both channels.
- Genuine offscreen Exporter/device/texture with no native window or socket.
  Output is the **viewport composite extracted to the target's visible layout
  box**, not isolated subtree rendering or a native-window screenshot. Overlays
  remain visible. Global logical geometry is kept; visible bounds intersect
  Rect/Clip/viewport; physical extraction rounds outward at positive finite scale.
  Missing/ambiguous/stale, invalid/empty geometry, caps and unavailable resources
  have explicit typed failures. Same conservative 4096-side/4M-pixel caps as live
  observation; hierarchy/accessibility capped at 4096 nodes and metadata at 1 MiB.
- Foreign SurfaceSlot worlds (including CanvasRegistry slots) are explicitly
  unsupported. Retained CPU `load_rgba` images work; missing/unfinished/native
  GPU-only/released CPU images return `image_unavailable`, never blank success.
- `crates/rndr_ui/src/lib.rs`: read-only `identity` adapter over existing mounted
  component keys; **no core→UI or UI→renderer dependency**, no second registry.
  Recycled logical row ItemIds are not component UiIds; documented lifetime limits.
- `crates/rndr_render/src/bin/rndr-capture.rs`, `Cargo.toml`: portable CLI using
  the same API. Deliberately bounded declarative JSON scene input, not remote live
  apps/arbitrary code. Strict schema/options, explicit public attestation, target
  errors, content-free diagnostics and stable exit categories. Output refuses
  overwrite, Unix mode 0600, ordinary-failure rollback of its own new files.
  Two-file output is not crash-atomic; Windows inherits directory ACLs.
- Tests: `capture/tests.rs` (13), `rndr_ui/tests/capture_identity.rs` (1), CLI unit
  tests (2), `scripts/test-component-capture.py` (26 subprocess checks).
- Examples: `examples/component-capture.json` and
  `crates/rndr/examples/component_capture.rs` (actual mounted Card/scoped UiId).
- Full contract/errors/coordinate/privacy/usage docs: `docs/component-capture.md`;
  discovery links in renderer/UI READMEs and `docs/native-observation.md`.
  No core source or Cargo.lock change. Markdown slots/theme, overlay/IME/startup
  implementations remain outside this increment.

## Exact verification

All commands ran from this isolated root with:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
export CARGO_TARGET_DIR="$(dirname "$PWD")/rndr-refined-cross-platform-ui-framework/target"
. scripts/data-gallery-env.sh
```

**Full affected suite**, exit 0, **770 Rust test executions / 100 suite outputs,
0 failures, 0 ignored**, including doctests (do not add the targeted reruns below
again to that count):

```sh
cargo test --offline --locked -p rndr_core -p rndr_paint -p rndr_ui -p rndr_render \
  --features rndr_render/observe -- --test-threads=1
```

Log: `affected-suites.log`. Includes the existing live-observation privacy,
transport, semantic/engine coherence regressions, normal export GPU tests and
prior virtual-list/selection/data-editor regressions.

`final-gates.tsv` records **exact final commands and exit codes**; each row's
name identifies its `.log`. All 12 final gates exited **0**:

| Gate | Command / result |
| --- | --- |
| capture-tests | `cargo test --offline --locked -p rndr_render --lib --features observe capture::tests -- --test-threads=1` — **13 passed**; real adapter/readback cases fail rather than skip if no adapter exists. |
| clippy | `cargo clippy --offline --locked -p rndr_render --features observe --lib --bin rndr-capture --no-deps -- -D warnings` |
| ui-clippy | `cargo clippy --offline --locked -p rndr_ui --lib --no-deps -- -D warnings` |
| docs | `env RUSTDOCFLAGS=-Dwarnings cargo doc --offline --locked -p rndr_render -p rndr_ui --features rndr_render/observe --no-deps` |
| no-default-capture | `cargo check --offline --locked -p rndr_render --no-default-features --features observe --bin rndr-capture` |
| no-observe | `cargo check --offline --locked -p rndr_render --no-default-features --lib` — no accidental default capture/CLI requirement. |
| cli-build | `cargo build --offline --locked -p rndr_render --bin rndr-capture --features observe` |
| cli-contract-tests | `python3 scripts/test-component-capture.py "$CARGO_TARGET_DIR/debug/rndr-capture"` — **26 subprocess checks passed**, actual PNG decoded/pixel-checked; error no-output/privacy, scope, stale generation, missing/ambiguous, hidden/zero/out-of-bounds, invalid/oversize input, file mode and overwrite rollback checks. |
| ui-example | `cargo run --offline --locked -p rndr --example component_capture --features observe -- evidence/component-capture/ui-card-final` — UiId42→Entity(1:0), **4 subtree nodes, 320×136 real pixels**; ambiguous/global versus parent-scope/pin assertions and text-free metadata checks passed. |
| cli-sample | `"$CARGO_TARGET_DIR/debug/rndr-capture" --scene examples/component-capture.json --ui-id 42 --parent 0:0 --output evidence/component-capture/cli-sample` — **128×64 real pixels**. |
| formatting | `rustfmt --edition 2021 --check --config skip_children=true` on exactly the nine changed Rust files, enumerated in TSV. |
| diff-check | `git diff --check` |

The pixel tests assert actual colors at scales 1/2, same-update changed key/color,
visible viewport clipping, occluding overlays, retained image pixels, unavailable
CPU image failure and post-paint privacy/identity/engine/semantic/scroll/theme/
scale invalidation. Geometry tests also cover fractional outward rounding,
negative/NaN/invalid scale, zero/offscreen/hidden, cycles and node/viewport caps.
The UI-only key test checks real Card/Button roots, internal nodes returning None,
parent scopes, in-place remount and dead generations without a renderer dependency.

Final witnesses: `ui-card-final.png/.json`, `cli-sample.png/.json`. Earlier
`ui-card.png/.json`, `renderer-tests.log` (88 tests before the last parity probe),
`cli-unit-tests.log` and `capture-tests-fixture-diagnostic.log` are retained as
intermediate evidence, not substituted for final logs. The diagnostic run had
11 pass/1 fail because the image test's green fixture fill painted over the blue
image, per existing normal painter order. Removing that test foreground fill
fixed the probe; final image/pixel tests and the entire affected suite passed.
No renderer paint order was changed to satisfy it.

Final staging note: preserved raw Cargo terminal logs contain trailing blank
lines. Unrestricted `git diff --cached --check` reports only those four raw-log
EOF blanks; they were not edited to cosmetically pass the gate. Final staged
source/doc/script checking uses `git diff --cached --check -- .
':!evidence/component-capture/*.log'`. Source/test hashes remain unchanged.

## Unverified / explicit limits

- **No native/platform certification.** Windows/Linux execution or cross-target
  compilation, MSRV, native visual/input/accessibility/screen-reader/DPI/IME,
  startup/overlay/compositor behavior were not exercised. Default and no-default
  feature checks here are host-only. No fresh native gallery was launched.
- Actual offscreen pixels are proven by GPU readback and decoded PNG assertions.
  The image-view tool returned image metadata, not inspectable pixels to this
  model; **no human/model aesthetic visual acceptance is claimed**.
- CLI fixtures use core layout/key adapters; mounted UI is demonstrated separately
  through the same library API. Arbitrary running apps/component serialization,
  remote process capture, foreign/canvas slots, native GPU-only images and an
  asynchronous deadline/cancellation API are not implemented.
- Capture advances one normal frame; it does not freeze clocks/background feeds or
  automatically detect unmarked secrets. Public attestation and engine versioning
  remain trusted application obligations. No benchmark/performance claim or
  byte-identical cross-backend text guarantee is made.
- Registry packaging/release, broader whole-workspace lint/doc gates, independent
  sender acceptance and native integration are not claimed by these scoped checks.
- **No remaining implementation/test blocker on this host.** Sender retains
  independent review and all native integration authority. Nothing was merged.
