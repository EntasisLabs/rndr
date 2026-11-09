//! Microphone recording to a file, with elapsed time and an input level.
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use rndr_core::Live;

#[cfg_attr(target_os = "linux", path = "rec_gstreamer.rs")]
#[cfg_attr(target_os = "macos", path = "rec_avfoundation.rs")]
#[cfg_attr(
    not(any(target_os = "linux", target_os = "macos")),
    path = "rec_unsupported.rs"
)]
mod backend;

/// The file format a [`Recorder`] writes, from the path's extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecordFormat {
    /// 16-bit PCM WAV (`.wav`).
    Wav,
    /// Opus in Ogg (`.ogg`, `.opus`), Linux.
    Opus,
    /// FLAC (`.flac`), Linux.
    Flac,
    /// AAC in MPEG-4 (`.m4a`), macOS.
    Aac,
}

impl RecordFormat {
    /// The extensions this platform records, for messages.
    #[cfg(target_os = "macos")]
    const SUPPORTED: &str = ".m4a or .wav";
    #[cfg(not(target_os = "macos"))]
    const SUPPORTED: &str = ".wav, .ogg, .opus or .flac";

    /// Format for `path`'s extension, or `None` when this platform can't
    /// write it: `.m4a` and `.wav` on macOS, `.wav`, `.ogg`, `.opus` and
    /// `.flac` elsewhere.
    pub fn for_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "wav" => Some(Self::Wav),
            #[cfg(target_os = "macos")]
            "m4a" => Some(Self::Aac),
            #[cfg(not(target_os = "macos"))]
            "ogg" | "opus" => Some(Self::Opus),
            #[cfg(not(target_os = "macos"))]
            "flac" => Some(Self::Flac),
            _ => None,
        }
    }
}

/// What a [`Recorder`] records.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum RecordSource {
    /// The system's default input.
    Microphone,
    /// A generated sine tone at this frequency in hertz, at half volume.
    /// Records like a microphone without a device, for tests and demos.
    TestTone(f32),
}

/// Where a [`Recorder`] is.
///
/// `Interrupted` and `Failed` messages start lower case and have no final
/// period, so they read after a colon.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecorderState {
    /// Not started, or cancelled.
    Idle,
    /// Opening the input.
    Starting,
    Recording,
    Paused,
    /// Stopped with the file written.
    Stopped,
    /// The input failed mid-recording. What was captured is kept;
    /// [`Recorder::stop`] returns it and [`Recorder::cancel`] deletes it.
    Interrupted(String),
    /// Couldn't record: no input, an unwritable path, an unsupported format
    /// or platform. [`Recorder::start`] tries again.
    Failed(String),
}

/// Microphone access, as the system reports it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MicPermission {
    Granted,
    /// The user said no; only the system settings can change it.
    Denied,
    /// Not asked yet.
    Undetermined,
    /// The system won't allow recording, or this platform can't record.
    Restricted,
}

/// Input level of the last few milliseconds, linear `0.0..=1.0` of full
/// scale, loudest channel.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Level {
    /// Highest sample.
    pub peak: f32,
    /// Root mean square: how loud it sounds.
    pub rms: f32,
}

impl Level {
    /// `rms` on a −60 dB to 0 dB scale mapped to `0.0..=1.0`, for a meter bar.
    pub fn meter(&self) -> f32 {
        if self.rms.is_nan() || self.rms <= 0.0 {
            return 0.0;
        }
        ((20.0 * self.rms.log10() + 60.0) / 60.0).clamp(0.0, 1.0)
    }
}

/// A finished recording.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Recording {
    /// The file, ready for [`Player::audio`](crate::Player::audio).
    pub path: PathBuf,
    /// Seconds of audio, pauses excluded.
    pub duration: f64,
}

// The unsupported backend fails at spawn and never reads commands.
#[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
pub(crate) enum Command {
    Start,
    Pause,
    Resume,
    Stop(Sender<()>),
    Cancel(Sender<()>),
}

#[derive(Clone, Debug)]
pub(crate) struct Status {
    pub state: RecorderState,
    pub elapsed: f64,
    pub level: Level,
}

type Waker = Arc<dyn Fn() + Send + Sync>;

pub(crate) struct Shared {
    pub path: PathBuf,
    #[cfg_attr(not(any(target_os = "linux", target_os = "macos")), allow(dead_code))]
    pub source: RecordSource,
    pub level: Live<f32>,
    status: Mutex<Status>,
    waker: Mutex<Option<Waker>>,
}

impl Shared {
    pub fn status(&self) -> MutexGuard<'_, Status> {
        self.status.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Apply `edit`; call the waker when the state changed.
    pub fn update(&self, edit: impl FnOnce(&mut Status)) {
        let (changed, meter) = {
            let mut status = self.status();
            let before = status.state.clone();
            edit(&mut status);
            (status.state != before, status.level.meter())
        };
        self.level.set(meter);
        if changed {
            let waker = self.waker.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let Some(waker) = waker {
                waker();
            }
        }
    }

    pub fn fail(&self, error: impl Into<String>) {
        let error = error.into();
        self.update(|s| {
            s.state = match s.state {
                RecorderState::Recording | RecorderState::Paused => {
                    RecorderState::Interrupted(error)
                }
                _ => RecorderState::Failed(error),
            };
            s.level = Level::default();
        });
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
            if worker.thread().id() != std::thread::current().id() {
                let _ = worker.join();
            }
        }
    }
}

/// Records audio to a file. Clones share one recorder; dropping the last
/// clone while recording stops it and keeps the file.
///
/// On Linux it records with GStreamer from the default input (PulseAudio,
/// PipeWire or ALSA), and writes WAV, Ogg Opus or FLAC by the path's
/// extension. On macOS it records with `AVAudioRecorder` to AAC (`.m4a`) or
/// WAV, asking for microphone access first when the user hasn't been asked.
/// Other platforms report [`RecorderState::Failed`].
///
/// ```no_run
/// use rndr_media::Recorder;
///
/// let rec = Recorder::to_file(if cfg!(target_os = "macos") { "note.m4a" } else { "note.ogg" });
/// rec.start();
/// // A meter bar that redraws without running Update:
/// // world.insert(bar, rndr_core::LiveScale::new(&...)) from `rec.live_level()`.
/// let note = rec.stop()?;
/// rndr_media::Player::audio(note.path).play();
/// # Ok::<(), String>(())
/// ```
#[derive(Clone)]
pub struct Recorder {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Recorder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = self.shared().status().clone();
        f.debug_struct("Recorder")
            .field("path", &self.shared().path)
            .field("state", &status.state)
            .field("elapsed", &status.elapsed)
            .finish_non_exhaustive()
    }
}

impl Recorder {
    /// Record the default input to `path`. Nothing opens until
    /// [`start`](Self::start).
    pub fn to_file(path: impl Into<PathBuf>) -> Self {
        Self::with_source(path, RecordSource::Microphone)
    }

    /// Record `source` to `path`.
    pub fn with_source(path: impl Into<PathBuf>, source: RecordSource) -> Self {
        let shared = Arc::new(Shared {
            path: path.into(),
            source,
            level: Live::new(0.0),
            status: Mutex::new(Status {
                state: RecorderState::Idle,
                elapsed: 0.0,
                level: Level::default(),
            }),
            waker: Mutex::new(None),
        });
        Self {
            inner: Arc::new(Inner {
                shared,
                commands: Mutex::new(None),
                worker: Mutex::new(None),
            }),
        }
    }

    fn shared(&self) -> &Shared {
        &self.inner.shared
    }

    fn send(&self, command: Command) -> bool {
        self.inner
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .is_some_and(|tx| tx.send(command).is_ok())
    }

    /// Microphone access, without asking. macOS reports what the user
    /// chose for this app (or the terminal that ran it); Linux has no
    /// per-app permission and reports [`MicPermission::Granted`];
    /// unsupported platforms report [`MicPermission::Restricted`].
    pub fn permission() -> MicPermission {
        backend::permission()
    }

    /// Ask for microphone access when it is
    /// [`Undetermined`](MicPermission::Undetermined), then call `callback`
    /// with the answer, from any thread; at once when already decided.
    /// macOS shows its prompt with the app's `NSMicrophoneUsageDescription`;
    /// a bundled app without one gets [`MicPermission::Denied`] rather than
    /// being ended by macOS. [`start`](Self::start) asks by itself.
    pub fn request_permission(callback: impl FnOnce(MicPermission) + Send + 'static) {
        backend::request_permission(Box::new(callback));
    }

    /// Start recording, overwriting the file. On macOS, a microphone
    /// recording asks for access first when it is undetermined (the state
    /// stays [`Starting`](RecorderState::Starting) meanwhile) and fails when
    /// it is denied, or when a bundled app's Info.plist has no
    /// `NSMicrophoneUsageDescription`. Restarts after
    /// [`stop`](Self::stop), [`cancel`](Self::cancel) or a failure, and
    /// resumes when paused.
    pub fn start(&self) {
        match self.state() {
            RecorderState::Recording | RecorderState::Starting => return,
            RecorderState::Paused => return self.resume(),
            _ => {}
        }
        if RecordFormat::for_path(&self.shared().path).is_none() {
            let shown = self.shared().path.display().to_string();
            self.shared().update(|s| {
                s.state = RecorderState::Failed(format!(
                    "can't record to {shown}: use {}",
                    RecordFormat::SUPPORTED
                ))
            });
            return;
        }
        self.finish_worker();
        self.shared().update(|s| {
            s.state = RecorderState::Starting;
            s.elapsed = 0.0;
            s.level = Level::default();
        });
        let (tx, rx): (Sender<Command>, Receiver<Command>) = mpsc::channel();
        let worker = backend::spawn(self.inner.shared.clone(), rx);
        *self
            .inner
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(tx);
        *self.inner.worker.lock().unwrap_or_else(|e| e.into_inner()) = worker;
        self.send(Command::Start);
    }

    pub fn pause(&self) {
        if self.state() == RecorderState::Recording {
            self.shared().update(|s| s.state = RecorderState::Paused);
            self.send(Command::Pause);
        }
    }

    pub fn resume(&self) {
        if self.state() == RecorderState::Paused {
            self.shared().update(|s| s.state = RecorderState::Recording);
            self.send(Command::Resume);
        }
    }

    /// Finish the file and return it. Blocks until the encoder has written
    /// its last block, normally a few milliseconds.
    pub fn stop(&self) -> Result<Recording, String> {
        match self.state() {
            RecorderState::Recording | RecorderState::Paused | RecorderState::Starting => {
                let (tx, rx) = mpsc::channel();
                if self.send(Command::Stop(tx)) {
                    let _ = rx.recv_timeout(Duration::from_secs(10));
                }
                self.finish_worker();
            }
            RecorderState::Interrupted(_) => self.finish_worker(),
            _ => {}
        }
        match self.state() {
            RecorderState::Stopped | RecorderState::Interrupted(_) => Ok(Recording {
                path: self.shared().path.clone(),
                duration: self.elapsed(),
            }),
            RecorderState::Failed(error) => Err(error),
            _ => Err("nothing was recorded".into()),
        }
    }

    /// Stop and delete the file.
    pub fn cancel(&self) {
        let (tx, rx) = mpsc::channel();
        if self.send(Command::Cancel(tx)) {
            let _ = rx.recv_timeout(Duration::from_secs(10));
        }
        self.finish_worker();
        if !matches!(self.state(), RecorderState::Idle | RecorderState::Failed(_)) {
            let _ = std::fs::remove_file(&self.shared().path);
        }
        self.shared().update(|s| {
            s.state = RecorderState::Idle;
            s.elapsed = 0.0;
            s.level = Level::default();
        });
    }

    fn finish_worker(&self) {
        self.inner
            .commands
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let worker = self
            .inner
            .worker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(worker) = worker {
            if worker.thread().id() != std::thread::current().id() {
                let _ = worker.join();
            }
        }
    }

    /// Called from the recorder's thread when the state changes. Not called
    /// for the level or elapsed time.
    pub fn set_waker(&self, waker: impl Fn() + Send + Sync + 'static) {
        *self
            .shared()
            .waker
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(waker));
    }

    pub fn state(&self) -> RecorderState {
        self.shared().status().state.clone()
    }

    pub fn is_recording(&self) -> bool {
        self.state() == RecorderState::Recording
    }

    /// Seconds recorded so far, pauses excluded.
    pub fn elapsed(&self) -> f64 {
        self.shared().status().elapsed
    }

    /// Input level, updated about 30 times a second while recording.
    pub fn level(&self) -> Level {
        self.shared().status().level
    }

    /// [`Level::meter`] as a [`Live`] value. Bind it to a meter bar, for
    /// example through [`rndr_core::LiveScale`], to redraw without polling.
    pub fn live_level(&self) -> Live<f32> {
        self.shared().level.clone()
    }

    /// The file this recorder writes.
    pub fn path(&self) -> &Path {
        &self.shared().path
    }
}
