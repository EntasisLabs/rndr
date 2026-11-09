//! AVFoundation recording. A worker thread owns an `AVAudioRecorder`, which
//! captures the default input, encodes AAC or writes WAV, and meters it.
//! `RecordSource::TestTone` writes a generated sine through an `AVAudioFile`
//! with the same settings, paced in real time, so it needs no device or
//! permission. `AVCaptureDevice` reports and asks for microphone access.
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyObject, Bool};
use objc2::AnyThread;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use objc2_avf_audio::{
    AVAudioFile, AVAudioPCMBuffer, AVAudioQuality, AVAudioRecorder, AVEncoderAudioQualityKey,
    AVFormatIDKey, AVLinearPCMBitDepthKey, AVLinearPCMIsBigEndianKey, AVLinearPCMIsFloatKey,
    AVNumberOfChannelsKey, AVSampleRateKey,
};
use objc2_foundation::{NSBundle, NSDictionary, NSError, NSNumber, NSString, NSURL};

use super::{Command, Level, MicPermission, RecordFormat, RecordSource, RecorderState, Shared};

const TICK: Duration = Duration::from_millis(33);
const SAMPLE_RATE: f64 = 44_100.0;
const DENIED: &str =
    "microphone access is denied: allow it in System Settings > Privacy & Security > Microphone";
const RESTRICTED: &str = "microphone access is restricted on this Mac";

pub(crate) fn permission() -> MicPermission {
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return MicPermission::Restricted;
    };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::Authorized => MicPermission::Granted,
        AVAuthorizationStatus::Denied => MicPermission::Denied,
        AVAuthorizationStatus::Restricted => MicPermission::Restricted,
        _ => MicPermission::Undetermined,
    }
}

pub(crate) fn request_permission(callback: Box<dyn FnOnce(MicPermission) + Send>) {
    if preflight().is_err() {
        return callback(MicPermission::Denied);
    }
    let current = permission();
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else {
        return callback(current);
    };
    if current != MicPermission::Undetermined {
        return callback(current);
    }
    let callback = Mutex::new(Some(callback));
    let block = RcBlock::new(move |granted: Bool| {
        let callback = callback.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(callback) = callback {
            callback(if granted.as_bool() {
                MicPermission::Granted
            } else {
                MicPermission::Denied
            });
        }
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block) };
}

/// macOS ends a bundled app that opens the microphone without a usage
/// description; refuse instead. An unbundled binary is attributed to the
/// terminal that ran it.
fn preflight() -> Result<(), String> {
    let bundle = NSBundle::mainBundle();
    if !bundle.bundlePath().to_string().ends_with(".app") {
        return Ok(());
    }
    let key = NSString::from_str("NSMicrophoneUsageDescription");
    match bundle.objectForInfoDictionaryKey(&key) {
        Some(_) => Ok(()),
        None => Err(
            "the app's Info.plist has no NSMicrophoneUsageDescription, so macOS won't let it record"
                .into(),
        ),
    }
}

pub(crate) fn spawn(shared: Arc<Shared>, commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    let spawned = std::thread::Builder::new()
        .name("rndr-recorder".into())
        .spawn({
            let shared = shared.clone();
            move || run(&shared, &commands)
        });
    match spawned {
        Ok(worker) => Some(worker),
        Err(error) => {
            shared.fail(format!("could not start the recorder thread: {error}"));
            None
        }
    }
}

fn run(shared: &Shared, commands: &Receiver<Command>) {
    let opened = match shared.source {
        RecordSource::Microphone => microphone(shared, commands).map(|r| {
            r.map(|recorder| Input::Mic {
                recorder,
                recorded: 0.0,
            })
        }),
        RecordSource::TestTone(hz) => autoreleasepool(|_| Tone::open(&shared.path, hz))
            .map(|tone| Some(Input::Tone(Box::new(tone)))),
    };
    let mut input = match opened {
        Ok(Some(input)) => input,
        Ok(None) => return,
        Err(error) => return shared.fail(error),
    };
    shared.update(|s| s.state = RecorderState::Recording);
    let mut paused = false;
    loop {
        let command = commands.recv_timeout(TICK);
        let (end, reply) = autoreleasepool(|_| match command {
            Ok(Command::Start | Command::Resume) => {
                if paused {
                    input.resume();
                    paused = false;
                }
                (None, None)
            }
            Ok(Command::Pause) => {
                input.pause();
                paused = true;
                let elapsed = input.elapsed();
                shared.update(|s| {
                    s.elapsed = elapsed;
                    s.level = Level::default();
                });
                (None, None)
            }
            Ok(Command::Stop(done)) => {
                input.finish();
                (Some(End::Stopped), Some(done))
            }
            Ok(Command::Cancel(done)) => {
                input.delete();
                (Some(End::Cancelled), Some(done))
            }
            // The last handle went away: keep what was recorded.
            Err(RecvTimeoutError::Disconnected) => {
                input.finish();
                (Some(End::Stopped), None)
            }
            Err(RecvTimeoutError::Timeout) if paused => (None, None),
            Err(RecvTimeoutError::Timeout) => match input.tick() {
                Ok(level) => {
                    let elapsed = input.elapsed();
                    shared.update(|s| {
                        s.elapsed = elapsed;
                        s.level = level;
                    });
                    (None, None)
                }
                Err(error) => {
                    input.finish();
                    (Some(End::Lost(error)), None)
                }
            },
        });
        let Some(end) = end else { continue };
        // The pool has drained, so the file is closed before anyone hears.
        let elapsed = input.elapsed();
        drop(input);
        match end {
            End::Stopped => shared.update(|s| {
                s.elapsed = elapsed;
                s.level = Level::default();
                if !matches!(
                    s.state,
                    RecorderState::Failed(_) | RecorderState::Interrupted(_)
                ) {
                    s.state = RecorderState::Stopped;
                }
            }),
            End::Cancelled => {}
            End::Lost(error) => {
                shared.update(|s| s.elapsed = elapsed);
                shared.fail(error);
            }
        }
        if let Some(done) = reply {
            let _ = done.send(());
        }
        return;
    }
}

enum End {
    Stopped,
    Cancelled,
    Lost(String),
}

/// Checks access (asking when undetermined), then starts an
/// `AVAudioRecorder`. `None` when stopped or dropped while the prompt is up.
fn microphone(
    shared: &Shared,
    commands: &Receiver<Command>,
) -> Result<Option<Retained<AVAudioRecorder>>, String> {
    preflight()?;
    let mut answer = permission();
    if answer == MicPermission::Undetermined {
        let (tx, rx) = mpsc::channel();
        request_permission(Box::new(move |answer| {
            let _ = tx.send(answer);
        }));
        answer = loop {
            match rx.recv_timeout(TICK) {
                Ok(answer) => break answer,
                Err(RecvTimeoutError::Disconnected) => break MicPermission::Denied,
                Err(RecvTimeoutError::Timeout) => {}
            }
            match commands.try_recv() {
                Ok(Command::Stop(done) | Command::Cancel(done)) => {
                    shared.update(|s| s.state = RecorderState::Idle);
                    let _ = done.send(());
                    return Ok(None);
                }
                Err(TryRecvError::Disconnected) => return Ok(None),
                _ => {}
            }
        };
    }
    match answer {
        MicPermission::Granted => {}
        MicPermission::Denied | MicPermission::Undetermined => return Err(DENIED.into()),
        MicPermission::Restricted => return Err(RESTRICTED.into()),
    }
    autoreleasepool(|_| {
        let audio = unsafe { AVMediaTypeAudio }.ok_or(RESTRICTED)?;
        if unsafe { AVCaptureDevice::defaultDeviceWithMediaType(audio) }.is_none() {
            return Err("no microphone is connected".to_string());
        }
        let path = &shared.path;
        let settings = settings(path)?;
        let recorder = unsafe {
            AVAudioRecorder::initWithURL_settings_error(
                AVAudioRecorder::alloc(),
                &file_url(path),
                &settings,
            )
        }
        .map_err(|e| cannot_record(path, &e))?;
        unsafe {
            recorder.setMeteringEnabled(true);
            if !recorder.prepareToRecord() {
                return Err(format!("couldn't create {}", path.display()));
            }
            if !recorder.record() {
                recorder.deleteRecording();
                return Err("couldn't start recording from the microphone".into());
            }
        }
        Ok(Some(recorder))
    })
}

enum Input {
    /// `recorded` keeps the length, which `currentTime` forgets on stop.
    Mic {
        recorder: Retained<AVAudioRecorder>,
        recorded: f64,
    },
    Tone(Box<Tone>),
}

impl Input {
    fn tick(&mut self) -> Result<Level, String> {
        match self {
            Self::Mic { recorder, recorded } => unsafe {
                if !recorder.isRecording() {
                    return Err("recording stopped: the input device went away".into());
                }
                *recorded = recorded.max(recorder.currentTime());
                recorder.updateMeters();
                Ok(Level {
                    peak: linear(recorder.peakPowerForChannel(0)),
                    rms: linear(recorder.averagePowerForChannel(0)),
                })
            },
            Self::Tone(tone) => tone.write(),
        }
    }

    /// Seconds recorded, pauses excluded.
    fn elapsed(&self) -> f64 {
        match self {
            Self::Mic { recorded, .. } => *recorded,
            Self::Tone(tone) => tone.written as f64 / tone.rate,
        }
    }

    fn pause(&mut self) {
        match self {
            Self::Mic { recorder, recorded } => unsafe {
                *recorded = recorded.max(recorder.currentTime());
                recorder.pause();
            },
            Self::Tone(tone) => {
                let _ = tone.write();
                tone.since = None;
            }
        }
    }

    fn resume(&mut self) {
        match self {
            Self::Mic { recorder, .. } => unsafe {
                recorder.record();
            },
            Self::Tone(tone) => tone.since = Some((Instant::now(), tone.written)),
        }
    }

    /// Write the rest and close the file.
    fn finish(&mut self) {
        match self {
            Self::Mic { recorder, recorded } => unsafe {
                *recorded = recorded.max(recorder.currentTime());
                recorder.stop();
            },
            Self::Tone(tone) => {
                if tone.since.is_some() {
                    let _ = tone.write();
                }
                tone.file = None;
            }
        }
    }

    fn delete(&mut self) {
        match self {
            Self::Mic { recorder, .. } => unsafe {
                recorder.stop();
                recorder.deleteRecording();
            },
            Self::Tone(tone) => {
                tone.file = None;
                let _ = std::fs::remove_file(&tone.path);
            }
        }
    }
}

/// A generated sine written to an `AVAudioFile` as fast as real time.
struct Tone {
    path: std::path::PathBuf,
    file: Option<Retained<AVAudioFile>>,
    hz: f64,
    rate: f64,
    written: u64,
    /// When the current run started, and the frames written before it.
    since: Option<(Instant, u64)>,
    level: Level,
}

impl Tone {
    fn open(path: &Path, hz: f32) -> Result<Self, String> {
        let settings = settings(path)?;
        let file = unsafe {
            AVAudioFile::initForWriting_settings_error(
                AVAudioFile::alloc(),
                &file_url(path),
                &settings,
            )
        }
        .map_err(|e| cannot_record(path, &e))?;
        let rate = unsafe { file.processingFormat().sampleRate() };
        Ok(Self {
            path: path.to_path_buf(),
            file: Some(file),
            hz: f64::from(hz),
            rate,
            written: 0,
            since: Some((Instant::now(), 0)),
            level: Level::default(),
        })
    }

    /// Write the frames due since the last call; the level of what was
    /// written, or the last level when nothing was due.
    fn write(&mut self) -> Result<Level, String> {
        let (Some(file), Some((start, base))) = (&self.file, self.since) else {
            return Ok(self.level);
        };
        let due = base + (start.elapsed().as_secs_f64() * self.rate) as u64;
        let frames = due.saturating_sub(self.written).min(self.rate as u64) as u32;
        if frames == 0 {
            return Ok(self.level);
        }
        let format = unsafe { file.processingFormat() };
        let buffer = unsafe {
            AVAudioPCMBuffer::initWithPCMFormat_frameCapacity(
                AVAudioPCMBuffer::alloc(),
                &format,
                frames,
            )
        }
        .ok_or("couldn't allocate an audio buffer")?;
        let channels = unsafe { format.channelCount() } as usize;
        let (mut peak, mut sum) = (0f32, 0f64);
        unsafe {
            buffer.setFrameLength(frames);
            let data = buffer.floatChannelData();
            for channel in 0..channels {
                let samples =
                    std::slice::from_raw_parts_mut((*data.add(channel)).as_ptr(), frames as usize);
                for (i, sample) in samples.iter_mut().enumerate() {
                    let t = (self.written + i as u64) as f64 / self.rate;
                    *sample = (0.5 * (std::f64::consts::TAU * self.hz * t).sin()) as f32;
                    if channel == 0 {
                        peak = peak.max(sample.abs());
                        sum += f64::from(*sample) * f64::from(*sample);
                    }
                }
            }
            file.writeFromBuffer_error(&buffer)
                .map_err(|e| format!("couldn't write {}: {}", self.path.display(), describe(&e)))?;
        }
        self.written += u64::from(frames);
        self.level = Level {
            peak,
            rms: (sum / f64::from(frames)).sqrt() as f32,
        };
        Ok(self.level)
    }
}

/// `AVAudioRecorder` settings for `path`'s format: mono AAC or 16-bit PCM.
fn settings(path: &Path) -> Result<Retained<NSDictionary<NSString, AnyObject>>, String> {
    let mut entries = vec![
        (unsafe { AVSampleRateKey }, NSNumber::new_f64(SAMPLE_RATE)),
        (unsafe { AVNumberOfChannelsKey }, NSNumber::new_u32(1)),
    ];
    match RecordFormat::for_path(path) {
        Some(RecordFormat::Aac) => entries.extend([
            (
                unsafe { AVFormatIDKey },
                NSNumber::new_u32(u32::from_be_bytes(*b"aac ")),
            ),
            (
                unsafe { AVEncoderAudioQualityKey },
                NSNumber::new_isize(AVAudioQuality::High.0),
            ),
        ]),
        Some(RecordFormat::Wav) => entries.extend([
            (
                unsafe { AVFormatIDKey },
                NSNumber::new_u32(u32::from_be_bytes(*b"lpcm")),
            ),
            (unsafe { AVLinearPCMBitDepthKey }, NSNumber::new_u32(16)),
            (unsafe { AVLinearPCMIsFloatKey }, NSNumber::new_bool(false)),
            (
                unsafe { AVLinearPCMIsBigEndianKey },
                NSNumber::new_bool(false),
            ),
        ]),
        _ => return Err("unsupported file type".into()),
    }
    let mut keys = Vec::with_capacity(entries.len());
    for (key, _) in &entries {
        keys.push(key.ok_or("AVFAudio's settings keys are unavailable")?);
    }
    let values: Vec<&AnyObject> = entries.iter().map(|(_, value)| value.as_ref()).collect();
    Ok(NSDictionary::from_slices(&keys, &values))
}

fn file_url(path: &Path) -> Retained<NSURL> {
    let path = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

fn cannot_record(path: &Path, error: &NSError) -> String {
    format!("couldn't record to {}: {}", path.display(), describe(error))
}

/// The error's description, lower case and without a final period.
fn describe(error: &NSError) -> String {
    let text = error.localizedDescription().to_string();
    let mut chars = text.chars();
    let text = match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect::<String>(),
        None => text,
    };
    text.trim_end_matches('.').to_string()
}

/// dBFS as AVAudioRecorder reports it (−160 is silence) to linear `0.0..=1.0`.
fn linear(db: f32) -> f32 {
    if db.is_finite() && db > -160.0 {
        10f32.powf(db / 20.0).clamp(0.0, 1.0)
    } else {
        0.0
    }
}
