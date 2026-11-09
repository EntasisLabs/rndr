# rndr_media

Native video and audio playback for [rndr](https://github.com/EntasisLabs/rndr),
using the platform's own media stack:

| Platform | Playback | Recording |
| --- | --- | --- |
| Linux | GStreamer 1.x (`playbin`) | GStreamer: WAV, Ogg Opus, FLAC |
| macOS | AVFoundation (`AVPlayer`) | Not yet: reports `Failed` |
| Others | Players report `PlayerState::Failed` | Reports `Failed` |

A `Player` plays a file or URL. Video frames go to its `FrameSurface`; draw
them with `SurfaceView`. Audio clips play without a view.

```rust,no_run
use rndr_core::{SurfaceFit, SurfaceView};
use rndr_media::Player;

let clip = Player::file("intro.mp4"); // video starts muted
clip.set_looping(true);
clip.play();
let view = SurfaceView::new(clip.surface()).with_fit(SurfaceFit::Contain);

let tap = Player::audio("tap.ogg"); // audio starts unmuted
tap.play();
```

Controls: `play`, `pause`, `seek`, `set_rate`, `set_looping`, `set_muted`.
State: `state`, `time`, `duration`, `natural_size`, plus `position()` as a
`Live<f32>` for progress bars and `set_waker` for state changes.

A `Recorder` records the default input to a file; the extension picks the
format (`.wav`, `.ogg`/`.opus`, `.flac`). It has `start`, `pause`, `resume`,
`stop` (returns the file and its length, pauses excluded) and `cancel`
(deletes the file), plus `state`, `elapsed`, `level()` and `live_level()` as a
`Live<f32>` for a meter bar. `RecordSource::TestTone(hz)` records a generated
tone instead of a device. Run `cargo run -p rndr_media --example recorder --
[--tone] [take.ogg]`.

Linux builds need the GStreamer development packages
(`libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev` on Debian/Ubuntu).
At run time, playback needs `gstreamer1.0-plugins-base` and `-good`; H.264
and other patented formats need `gstreamer1.0-libav`.

Frames are copied from the decoder to the GPU; there is no zero-copy import
yet. On macOS, `AVPlayer` runs on the main queue, so the app's event loop must
be running for a player to load.

Run the example with `cargo run -p rndr_media --example player -- [file]`.
