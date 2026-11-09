//! Subtree blur and frosted backdrop panels.
//!
//! A frosted panel slides over colourful shapes, blurring what is behind it,
//! and a card on the right pulses in and out of focus.
//!
//! `cargo run -p rndr --example blur --features parley`
//! `cargo run -p rndr --example blur --features parley -- --png OUT.png`
use std::time::Duration;

use rndr_core::motion::{self, Easing, Keyframes, Prop};
use rndr_core::*;

const W: f32 = 640.0;
const H: f32 = 360.0;
const INK: Color = Color::rgb(0.08, 0.08, 0.1);

fn node(app: &mut App, parent: Entity, layout: Layout, color: Color) -> Entity {
    let e = app.world_mut().spawn((layout, color));
    set_parent(app.world_mut(), e, parent);
    e
}

fn px(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::column()
    }
}

fn pinned(app: &mut App, e: Entity, left: f32, top: f32) {
    app.world_mut().insert(
        e,
        Absolute {
            left: Some(left),
            top: Some(top),
            ..Absolute::default()
        },
    );
}

fn build(app: &mut App) {
    let root = app
        .world_mut()
        .spawn((px(W, H), Color::rgb(0.96, 0.95, 0.92)));
    let shapes = [
        (40.0, 40.0, 120.0, Color::rgb(0.95, 0.35, 0.3)),
        (150.0, 150.0, 140.0, Color::rgb(0.25, 0.45, 0.95)),
        (300.0, 50.0, 100.0, Color::rgb(0.2, 0.7, 0.45)),
        (240.0, 230.0, 90.0, Color::rgb(0.98, 0.75, 0.2)),
    ];
    for (x, y, d, color) in shapes {
        let dot = node(app, root, px(d, d), color);
        app.world_mut().insert(dot, Corners([d / 2.0; 4]));
        pinned(app, dot, x, y);
    }
    for i in 0..6 {
        let stripe = node(app, root, px(8.0, H), INK);
        pinned(app, stripe, 20.0 + i as f32 * 70.0, 0.0);
    }
    let panel = node(
        app,
        root,
        px(220.0, 150.0),
        Color::rgba(1.0, 1.0, 1.0, 0.35),
    );
    pinned(app, panel, 30.0, 105.0);
    app.world_mut().insert(panel, BackdropBlur(14.0));
    app.world_mut().insert(panel, Radius(22.0));
    app.world_mut().insert(
        panel,
        Border {
            color: Color::rgba(1.0, 1.0, 1.0, 0.7),
            width: 1.0,
        },
    );
    app.world_mut()
        .insert(panel, Label::new("Frosted glass").with_color(INK));
    let card = node(app, root, px(150.0, 150.0), Color::rgb(0.55, 0.3, 0.85));
    pinned(app, card, 460.0, 105.0);
    app.world_mut().insert(card, Corners([18.0; 4]));
    let dot = node(app, card, px(60.0, 60.0), Color::rgb(1.0, 1.0, 1.0));
    app.world_mut().insert(dot, Corners([30.0; 4]));
    pinned(app, dot, 45.0, 45.0);
    let slide = Keyframes::new()
        .eased(140.0, 2.4, Easing::EaseInOut)
        .forever()
        .alternate();
    motion::play(app.world_mut(), panel, Prop::X, &slide);
    let focus = Keyframes::new()
        .eased(10.0, 1.2, Easing::EaseInOut)
        .forever()
        .alternate();
    motion::play(app.world_mut(), card, Prop::Blur, &focus);
}

fn strip(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = Sim::new();
    build(&mut sim.app);
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::bundled());
    sim.world().insert_resource(engine);
    sim.world().insert_resource(Viewport::new(W, H));
    let mut exporter = rndr_render::Exporter::new()?;
    let mut strip = rndr_render::RgbaImage {
        width: W as u32,
        height: 0,
        rgba: Vec::new(),
    };
    for ms in [16u64, 600, 600, 1200] {
        sim.advance(Duration::from_millis(ms));
        let frame = rndr_paint::frame(&mut sim.app);
        let image = exporter.render_frame(Some(sim.world()), &frame, W as u32, H as u32, 1.0)?;
        strip.rgba.extend_from_slice(&image.rgba);
        strip
            .rgba
            .extend(std::iter::repeat_n(80u8, W as usize * 4 * 2));
        strip.height += H as u32 + 2;
    }
    std::fs::write(out, rndr_render::encode_png(&strip)?)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if let Some(out) = args
        .iter()
        .position(|a| a == "--png")
        .and_then(|i| args.get(i + 1))
    {
        return strip(out);
    }
    let mut app = App::new();
    build(&mut app);
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr blur".into(),
        size: LogicalSize::new(W, H),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}
