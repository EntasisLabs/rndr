//! AVFoundation backend. `AVPlayer` is main-thread only, so its state lives
//! on the main queue, which the app's event loop drains. A worker thread
//! owns the command channel, forwards commands and status polls to the main
//! queue, and copies frames from an `AVPlayerItemVideoOutput`, which is
//! thread-safe. The worker never waits on the main thread, so dropping the
//! last [`Player`](crate::Player) there cannot deadlock.
use std::cell::RefCell;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use dispatch2::DispatchQueue;
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::AnyObject;
use objc2::{AnyThread, MainThreadMarker};
use objc2_av_foundation::{
    AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus, AVPlayerItemVideoOutput,
    AVPlayerTimeControlStatus,
};
use objc2_core_media::{kCMTimeZero, CMTime, CMTimeFlags};
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, CVPixelBuffer,
    CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use rndr_core::VideoFrame;

use crate::{Command, Kind, MediaSource, Shared};

const FRAME: Duration = Duration::from_millis(8);
const POLL: Duration = Duration::from_millis(50);

/// Main-queue-only state. Only touched inside closures run on the main
/// queue, which is what makes the `Send` impls sound.
struct OnMain(RefCell<Option<Main>>);
unsafe impl Send for OnMain {}
unsafe impl Sync for OnMain {}

struct Main {
    shared: Arc<Shared>,
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    rate: f32,
    loaded: bool,
}

pub(crate) fn spawn(shared: Arc<Shared>, commands: Receiver<Command>) -> Option<JoinHandle<()>> {
    let output = (shared.kind == Kind::Video).then(video_output);
    let main = Arc::new(OnMain(RefCell::new(None)));
    {
        let (main, shared, output) = (main.clone(), shared.clone(), output.clone());
        DispatchQueue::main()
            .exec_async(move || autoreleasepool(|_| open(&main, shared, output.as_deref())));
    }
    let worker = std::thread::Builder::new()
        .name("rndr-media".into())
        .spawn({
            let shared = shared.clone();
            move || pump(&shared, &main, &commands, output.as_deref())
        });
    match worker {
        Ok(worker) => Some(worker),
        Err(error) => {
            shared.fail(format!("could not start the media thread: {error}"));
            None
        }
    }
}

fn video_output() -> Retained<AVPlayerItemVideoOutput> {
    // SAFETY: CFString keys are toll-free bridged to NSString.
    let key: &NSString =
        unsafe { &*(kCVPixelBufferPixelFormatTypeKey as *const _ as *const NSString) };
    let format = NSNumber::numberWithUnsignedInt(kCVPixelFormatType_32BGRA);
    let attributes =
        NSDictionary::<NSString, AnyObject>::from_slices(&[key], &[format.as_ref() as &AnyObject]);
    unsafe {
        AVPlayerItemVideoOutput::initWithPixelBufferAttributes(
            AVPlayerItemVideoOutput::alloc(),
            Some(&attributes),
        )
    }
}

fn on_main(main: &Arc<OnMain>, work: impl FnOnce(&mut Main) + Send + 'static) {
    let main = main.clone();
    DispatchQueue::main().exec_async(move || {
        autoreleasepool(|_| {
            if let Some(state) = main.0.borrow_mut().as_mut() {
                work(state);
            }
        })
    });
}

fn pump(
    shared: &Arc<Shared>,
    main: &Arc<OnMain>,
    commands: &Receiver<Command>,
    output: Option<&AVPlayerItemVideoOutput>,
) {
    let mut polled = Instant::now();
    loop {
        match commands.recv_timeout(FRAME) {
            Ok(command) => on_main(main, move |state| state.apply(command)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        shared.flush_wake();
        if polled.elapsed() >= POLL {
            polled = Instant::now();
            on_main(main, Main::poll);
        }
        if let Some(output) = output {
            autoreleasepool(|_| copy_frame(shared, output));
        }
    }
    let main = main.clone();
    DispatchQueue::main().exec_async(move || {
        if let Some(state) = main.0.borrow_mut().take() {
            unsafe { state.player.pause() };
        }
    });
}

fn copy_frame(shared: &Shared, output: &AVPlayerItemVideoOutput) {
    let now = objc2_quartz_core::CACurrentMediaTime();
    unsafe {
        let time = output.itemTimeForHostTime(now);
        if !output.hasNewPixelBufferForItemTime(time) {
            return;
        }
        let Some(buffer) =
            output.copyPixelBufferForItemTime_itemTimeForDisplay(time, std::ptr::null_mut())
        else {
            return;
        };
        if let Some(frame) = to_frame(&buffer) {
            let _ = shared.surface.push(frame);
        }
    }
}

/// Copy a BGRA pixel buffer into a tightly packed frame.
fn to_frame(buffer: &CVPixelBuffer) -> Option<VideoFrame> {
    unsafe {
        if CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) != 0 {
            return None;
        }
    }
    let base = CVPixelBufferGetBaseAddress(buffer) as *const u8;
    let stride = CVPixelBufferGetBytesPerRow(buffer);
    let (w, h) = (
        CVPixelBufferGetWidth(buffer),
        CVPixelBufferGetHeight(buffer),
    );
    let frame = (!base.is_null() && stride >= w * 4).then(|| {
        let mut bytes = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            // SAFETY: the locked buffer holds `h` rows of `stride` bytes.
            let row = unsafe { std::slice::from_raw_parts(base.add(y * stride), w * 4) };
            bytes.extend_from_slice(row);
        }
        VideoFrame::bgra(w as u32, h as u32, bytes)
    });
    unsafe {
        CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly);
    }
    frame
}

fn seconds(time: CMTime) -> Option<f64> {
    let valid =
        time.flags.contains(CMTimeFlags::Valid) && !time.flags.contains(CMTimeFlags::Indefinite);
    valid.then(|| unsafe { time.seconds() })
}

fn open(main: &OnMain, shared: Arc<Shared>, output: Option<&AVPlayerItemVideoOutput>) {
    let Some(mtm) = MainThreadMarker::new() else {
        return shared.fail("AVFoundation must be driven from the main queue");
    };
    let url = match &shared.source {
        MediaSource::File(path) => match std::path::absolute(path) {
            Ok(path) => NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy())),
            Err(error) => return shared.fail(error.to_string()),
        },
        MediaSource::Url(url) => match NSURL::URLWithString(&NSString::from_str(url)) {
            Some(url) => url,
            None => return shared.fail(format!("invalid URL: {url}")),
        },
    };
    let (muted, rate) = {
        let status = shared.status();
        (status.muted, status.rate)
    };
    unsafe {
        let item = AVPlayerItem::playerItemWithURL(&url, mtm);
        if let Some(output) = output {
            item.addOutput(output);
        }
        let player = AVPlayer::playerWithPlayerItem(Some(&item), mtm);
        player.setActionAtItemEnd(AVPlayerActionAtItemEnd::Pause);
        player.setMuted(muted);
        *main.0.borrow_mut() = Some(Main {
            shared,
            player,
            item,
            rate,
            loaded: false,
        });
    }
}

impl Main {
    fn apply(&mut self, command: Command) {
        unsafe {
            match command {
                Command::Play => {
                    if self.at_end() {
                        self.seek(0.0);
                    }
                    self.player.setRate(self.rate);
                }
                Command::Pause => self.player.pause(),
                Command::Seek(at) => self.seek(at),
                Command::Rate(rate) => {
                    self.rate = rate;
                    if self.shared.status().playing {
                        self.player.setRate(rate);
                    }
                }
                Command::Muted(muted) => self.player.setMuted(muted),
            }
        }
    }

    fn seek(&self, at: f64) {
        unsafe {
            self.player.seekToTime_toleranceBefore_toleranceAfter(
                CMTime::with_seconds(at, 600),
                kCMTimeZero,
                kCMTimeZero,
            );
        }
    }

    fn time(&self) -> Option<f64> {
        seconds(unsafe { self.player.currentTime() })
    }

    fn duration(&self) -> Option<f64> {
        seconds(unsafe { self.item.duration() })
    }

    fn at_end(&self) -> bool {
        matches!((self.time(), self.duration()), (Some(t), Some(d)) if t >= d - 0.01)
    }

    fn poll(&mut self) {
        let status = unsafe { self.item.status() };
        if status == AVPlayerItemStatus::Failed {
            let error = unsafe { self.item.error() }
                .map(|e| e.localizedDescription().to_string())
                .unwrap_or_else(|| "could not open the clip".into());
            return self.shared.fail(error);
        }
        if status != AVPlayerItemStatus::ReadyToPlay {
            return;
        }
        let duration = self.duration();
        let time = self.time();
        let stopped =
            unsafe { self.player.timeControlStatus() } == AVPlayerTimeControlStatus::Paused;
        let playing = self.shared.status().playing;
        let finished = playing && stopped && self.at_end();
        if finished && self.shared.looping.load(Ordering::Relaxed) {
            self.seek(0.0);
            unsafe { self.player.setRate(self.rate) };
            self.shared.update(|s| s.time = 0.0);
            return;
        }
        self.loaded = true;
        self.shared.update(|s| {
            s.loaded = true;
            s.duration = duration;
            if let Some(time) = time {
                s.time = time;
            }
            if finished {
                s.playing = false;
                s.ended = true;
            }
        });
    }
}
