//! Linux playback through GStreamer, against clips encoded at test time with
//! gst-plugins-base (Theora video, Vorbis audio, Ogg container).
#![cfg(target_os = "linux")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use gst::prelude::*;
use rndr_core::PixelFormat;
use rndr_media::{Player, PlayerState};

fn encode(name: &str, pipeline: &str) -> PathBuf {
    gst::init().unwrap();
    let path = std::env::temp_dir().join(format!("rndr-media-{}-{name}", std::process::id()));
    let launch = format!("{pipeline} ! filesink location={}", path.display());
    let pipeline = gst::parse::launch(&launch).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let bus = pipeline.bus().unwrap();
    for message in bus.iter_timed(gst::ClockTime::from_seconds(30)) {
        match message.view() {
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(e) => panic!("encoding {name}: {}", e.error()),
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null).unwrap();
    path
}

/// One second of solid red, 64x48 at 30 fps.
fn video_clip() -> &'static PathBuf {
    static CLIP: OnceLock<PathBuf> = OnceLock::new();
    CLIP.get_or_init(|| {
        encode(
            "red.ogv",
            "videotestsrc num-buffers=30 pattern=red ! video/x-raw,width=64,height=48,framerate=30/1 \
             ! theoraenc ! oggmux",
        )
    })
}

/// One second of a 440 Hz tone.
fn audio_clip() -> &'static PathBuf {
    static CLIP: OnceLock<PathBuf> = OnceLock::new();
    CLIP.get_or_init(|| {
        encode(
            "tone.ogg",
            "audiotestsrc num-buffers=44 samplesperbuffer=1000 ! audio/x-raw,rate=44000 \
             ! audioconvert ! vorbisenc ! oggmux",
        )
    })
}

fn wait_for(player: &Player, what: &str, done: impl Fn(&Player) -> bool) {
    let start = Instant::now();
    while !done(player) {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out waiting for {what}: {player:?}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn wait_state(player: &Player, state: PlayerState) {
    wait_for(player, &format!("{state:?}"), |p| p.state() == state);
}

#[test]
fn opens_paused_with_a_poster_frame() {
    let clip = Player::file(video_clip());
    wait_state(&clip, PlayerState::Ready);
    assert!(clip.muted(), "video starts muted");
    let duration = clip.duration().expect("duration");
    assert!((duration - 1.0).abs() < 0.05, "duration {duration}");
    wait_for(&clip, "poster frame", |p| p.natural_size().is_some());
    assert_eq!(clip.natural_size(), Some([64, 48]));
    let frame = clip.surface().frame().unwrap();
    assert_eq!(frame.format, PixelFormat::Nv12);
    // BT.601 video-range red is Y 81, Cb 90, Cr 240; Theora is lossy.
    let y = frame.planes[0][24 * 64 + 32];
    let (cb, cr) = (frame.planes[1][12 * 64 + 32], frame.planes[1][12 * 64 + 33]);
    assert!(
        y.abs_diff(81) < 6 && cb.abs_diff(90) < 8 && cr.abs_diff(240) < 8,
        "{y} {cb} {cr}"
    );
}

#[test]
fn plays_to_the_end_and_restarts() {
    let clip = Player::file(video_clip());
    wait_state(&clip, PlayerState::Ready);
    let before = clip.surface().stats().pushed;
    clip.play();
    assert_eq!(clip.state(), PlayerState::Playing);
    wait_state(&clip, PlayerState::Ended);
    assert!(clip.time() > 0.9, "time at end {}", clip.time());
    assert!(
        clip.surface().stats().pushed >= before + 20,
        "frames pushed {}",
        clip.surface().stats().pushed - before
    );
    clip.play();
    assert_eq!(clip.state(), PlayerState::Playing);
    wait_for(&clip, "restart", |p| p.time() < 0.5);
    wait_state(&clip, PlayerState::Ended);
}

#[test]
fn loops_instead_of_ending() {
    let clip = Player::file(video_clip());
    clip.set_looping(true);
    clip.play();
    let start = Instant::now();
    let mut wrapped = false;
    let mut last = 0.0;
    while start.elapsed() < Duration::from_millis(2500) {
        assert_eq!(clip.state(), PlayerState::Playing);
        let now = clip.time();
        wrapped |= now + 0.3 < last;
        last = now;
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(wrapped, "time never wrapped to the start");
}

#[test]
fn seeks_while_paused_and_shows_the_frame() {
    let clip = Player::file(video_clip());
    wait_state(&clip, PlayerState::Ready);
    let version = clip.surface().version();
    clip.seek(0.5);
    assert_eq!(clip.time(), 0.5);
    wait_for(&clip, "new frame after seek", |p| {
        p.surface().version() > version
    });
    assert_eq!(clip.state(), PlayerState::Ready);
    clip.seek(99.0);
    assert!(
        (clip.time() - clip.duration().unwrap()).abs() < 1e-9,
        "clamped"
    );
    clip.seek(f64::NAN);
    assert_eq!(clip.time(), 0.0);
}

#[test]
fn a_faster_rate_finishes_sooner() {
    let clip = Player::file(video_clip());
    clip.set_rate(4.0);
    clip.set_rate(-1.0);
    clip.set_rate(0.0);
    assert_eq!(clip.rate(), 4.0);
    wait_state(&clip, PlayerState::Ready);
    let start = Instant::now();
    clip.play();
    wait_state(&clip, PlayerState::Ended);
    let took = start.elapsed();
    assert!(took < Duration::from_millis(700), "took {took:?} at 4x");
}

#[test]
fn pause_holds_the_time() {
    let clip = Player::file(video_clip());
    clip.play();
    wait_for(&clip, "progress", |p| p.time() > 0.2);
    clip.pause();
    assert_eq!(clip.state(), PlayerState::Ready);
    std::thread::sleep(Duration::from_millis(150));
    let held = clip.time();
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        (clip.time() - held).abs() < 0.01,
        "{held} -> {}",
        clip.time()
    );
}

#[test]
fn audio_plays_without_frames() {
    let tone = Player::audio(audio_clip());
    assert!(tone.is_audio());
    assert!(!tone.muted(), "audio starts unmuted");
    wait_state(&tone, PlayerState::Ready);
    let duration = tone.duration().expect("duration");
    assert!((duration - 1.0).abs() < 0.05, "duration {duration}");
    let position = tone.position();
    tone.set_muted(true);
    tone.play();
    wait_state(&tone, PlayerState::Ended);
    assert!(position.get() > 0.9, "position {}", position.get());
    assert!(tone.surface().frame().is_none());
    assert_eq!(tone.natural_size(), None);
}

#[test]
fn missing_files_fail() {
    let clip = Player::file("/nonexistent/clip.ogv");
    wait_for(&clip, "failure", |p| {
        matches!(p.state(), PlayerState::Failed(_))
    });
    clip.play();
    assert!(matches!(clip.state(), PlayerState::Failed(_)));
}

#[test]
fn the_waker_hears_state_changes() {
    let calls = Arc::new(AtomicUsize::new(0));
    let clip = Player::file(video_clip());
    let counter = calls.clone();
    clip.set_waker(move || {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    wait_state(&clip, PlayerState::Ready);
    clip.play();
    wait_state(&clip, PlayerState::Ended);
    assert!(
        calls.load(Ordering::SeqCst) >= 2,
        "calls {}",
        calls.load(Ordering::SeqCst)
    );
}

#[test]
fn dropping_the_last_clone_stops_playback() {
    let clip = Player::file(video_clip());
    let other = clip.clone();
    clip.play();
    drop(clip);
    wait_for(&other, "progress", |p| p.time() > 0.1);
    let start = Instant::now();
    drop(other);
    assert!(start.elapsed() < Duration::from_secs(1));
}
