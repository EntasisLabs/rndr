//! A background thread pushes animated NV12 frames into a `FrameSurface`,
//! drawn twice: contained with rounded corners, and covered and mirrored.
//! Each new frame repaints without running layout. Prints surface stats once a
//! second.
//!
//! `cargo run -p rndr_render --example frame_surface`
use std::time::{Duration, Instant};

use rndr_core::{
    set_parent, App, Color, FrameSurface, Label, Layout, Size, SurfaceFit, SurfaceView, VideoFrame,
    WindowConfig,
};

const W: u32 = 320;
const H: u32 = 180;

/// A moving colour sweep with a white bar, BT.709 video range.
fn frame(t: f32) -> VideoFrame {
    let mut luma = vec![0u8; (W * H) as usize];
    let mut chroma = vec![0u8; (W.div_ceil(2) * H.div_ceil(2) * 2) as usize];
    let bar = ((t * 0.25).fract() * W as f32) as u32;
    for y in 0..H {
        for x in 0..W {
            let near_bar = x.abs_diff(bar) < 6;
            luma[(y * W + x) as usize] = if near_bar {
                235
            } else {
                (60 + (y * 120 / H)) as u8
            };
        }
    }
    for y in 0..H.div_ceil(2) {
        for x in 0..W.div_ceil(2) {
            let phase = t + x as f32 / 40.0;
            let at = ((y * W.div_ceil(2) + x) * 2) as usize;
            chroma[at] = (128.0 + 80.0 * phase.sin()) as u8;
            chroma[at + 1] = (128.0 + 80.0 * (phase * 0.7 + y as f32 / 30.0).cos()) as u8;
        }
    }
    VideoFrame::nv12(W, H, luma, chroma)
}

fn main() {
    let surface = FrameSurface::new();
    let producer = surface.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        let mut report = Instant::now();
        loop {
            producer
                .push(frame(start.elapsed().as_secs_f32()))
                .expect("valid frame");
            if report.elapsed() >= Duration::from_secs(1) {
                let stats = producer.stats();
                eprintln!(
                    "pushed {} shown {} size {:?}",
                    stats.pushed,
                    stats.shown,
                    producer.size()
                );
                report = Instant::now();
            }
            std::thread::sleep(Duration::from_millis(33));
        }
    });

    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr frame surface".into(),
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
    let title = world.spawn((
        Layout::column(),
        Label::new("Frames from a background thread"),
    ));
    set_parent(world, title, root);
    let row = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 12.0,
        ..Layout::row()
    },));
    set_parent(world, row, root);
    let views = [
        SurfaceView::new(&surface).with_radius(16.0),
        SurfaceView::new(&surface)
            .with_fit(SurfaceFit::Cover)
            .with_mirrored(true),
    ];
    for view in views {
        let host = world.spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::row()
            },
            Color::rgb(0.16, 0.17, 0.2),
            view,
        ));
        set_parent(world, host, row);
    }
    rndr_render::run(app);
}
