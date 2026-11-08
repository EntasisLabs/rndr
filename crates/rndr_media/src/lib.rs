//! Native video and audio playback for rndr.
//!
//! A [`Player`] plays a file or URL with the platform's own media stack:
//! GStreamer on Linux and AVFoundation on macOS. Video frames go to the
//! player's [`FrameSurface`]; draw them with a
//! [`SurfaceView`](rndr_core::SurfaceView). Audio plays without a view.
//! On other platforms a player reports [`PlayerState::Failed`].
//!
//! ```no_run
//! use rndr_core::{SurfaceFit, SurfaceView};
//! use rndr_media::Player;
//!
//! let clip = Player::file("intro.mp4");
//! clip.set_looping(true);
//! clip.play();
//! let view = SurfaceView::new(clip.surface()).with_fit(SurfaceFit::Contain);
//! # let _ = view;
//!
//! let tap = Player::audio("tap.ogg");
//! tap.play();
//! ```
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use rndr_core::{FrameSurface, Live};

#[cfg_attr(target_os = "macos", path = "avfoundation.rs")]
#[cfg_attr(target_os = "linux", path = "gstreamer.rs")]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos")),
    path = "unsupported.rs"
)]
mod backend;

/// Where a [`Player`] reads its clip.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaSource {
    File(PathBuf),
    /// Any URL the platform player accepts, such as `https://` or `file://`.
    Url(String),
}

impl From<&str> for MediaSource {
    fn from(s: &str) -> Self {
        Self::from(s.to_owned())
    }
}

impl From<String> for MediaSource {
    fn from(s: String) -> Self {
        if s.contains("://") {
            Self::Url(s)
        } else {
            Self::File(s.into())
        }
    }
}

impl From<PathBuf> for MediaSource {
    fn from(path: PathBuf) -> Self {
        Self::File(path)
    }
}

impl From<&PathBuf> for MediaSource {
    fn from(path: &PathBuf) -> Self {
        Self::File(path.clone())
    }
}

impl From<&Path> for MediaSource {
    fn from(path: &Path) -> Self {
        Self::File(path.into())
    }
}

/// Playback state.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum PlayerState {
    /// Opening the clip and, for video, decoding its first frame.
    Loading,
    /// Loaded and paused.
    Ready,
    Playing,
    /// Reached the end without looping. [`Player::play`] starts again from
    /// the beginning.
    Ended,
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Video,
    Audio,
}

pub(crate) enum Command {
    Play,
    Pause,
    Seek(f64),
    Rate(f32),
    Muted(bool),
}

/// What the backend last reported, plus the requested play/pause intent.
#[derive(Clone, Debug)]
pub(crate) struct Status {
    pub loaded: bool,
    pub playing: bool,
    pub ended: bool,
    pub failed: Option<String>,
    pub time: f64,
    pub duration: Option<f64>,
    pub rate: f32,
    pub muted: bool,
}

impl Status {
    fn state(&self) -> PlayerState {
        if let Some(error) = &self.failed {
            PlayerState::Failed(error.clone())
        } else if self.ended {
            PlayerState::Ended
        } else if self.playing {
            PlayerState::Playing
        } else if self.loaded {
            PlayerState::Ready
        } else {
            PlayerState::Loading
        }
    }
}

type Waker = Arc<dyn Fn() + Send + Sync>;

/// State shared by the handles and the backend's worker thread.
pub(crate) struct Shared {
    pub kind: Kind,
    pub source: MediaSource,
    pub surface: FrameSurface,
    pub position: Live<f32>,
    pub looping: AtomicBool,
    status: Mutex<Status>,
    waker: Mutex<Option<Waker>>,
}

impl Shared {
    pub fn status(&self) -> MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Apply `edit`, publish the time, and call the waker if the state or
    /// duration changed.
    pub fn update(&self, edit: impl FnOnce(&mut Status)) {
        let (changed, time) = {
            let mut status = self.status();
            let before = (status.state(), status.duration);
            edit(&mut status);
            ((status.state(), status.duration) != before, status.time)
        };
        self.position.set(time as f32);
        if changed {
            self.wake();
        }
    }

    pub fn fail(&self, error: impl Into<String>) {
        let error = error.into();
        self.update(|s| {
            s.failed.get_or_insert(error);
            s.playing = false;
        });
    }

    fn wake(&self) {
        let waker = self.waker.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(waker) = waker {
            waker();
        }
    }
}

struct Inner {
    shared: Arc<Shared>,
    commands: Mutex<Option<Sender<Command>>>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(worker) = self.worker.lock().unwrap_or_else(|e| e.into_inner()).take() {
            let _ = worker.join();
        }
    }
}

/// A video or audio clip. Clones share one player; dropping the last clone
/// stops it and frees its decoder. Video starts muted and audio unmuted. The
/// clip opens right away, and a video's first frame appears in its surface
/// before playback starts.
///
/// Controls take effect asynchronously on the player's worker thread.
/// [`state`](Self::state) reflects [`play`](Self::play) and
/// [`pause`](Self::pause) at once; everything else is as last reported by
/// the platform player.
#[derive(Clone)]
pub struct Player {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = self.inner.shared.status().clone();
        f.debug_struct("Player")
            .field("kind", &self.inner.shared.kind)
            .field("source", &self.inner.shared.source)
            .field("state", &status.state())
            .field("time", &status.time)
            .finish_non_exhaustive()
    }
}

impl Player {
    /// A video, muted.
    pub fn new(source: impl Into<MediaSource>) -> Self {
        Self::create(source.into(), Kind::Video)
    }

    /// A video file, muted.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::new(MediaSource::File(path.into()))
    }

    /// A video URL, muted.
    pub fn url(url: impl Into<String>) -> Self {
        Self::new(MediaSource::Url(url.into()))
    }

    /// An audio clip, unmuted. Any video track is ignored.
    pub fn audio(source: impl Into<MediaSource>) -> Self {
        Self::create(source.into(), Kind::Audio)
    }

    fn create(source: MediaSource, kind: Kind) -> Self {
        let shared = Arc::new(Shared {
            kind,
            source,
            surface: FrameSurface::new(),
            position: Live::new(0.0),
            looping: AtomicBool::new(false),
            status: Mutex::new(Status {
                loaded: false,
                playing: false,
                ended: false,
                failed: None,
                time: 0.0,
                duration: None,
                rate: 1.0,
                muted: kind == Kind::Video,
            }),
            waker: Mutex::new(None),
        });
        let (tx, rx): (Sender<Command>, Receiver<Command>) = mpsc::channel();
        let worker = backend::spawn(shared.clone(), rx);
        Self {
            inner: Arc::new(Inner {
                shared,
                commands: Mutex::new(Some(tx)),
                worker: Mutex::new(worker),
            }),
        }
    }

    fn send(&self, command: Command) {
        if let Some(tx) = &*self
            .inner
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
        {
            let _ = tx.send(command);
        }
    }

    fn shared(&self) -> &Shared {
        &self.inner.shared
    }

    pub fn source(&self) -> &MediaSource {
        &self.shared().source
    }

    pub fn is_audio(&self) -> bool {
        self.shared().kind == Kind::Audio
    }

    /// Start or resume playback. After [`PlayerState::Ended`] it starts from
    /// the beginning.
    pub fn play(&self) {
        let mut ignored = false;
        self.shared().update(|s| {
            ignored = s.failed.is_some();
            if !ignored {
                if s.ended {
                    s.time = 0.0;
                }
                s.ended = false;
                s.playing = true;
            }
        });
        if !ignored {
            self.send(Command::Play);
        }
    }

    pub fn pause(&self) {
        self.shared().update(|s| s.playing = false);
        self.send(Command::Pause);
    }

    /// Jump to `seconds`, clamped to the clip. Keeps playing if it was
    /// playing.
    pub fn seek(&self, seconds: f64) {
        let seconds = if seconds.is_finite() {
            seconds.max(0.0)
        } else {
            0.0
        };
        let mut to = seconds;
        self.shared().update(|s| {
            to = s.duration.map_or(seconds, |d| seconds.min(d));
            s.time = to;
            s.ended = false;
        });
        self.send(Command::Seek(to));
    }

    /// Playback speed; 1 is normal. Zero, negative and non-finite rates are
    /// ignored.
    pub fn set_rate(&self, rate: f32) {
        if rate.is_finite() && rate > 0.0 {
            self.shared().update(|s| s.rate = rate);
            self.send(Command::Rate(rate));
        }
    }

    /// Restart from the beginning at the end instead of ending.
    pub fn set_looping(&self, looping: bool) {
        self.shared().looping.store(looping, Ordering::Relaxed);
    }

    pub fn set_muted(&self, muted: bool) {
        self.shared().update(|s| s.muted = muted);
        self.send(Command::Muted(muted));
    }

    /// Called from the player's thread when the state or duration changes:
    /// loaded, ended, failed, or played and paused. It is not called for
    /// progress; use [`position`](Self::position) for that.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *self
            .shared()
            .waker
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(waker));
    }

    pub fn state(&self) -> PlayerState {
        self.shared().status().state()
    }

    pub fn is_playing(&self) -> bool {
        self.state() == PlayerState::Playing
    }

    /// Current time in seconds, updated about 20 times a second while
    /// playing.
    pub fn time(&self) -> f64 {
        self.shared().status().time
    }

    /// Current time in seconds as a [`Live`] value. It wakes windows that
    /// read it, so progress redraws without polling.
    pub fn position(&self) -> Live<f32> {
        self.shared().position.clone()
    }

    /// Clip length in seconds, once known. `None` for live streams.
    pub fn duration(&self) -> Option<f64> {
        self.shared().status().duration
    }

    /// Video size in pixels, once the first frame has decoded.
    pub fn natural_size(&self) -> Option<[u32; 2]> {
        self.shared().surface.size()
    }

    pub fn rate(&self) -> f32 {
        self.shared().status().rate
    }

    pub fn looping(&self) -> bool {
        self.shared().looping.load(Ordering::Relaxed)
    }

    pub fn muted(&self) -> bool {
        self.shared().status().muted
    }

    /// Where video frames go. Draw it with a
    /// [`SurfaceView`](rndr_core::SurfaceView). Audio clips leave it empty.
    pub fn surface(&self) -> &FrameSurface {
        &self.shared().surface
    }
}
