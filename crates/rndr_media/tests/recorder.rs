//! Recording a generated tone (GStreamer on Linux, AVFoundation on macOS),
//! then reading it back.
#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::path::PathBuf;
use std::time::{Duration, Instant};

use rndr_media::{RecordSource, Recorder, RecorderState};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rndr-rec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

fn wait(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out: {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn tone(name: &str) -> Recorder {
    Recorder::with_source(temp(name), RecordSource::TestTone(440.0))
}

fn record(rec: &Recorder, ms: u64) {
    rec.start();
    wait("recording", || rec.is_recording());
    std::thread::sleep(Duration::from_millis(ms));
}

#[cfg(target_os = "linux")]
fn played_duration(path: &PathBuf) -> f64 {
    use rndr_media::{Player, PlayerState};
    let clip = Player::audio(path);
    wait("loaded", || {
        clip.duration().is_some() || matches!(clip.state(), PlayerState::Failed(_))
    });
    assert!(
        !matches!(clip.state(), PlayerState::Failed(_)),
        "{:?}",
        clip.state()
    );
    clip.duration().unwrap()
}

/// `Player` runs on the main queue, which the test harness never drains, so
/// read the file with Core Audio's `afinfo` instead.
#[cfg(target_os = "macos")]
fn played_duration(path: &PathBuf) -> f64 {
    let out = std::process::Command::new("afinfo")
        .arg(path)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "afinfo {}: {text}", path.display());
    text.lines()
        .find_map(|line| line.trim().strip_prefix("estimated duration:"))
        .and_then(|rest| rest.trim().trim_end_matches("sec").trim().parse().ok())
        .unwrap_or_else(|| panic!("no duration in {text}"))
}

/// Compressed formats this platform records.
#[cfg(target_os = "linux")]
const COMPRESSED: &[&str] = &["tone.ogg", "tone.opus", "tone.flac"];
#[cfg(target_os = "macos")]
const COMPRESSED: &[&str] = &["tone.m4a"];

#[test]
fn a_wav_recording_has_its_length_and_a_level() {
    let rec = tone("tone.wav");
    assert_eq!(rec.state(), RecorderState::Idle);
    record(&rec, 600);
    let level = rec.level();
    assert!(level.peak > 0.3 && level.rms > 0.2, "{level:?}");
    assert!(rec.level().meter() > 0.7, "{}", rec.level().meter());
    assert!(rec.live_level().get() > 0.7);
    let note = rec.stop().expect("stopped");
    assert_eq!(rec.state(), RecorderState::Stopped);
    assert_eq!(rec.live_level().get(), 0.0);
    assert!(
        note.duration > 0.5 && note.duration < 1.2,
        "{}",
        note.duration
    );
    let bytes = std::fs::metadata(&note.path).unwrap().len();
    assert!(bytes > 20_000, "{bytes}");
    let played = played_duration(&note.path);
    assert!(
        (played - note.duration).abs() < 0.25,
        "{played} vs {}",
        note.duration
    );
}

#[test]
fn compressed_recordings_play_back() {
    for &name in COMPRESSED {
        let rec = tone(name);
        record(&rec, 400);
        let note = rec.stop().expect(name);
        let played = played_duration(&note.path);
        assert!(played > 0.25 && played < 1.0, "{name}: {played}");
    }
}

#[test]
fn pauses_are_left_out() {
    let rec = tone("paused.wav");
    record(&rec, 300);
    rec.pause();
    assert_eq!(rec.state(), RecorderState::Paused);
    std::thread::sleep(Duration::from_millis(500));
    let held = rec.elapsed();
    assert!(held < 0.6, "{held}");
    rec.resume();
    std::thread::sleep(Duration::from_millis(300));
    let note = rec.stop().unwrap();
    assert!(
        note.duration > 0.45 && note.duration < 0.9,
        "{}",
        note.duration
    );
    let played = played_duration(&note.path);
    assert!(played < 0.95, "{played}");
}

#[test]
fn cancel_deletes_the_file_and_start_records_again() {
    let rec = tone("cancel.wav");
    record(&rec, 200);
    rec.cancel();
    assert_eq!(rec.state(), RecorderState::Idle);
    assert!(!rec.path().exists());
    assert!(rec.stop().is_err());
    record(&rec, 200);
    assert!(rec.stop().is_ok());
    assert!(rec.path().exists());
}

#[test]
fn dropping_the_last_handle_keeps_the_file() {
    let path = temp("dropped.wav");
    {
        let rec = Recorder::with_source(&path, RecordSource::TestTone(220.0));
        record(&rec, 300);
    }
    assert!(played_duration(&path) > 0.2);
}

#[test]
fn an_unknown_extension_or_bad_path_fails() {
    let rec = tone("tone.mp3");
    rec.start();
    let RecorderState::Failed(message) = rec.state() else {
        panic!("{:?}", rec.state());
    };
    assert!(message.contains(".wav"), "{message}");
    let other = if cfg!(target_os = "macos") {
        "tone.ogg"
    } else {
        "tone.m4a"
    };
    let rec = tone(other);
    rec.start();
    assert!(
        matches!(rec.state(), RecorderState::Failed(_)),
        "{:?}",
        rec.state()
    );
    let rec = Recorder::with_source("/nonexistent-dir/x/tone.wav", RecordSource::TestTone(440.0));
    rec.start();
    wait("failed", || matches!(rec.state(), RecorderState::Failed(_)));
    assert!(rec.stop().is_err());
}
