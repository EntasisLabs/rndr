//! GStreamer recording: `source ! audioconvert ! audioresample ! level !
//! encoder ! filesink`. A worker thread owns the pipeline.
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use gst::prelude::*;

use super::{Command, Level, MicPermission, RecordFormat, RecordSource, RecorderState, Shared};

const TICK: Duration = Duration::from_millis(20);

pub(crate) fn permission() -> MicPermission {
    MicPermission::Granted
}

pub(crate) fn spawn(shared: Arc<Shared>, commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    let spawned = std::thread::Builder::new()
        .name("rndr-recorder".into())
        .spawn({
            let shared = shared.clone();
            move || match Session::open(shared.clone()) {
                Ok(session) => session.run(&commands),
                Err(error) => shared.fail(error),
            }
        });
    match spawned {
        Ok(worker) => Some(worker),
        Err(error) => {
            shared.fail(format!("could not start the recorder thread: {error}"));
            None
        }
    }
}

struct Session {
    shared: Arc<Shared>,
    pipeline: gst::Pipeline,
    bus: gst::Bus,
    /// Recording time before the current run, and when that run started.
    banked: f64,
    since: Option<Instant>,
}

impl Session {
    fn open(shared: Arc<Shared>) -> Result<Self, String> {
        gst::init().map_err(|e| format!("GStreamer is unavailable: {e}"))?;
        let format = RecordFormat::for_path(&shared.path).ok_or("unsupported file type")?;
        let source = match shared.source {
            RecordSource::Microphone => "autoaudiosrc".to_string(),
            RecordSource::TestTone(hz) => {
                format!("audiotestsrc is-live=true wave=sine volume=0.5 freq={hz}")
            }
        };
        let encoder = match format {
            RecordFormat::Wav => "wavenc",
            RecordFormat::Opus => "opusenc ! oggmux",
            RecordFormat::Flac => "flacenc",
        };
        let description = format!(
            "{source} ! audioconvert ! audioresample ! level interval=33333333 \
             post-messages=true ! audioconvert ! {encoder} ! filesink name=out"
        );
        let pipeline = gst::parse::launch(&description)
            .map_err(|e| format!("couldn't build the recording pipeline: {e}"))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| "the recording pipeline is not a pipeline".to_string())?;
        let sink = pipeline.by_name("out").ok_or("no file sink")?;
        sink.set_property("location", shared.path.to_string_lossy().to_string());
        let bus = pipeline.bus().ok_or("the recording pipeline has no bus")?;
        Ok(Self {
            shared,
            pipeline,
            bus,
            banked: 0.0,
            since: None,
        })
    }

    fn elapsed(&self) -> f64 {
        self.banked + self.since.map_or(0.0, |t| t.elapsed().as_secs_f64())
    }

    fn run(mut self, commands: &Receiver<Command>) {
        loop {
            match commands.recv_timeout(TICK) {
                Ok(Command::Start | Command::Resume) => self.play(),
                Ok(Command::Pause) => {
                    let _ = self.pipeline.set_state(gst::State::Paused);
                    self.banked = self.elapsed();
                    self.since = None;
                    let elapsed = self.banked;
                    self.shared.update(|s| {
                        s.elapsed = elapsed;
                        s.level = Level::default();
                    });
                }
                Ok(Command::Stop(done)) => {
                    self.finish();
                    let _ = done.send(());
                    return;
                }
                Ok(Command::Cancel(done)) => {
                    let _ = self.pipeline.set_state(gst::State::Null);
                    let _ = done.send(());
                    return;
                }
                Err(RecvTimeoutError::Timeout) => {}
                // The last handle went away: keep what was recorded.
                Err(RecvTimeoutError::Disconnected) => {
                    self.finish();
                    return;
                }
            }
            if self.drain() {
                let _ = self.pipeline.set_state(gst::State::Null);
                return;
            }
            if self.since.is_some() {
                let elapsed = self.elapsed();
                self.shared.update(|s| s.elapsed = elapsed);
            }
        }
    }

    fn play(&mut self) {
        if self.pipeline.set_state(gst::State::Playing).is_err() {
            self.shared.fail(
                self.error()
                    .unwrap_or_else(|| "couldn't open the audio input or the file".to_string()),
            );
            return;
        }
        self.since = Some(Instant::now());
        self.shared.update(|s| s.state = RecorderState::Recording);
    }

    /// Read bus messages. Returns `true` when an error ended the session.
    fn drain(&mut self) -> bool {
        while let Some(message) = self.bus.pop() {
            if let Some(error) = error_text(&message) {
                self.banked = self.elapsed();
                self.since = None;
                let elapsed = self.banked;
                self.shared.update(|s| s.elapsed = elapsed);
                self.shared.fail(error);
                return true;
            }
            if let Some(level) = level_of(&message) {
                if self.since.is_some() {
                    self.shared.update(|s| s.level = level);
                }
            }
        }
        false
    }

    fn error(&self) -> Option<String> {
        let message = self.bus.timed_pop_filtered(
            gst::ClockTime::from_mseconds(200),
            &[gst::MessageType::Error],
        )?;
        error_text(&message)
    }

    /// Send end-of-stream so the encoder writes its last block and the
    /// header, then close the pipeline.
    fn finish(&mut self) {
        let recording = self.since.is_some();
        self.banked = self.elapsed();
        self.since = None;
        if recording || self.pipeline.current_state() == gst::State::Paused {
            if !recording {
                let _ = self.pipeline.set_state(gst::State::Playing);
            }
            self.pipeline.send_event(gst::event::Eos::new());
            let _ = self.bus.timed_pop_filtered(
                gst::ClockTime::from_seconds(5),
                &[gst::MessageType::Eos, gst::MessageType::Error],
            );
        }
        let _ = self.pipeline.set_state(gst::State::Null);
        let elapsed = self.banked;
        self.shared.update(|s| {
            s.elapsed = elapsed;
            s.level = Level::default();
            if !matches!(
                s.state,
                RecorderState::Failed(_) | RecorderState::Interrupted(_)
            ) {
                s.state = RecorderState::Stopped;
            }
        });
    }
}

fn error_text(message: &gst::Message) -> Option<String> {
    let gst::MessageView::Error(error) = message.view() else {
        return None;
    };
    let text = error.error().to_string();
    let mut chars = text.chars();
    let text = match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect::<String>(),
        None => text,
    };
    Some(text.trim_end_matches('.').to_string())
}

fn level_of(message: &gst::Message) -> Option<Level> {
    let gst::MessageView::Element(element) = message.view() else {
        return None;
    };
    let structure = element.structure()?;
    if structure.name() != "level" {
        return None;
    }
    let loudest = |field: &str| -> Option<f32> {
        let values = structure.get::<gst::glib::ValueArray>(field).ok()?;
        values
            .as_slice()
            .iter()
            .filter_map(|v| v.get::<f64>().ok())
            .map(|db| 10f64.powf(db / 20.0) as f32)
            .reduce(f32::max)
    };
    Some(Level {
        peak: loudest("peak")?.clamp(0.0, 1.0),
        rms: loudest("rms")?.clamp(0.0, 1.0),
    })
}
