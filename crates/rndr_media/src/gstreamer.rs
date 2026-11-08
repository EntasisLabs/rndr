//! GStreamer backend: `playbin` with an `appsink` that copies decoded frames
//! into the player's surface. A worker thread owns the pipeline, applies
//! commands and reads the bus.
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use gst::prelude::*;
use gst_video::prelude::*;
use rndr_core::{VideoFrame, YuvMatrix, YuvRange};

use crate::{Command, Kind, MediaSource, Shared};

const TICK: Duration = Duration::from_millis(50);

pub(crate) fn spawn(shared: Arc<Shared>, commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    let spawned = std::thread::Builder::new()
        .name("rndr-media".into())
        .spawn({
            let shared = shared.clone();
            move || match Pipeline::open(shared.clone()) {
                Ok(pipeline) => pipeline.run(&commands),
                Err(error) => shared.fail(error),
            }
        });
    match spawned {
        Ok(worker) => Some(worker),
        Err(error) => {
            shared.fail(format!("could not start the media thread: {error}"));
            None
        }
    }
}

struct Pipeline {
    shared: Arc<Shared>,
    playbin: gst::Element,
    bus: gst::Bus,
    rate: f64,
    pending_seek: Option<f64>,
}

impl Pipeline {
    fn open(shared: Arc<Shared>) -> Result<Self, String> {
        gst::init().map_err(|e| format!("GStreamer is unavailable: {e}"))?;
        let uri = match &shared.source {
            MediaSource::Url(url) => url.clone(),
            MediaSource::File(path) => {
                let path = std::path::absolute(path).map_err(|e| e.to_string())?;
                gst::glib::filename_to_uri(&path, None)
                    .map_err(|e| e.to_string())?
                    .to_string()
            }
        };
        let playbin = gst::ElementFactory::make("playbin")
            .property("uri", &uri)
            .build()
            .map_err(|_| "GStreamer's playbin is missing (install gst-plugins-base)".to_string())?;
        let video_sink = match shared.kind {
            Kind::Video => video_sink(&shared),
            Kind::Audio => gst::ElementFactory::make("fakesink")
                .build()
                .map_err(|e| e.to_string())?,
        };
        playbin.set_property("video-sink", &video_sink);
        let (muted, rate) = {
            let status = shared.status();
            (status.muted, status.rate)
        };
        playbin.set_property("mute", muted);
        let bus = playbin.bus().ok_or("playbin has no bus")?;
        playbin
            .set_state(gst::State::Paused)
            .map_err(|_| format!("could not open {uri}"))?;
        Ok(Self {
            shared,
            playbin,
            bus,
            rate: rate as f64,
            pending_seek: None,
        })
    }

    fn run(mut self, commands: &Receiver<Command>) {
        loop {
            match commands.recv_timeout(TICK) {
                Ok(command) => self.apply(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            while let Some(message) = self.bus.pop() {
                self.message(&message);
            }
            if self.shared.status().playing {
                self.publish_time();
            }
        }
        let _ = self.playbin.set_state(gst::State::Null);
    }

    fn loaded(&self) -> bool {
        self.shared.status().loaded
    }

    fn apply(&mut self, command: Command) {
        match command {
            Command::Play => {
                if self.playbin.current_state() == gst::State::Paused && self.at_end() {
                    self.seek(0.0);
                }
                let _ = self.playbin.set_state(gst::State::Playing);
            }
            Command::Pause => {
                let _ = self.playbin.set_state(gst::State::Paused);
            }
            Command::Seek(seconds) => self.seek(seconds),
            Command::Rate(rate) => {
                self.rate = rate as f64;
                if self.loaded() {
                    let at = self.position().unwrap_or(0.0);
                    self.seek(at);
                }
            }
            Command::Muted(muted) => self.playbin.set_property("mute", muted),
        }
    }

    fn at_end(&self) -> bool {
        match (self.position(), self.shared.status().duration) {
            (Some(at), Some(duration)) => at >= duration - 0.001,
            _ => false,
        }
    }

    fn seek(&mut self, seconds: f64) {
        if !self.loaded() {
            self.pending_seek = Some(seconds);
            return;
        }
        let _ = self.playbin.seek(
            self.rate,
            gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
            gst::SeekType::Set,
            gst::ClockTime::from_nseconds((seconds * 1e9) as u64),
            gst::SeekType::None,
            gst::ClockTime::NONE,
        );
    }

    fn position(&self) -> Option<f64> {
        self.playbin
            .query_position::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1e9)
    }

    fn duration(&self) -> Option<f64> {
        self.playbin
            .query_duration::<gst::ClockTime>()
            .map(|t| t.nseconds() as f64 / 1e9)
    }

    fn publish_time(&self) {
        if let Some(at) = self.position() {
            self.shared.update(|s| s.time = at);
        }
    }

    fn message(&mut self, message: &gst::Message) {
        use gst::MessageView;
        match message.view() {
            MessageView::Error(error) => {
                let detail = error.debug().map(|d| format!(" ({d})")).unwrap_or_default();
                self.shared.fail(format!("{}{detail}", error.error()));
            }
            MessageView::AsyncDone(_) => {
                let first = !self.loaded();
                let duration = self.duration();
                self.shared.update(|s| {
                    s.loaded = true;
                    if duration.is_some() {
                        s.duration = duration;
                    }
                });
                if first && (self.rate - 1.0).abs() > f64::EPSILON && self.pending_seek.is_none() {
                    self.pending_seek = Some(0.0);
                }
                if let Some(at) = self.pending_seek.take() {
                    self.seek(at);
                }
                if !self.shared.status().playing {
                    self.publish_time();
                }
            }
            MessageView::DurationChanged(_) => {
                if let Some(duration) = self.duration() {
                    self.shared.update(|s| s.duration = Some(duration));
                }
            }
            MessageView::Eos(_) => {
                if self
                    .shared
                    .looping
                    .load(std::sync::atomic::Ordering::Relaxed)
                {
                    self.seek(0.0);
                    self.shared.update(|s| s.time = 0.0);
                } else {
                    let _ = self.playbin.set_state(gst::State::Paused);
                    let end = self.duration().or(self.position());
                    self.shared.update(|s| {
                        s.playing = false;
                        s.ended = true;
                        if let Some(end) = end {
                            s.time = end;
                        }
                    });
                }
            }
            _ => {}
        }
    }
}

fn video_sink(shared: &Arc<Shared>) -> gst::Element {
    let caps = gst_video::VideoCapsBuilder::new()
        // Video codecs decode to YUV; NV12 is half the upload of RGBA and the
        // renderer converts it on the GPU. playbin converts anything else.
        .format(gst_video::VideoFormat::Nv12)
        .build();
    let sink = gst_app::AppSink::builder()
        .caps(&caps)
        .max_buffers(1)
        .drop(true)
        .build();
    let on_sample = shared.clone();
    let on_preroll = shared.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                show(&on_sample, &sample);
                Ok(gst::FlowSuccess::Ok)
            })
            .new_preroll(move |sink| {
                let sample = sink.pull_preroll().map_err(|_| gst::FlowError::Eos)?;
                show(&on_preroll, &sample);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    sink.upcast()
}

fn show(shared: &Shared, sample: &gst::Sample) {
    if let Some(frame) = to_frame(sample) {
        let _ = shared.surface.push(frame);
    }
}

/// Copy a decoded sample into a tightly packed [`VideoFrame`].
fn to_frame(sample: &gst::Sample) -> Option<VideoFrame> {
    let info = gst_video::VideoInfo::from_caps(sample.caps()?).ok()?;
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(sample.buffer()?, &info).ok()?;
    let (w, h) = (info.width(), info.height());
    let plane = |index: u32, row: u32, rows: u32| -> Option<Vec<u8>> {
        let data = frame.plane_data(index).ok()?;
        let stride = *frame.plane_stride().get(index as usize)? as usize;
        let (row, rows) = (row as usize, rows as usize);
        let mut out = Vec::with_capacity(row * rows);
        for y in 0..rows {
            out.extend_from_slice(data.get(y * stride..y * stride + row)?);
        }
        Some(out)
    };
    let colorimetry = info.colorimetry();
    let matrix = match colorimetry.matrix() {
        gst_video::VideoColorMatrix::Bt709 => YuvMatrix::Bt709,
        _ => YuvMatrix::Bt601,
    };
    let range = match colorimetry.range() {
        gst_video::VideoColorRange::Range0_255 => YuvRange::Full,
        _ => YuvRange::Video,
    };
    match info.format() {
        gst_video::VideoFormat::Nv12 => Some(
            VideoFrame::nv12(
                w,
                h,
                plane(0, w, h)?,
                plane(1, w.div_ceil(2) * 2, h.div_ceil(2))?,
            )
            .with_yuv(matrix, range),
        ),
        gst_video::VideoFormat::Rgba => Some(VideoFrame::rgba(w, h, plane(0, w * 4, h)?)),
        gst_video::VideoFormat::Bgra => Some(VideoFrame::bgra(w, h, plane(0, w * 4, h)?)),
        _ => None,
    }
}
