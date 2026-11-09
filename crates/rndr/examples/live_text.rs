//! Live text and live colours driven from a background thread.
//!
//! A worker thread updates a clock, its colour, a border, a shadow and a
//! gradient through `Live` handles. Frames run without Update or Layout.
//!
//! `cargo run -p rndr --example live_text --features parley`
//! `cargo run -p rndr --example live_text --features parley -- --png OUT.png`
use std::time::{Duration, Instant};

use rndr_core::*;

const W: f32 = 640.0;
const H: f32 = 360.0;

struct Handles {
    clock: LiveText,
    ink: Live<Color>,
    edge: Live<Color>,
    glow: Live<Color>,
    top: Live<Color>,
    bottom: Live<Color>,
}

fn hue(t: f32) -> Color {
    let tau = std::f32::consts::TAU;
    let channel = |shift: f32| 0.5 + 0.45 * (tau * (t + shift)).cos();
    Color::rgb(channel(0.0), channel(1.0 / 3.0), channel(2.0 / 3.0))
}

fn build(app: &mut App) -> Handles {
    let handles = Handles {
        clock: LiveText::new("00:00.0"),
        ink: Live::new(hue(0.0)),
        edge: Live::new(hue(0.3)),
        glow: Live::new(hue(0.6)),
        top: Live::new(hue(0.1)),
        bottom: Live::new(hue(0.5)),
    };
    let world = app.world_mut();
    let root = world.spawn((
        Layout {
            width: Size::Px(W),
            height: Size::Px(H),
            padding: 60.0,
            gap: 40.0,
            ..Layout::row()
        },
        Color::rgb(0.97, 0.96, 0.94),
    ));
    let clock = world.spawn((
        Layout {
            width: Size::Px(240.0),
            height: Size::Px(120.0),
            padding: 24.0,
            ..Layout::column()
        },
        Color::rgb(1.0, 1.0, 1.0),
        Label::new("00:00.0"),
        Radius(16.0),
        Border {
            color: hue(0.3),
            width: 4.0,
        },
        Shadows(vec![Shadow::drop(10.0, 24.0, hue(0.6))]),
    ));
    world.insert(
        clock,
        TextStyle {
            family: "Inter".into(),
            size: 40.0,
            weight: 600,
            ..TextStyle::default()
        },
    );
    world.insert(clock, LiveLabel::new(&handles.clock));
    world.insert(clock, LiveTextColor::new(&handles.ink));
    world.insert(clock, LiveBorder::new(&handles.edge));
    world.insert(
        clock,
        LiveShadow::spring(&handles.glow, motion::Spring::SMOOTH),
    );
    let swatch = world.spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(200.0),
            ..Layout::column()
        },
        Corners([24.0; 4]),
        Gradient::Linear {
            angle: 0.0,
            stops: vec![
                GradientStop::new(0.0, hue(0.1)),
                GradientStop::new(1.0, hue(0.5)),
            ],
        },
    ));
    world.insert(swatch, LiveGradient::new(&handles.top, &handles.bottom));
    set_parent(world, clock, root);
    set_parent(world, swatch, root);
    handles
}

fn publish(handles: &Handles, seconds: f32) {
    let tenths = (seconds * 10.0) as u32;
    handles.clock.set(format!(
        "{:02}:{:02}.{}",
        tenths / 600,
        tenths / 10 % 60,
        tenths % 10
    ));
    let t = seconds / 6.0;
    handles.ink.set(hue(t));
    handles.edge.set(hue(t + 0.3));
    handles.glow.set(hue(t + 0.6));
    handles.top.set(hue(t + 0.1));
    handles.bottom.set(hue(t + 0.5));
}

fn strip(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = Sim::new();
    let handles = build(&mut sim.app);
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
    rndr_paint::frame(&mut sim.app);
    for seconds in [0.0f32, 1.3, 2.7, 4.1] {
        publish(&handles, seconds);
        sim.advance(Duration::from_millis(16));
        let frame = rndr_paint::frame_live(&mut sim.app);
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
    let handles = build(&mut app);
    std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            publish(&handles, start.elapsed().as_secs_f32());
            std::thread::sleep(Duration::from_millis(50));
        }
    });
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr live text".into(),
        size: LogicalSize::new(W, H),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}
