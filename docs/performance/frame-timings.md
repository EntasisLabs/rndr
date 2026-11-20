# Opt-in CPU frame timings

Insert a `FrameTimings` resource before running the native app:

```rust
use rndr_core::{App, FrameStage, FrameTimings};
let mut app = App::new();
app.world_mut().insert_resource(FrameTimings::new(240));
// In a system, debug panel, or headless driver:
app.update();
let timings = app.world().resource::<FrameTimings>().unwrap();
if let Some(update) = timings.summary(FrameStage::Update) {
    println!("update p50={:?} p95={:?} p99={:?}", update.p50, update.p95, update.p99);
}
```

The application chooses how to display or export these values. RNDR does not
log them automatically, open a listener, or capture document/input content.
Without the resource, timing hooks perform resource lookups, but do not read a
clock or allocate timing storage. Enabled collection uses a mutex and bounded
rolling queues. Each stage and native frame history retains at most `capacity`
samples (default 240; zero clamps to one). Queries copy data; percentile queries
sort a temporary vector using nearest-rank percentiles, not interpolation.

## Labels and nesting

- `Update`: entire normal `App::update`, including startup/input/systems, layout,
  text, and final live/motion/window work.
- `Layout`: user Layout systems and hierarchy/list/markdown layout passes,
  including text measurement performed by those passes.
- `Text`: post-layout `apply_text_layout` and mono-grid preparation. This is not
  all text-related CPU time; layout measurement and paint projection also do text work.
- `LiveUpdate`: only `App::update_live`. Normal Update/Layout/Text samples are
  **not** fabricated on live-only frames.
- `Paint`: paint projection, excluding update and Render schedule systems.
- `Ingest`: glyph atlas upload and CPU image ingestion. Pending image collection
  is outside this stage; scene image GPU preparation is in Encode.
- `QuadUpload`: plain-quad instance encoding and buffer uploads before acquire.
  The scene/display-list path does not run this stage.
- `Acquire`: surface texture acquisition, including failed attempts. Surface
  reconfiguration after a lost/outdated acquisition is outside this stage.
- `Encode`: surface view creation and command encoding, including scene image
  upload preparation. The plain-quad path also has QuadUpload above.
- `SubmitPresent`: CPU queue submit through present return. With the `observe`
  feature actively capturing, this also includes synchronous observation readback;
  do not compare capture frames to normal interaction frames as equal workloads.
- `Redraw`: callback entry through completion of redraw handling, including
  other CPU stages and post-frame shell effects.
- `InputToRedraw`: oldest pending platform input callback receipt to redraw entry.
- `InputToPresent`: oldest pending input receipt to successful CPU present return
  (sampled immediately after the renderer returns to the runner).

These are **CPU wall-clock durations**, not GPU timestamps, OS event creation
latency, physical display completion, or input-to-photon latency. Nested labels
must not be summed. Failed attempts may have Update/Layout/Acquire samples but
no SubmitPresent sample. A shell that refuses frames before acquisition has no
Acquire sample. Headless drivers have core/paint samples but no native samples.

## Native lifecycle and input coalescing

`native_frames()` returns the bounded oldest-first native attempt history:
`presented`, `live_only`, `redraw`, and optional input latencies. Failed
presentation is explicit. `live_only` is true only if that path actually ran,
not when an occluded live update was skipped. Frames with no pending input have
`None` input latencies rather than misleading zeros.

Input starts at callback receipt for keyboard/modifiers/IME, pointer, wheel,
gesture and drag/drop events (or cursor/button/scroll/key events on layer shell).
Hover input can therefore be pending until a later redraw. Multiple events are
coalesced using the oldest pending receipt. Failed redraws retain it; only a
successful CPU present clears it. InputToRedraw can contain multiple attempts for
the same pending input; InputToPresent samples successful presentations only.
Resource removal discards pending input/history. Browser DOM-specific bridge
input outside the platform event callback, application-created input and global
hotkey/assistive action delivery are not separately timestamped by these hooks.

Native fields allow callers to distinguish failed/successful, input/no-input,
and live/full paths. Stage percentiles aggregate all attempts that ran that
stage; they are not per-native-frame traces and must not be aligned by index.
No cross-platform native latency or GPU performance claim follows from headless
unit tests alone.
