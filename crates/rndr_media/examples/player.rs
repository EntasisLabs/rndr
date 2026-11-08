//! Plays a video in a window, looping, and prints its state once a second.
//! Without a path it encodes a short test clip with GStreamer first (Linux).
//!
//! `cargo run -p rndr_media --example player -- [path or URL]`
use std::time::Duration;

use rndr_core::{
    set_parent, App, Color, Label, Layout, Size, SurfaceFit, SurfaceView, WindowConfig,
};
use rndr_media::Player;

fn main() {
    let source = std::env::args().nth(1).unwrap_or_else(test_clip);
    let clip = Player::new(source.as_str());
    clip.set_looping(true);
    clip.play();
    {
        let clip = clip.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            let stats = clip.surface().stats();
            eprintln!(
                "{:?} at {:.2}/{:.2}s size {:?} pushed {} shown {}",
                clip.state(),
                clip.time(),
                clip.duration().unwrap_or(0.0),
                clip.natural_size(),
                stats.pushed,
                stats.shown,
            );
        });
    }

    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr player".into(),
        ..Default::default()
    });
    let world = app.world_mut();
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            gap: 12.0,
            padding: 16.0,
            ..Layout::column()
        },
        Color::rgb(0.08, 0.09, 0.11),
    ));
    let title = world.spawn((Layout::column(), Label::new(format!("Playing {source}"))));
    set_parent(world, title, root);
    let view = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::row()
        },
        Color::rgb(0.0, 0.0, 0.0),
        SurfaceView::new(clip.surface())
            .with_fit(SurfaceFit::Contain)
            .with_radius(12.0),
    ));
    set_parent(world, view, root);
    rndr_render::run(app);
}

#[cfg(target_os = "linux")]
fn test_clip() -> String {
    use gst::prelude::*;
    gst::init().expect("GStreamer");
    let path = std::env::temp_dir().join("rndr-player-example.ogv");
    let pipeline = gst::parse::launch(&format!(
        "videotestsrc num-buffers=150 pattern=ball \
         ! video/x-raw,width=640,height=360,framerate=30/1 ! theoraenc ! oggmux \
         ! filesink location={}",
        path.display()
    ))
    .expect("test clip pipeline");
    pipeline.set_state(gst::State::Playing).expect("encode");
    let bus = pipeline.bus().expect("bus");
    bus.timed_pop_filtered(
        gst::ClockTime::NONE,
        &[gst::MessageType::Eos, gst::MessageType::Error],
    );
    pipeline.set_state(gst::State::Null).expect("stop");
    path.display().to_string()
}

#[cfg(not(target_os = "linux"))]
fn test_clip() -> String {
    eprintln!("usage: player <path or URL>");
    std::process::exit(2);
}
