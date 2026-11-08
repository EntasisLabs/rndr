//! Colour animation, keyframe loops, size animation and press scale.
//!
//! Click the colour card to blend its fill and text in OKLab, click the
//! accordion header to grow or shrink its panel, and hold the button to see
//! press scale. The dot pulses on an endless keyframe loop.
//!
//! `cargo run -p rndr --example motion_depth --features parley`
//! `cargo run -p rndr --example motion_depth --features parley -- --png OUT.png`
use std::time::Duration;

use rndr_core::motion::{
    self, ColorProp, Easing, Keyframes, LayoutMotion, PressScale, Prop, Spring,
};
use rndr_core::*;

const W: f32 = 640.0;
const H: f32 = 340.0;
const BLUE: Color = Color::rgb(0.16, 0.36, 0.95);
const YELLOW: Color = Color::rgb(0.98, 0.82, 0.12);
const INK: Color = Color::rgb(1.0, 1.0, 1.0);
const DARK: Color = Color::rgb(0.1, 0.1, 0.12);

struct Scene {
    swatch: Entity,
    header: Entity,
    panel: Entity,
    yellow: bool,
    open: bool,
}

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

fn build(app: &mut App) {
    let root = app.world_mut().spawn((
        Layout {
            padding: 24.0,
            gap: 20.0,
            width: Size::Px(W),
            height: Size::Px(H),
            ..Layout::column()
        },
        Color::rgb(0.95, 0.95, 0.97),
    ));
    let row = node(
        app,
        root,
        Layout {
            gap: 24.0,
            ..Layout::row()
        },
        Color::rgba(0.0, 0.0, 0.0, 0.0),
    );
    let swatch = node(app, row, px(200.0, 90.0), BLUE);
    app.world_mut()
        .insert(swatch, Label::new("Click: OKLab blend").with_color(INK));
    app.world_mut().insert(
        swatch,
        Border {
            color: DARK,
            width: 3.0,
        },
    );
    let dot = node(app, row, px(60.0, 60.0), Color::rgb(0.9, 0.25, 0.45));
    app.world_mut().insert(dot, Radius(30.0));
    let pulse = Keyframes::new()
        .eased(1.18, 0.45, Easing::EaseInOut)
        .eased(1.0, 0.45, Easing::EaseInOut)
        .forever();
    motion::play(app.world_mut(), dot, Prop::Scale, &pulse);
    let button = node(app, row, px(180.0, 60.0), DARK);
    app.world_mut()
        .insert(button, Label::new("Hold me").with_color(INK));
    app.world_mut().insert(button, Radius(12.0));
    app.world_mut().insert(button, PressScale(0.92));

    let panel = node(app, root, px(592.0, 44.0), Color::rgb(0.22, 0.24, 0.3));
    app.world_mut()
        .insert(panel, LayoutMotion(Spring::SNAPPY.into()));
    let header = node(app, panel, px(592.0, 44.0), Color::rgb(0.3, 0.32, 0.4));
    app.world_mut().insert(
        header,
        Label::new("Accordion: click to open").with_color(INK),
    );
    for (i, shade) in [0.55, 0.65, 0.75].into_iter().enumerate() {
        let item = node(app, panel, px(592.0, 40.0), Color::rgb(shade, 0.8, shade));
        app.world_mut()
            .insert(item, Label::new(format!("Row {}", i + 1)).with_color(DARK));
    }
    app.world_mut().insert_resource(Scene {
        swatch,
        header,
        panel,
        yellow: false,
        open: false,
    });
    app.add_system(Schedule::Update, clicks);
}

fn toggle_color(world: &mut World) {
    let Some(scene) = world.resource_mut::<Scene>() else {
        return;
    };
    scene.yellow = !scene.yellow;
    let (swatch, yellow) = (scene.swatch, scene.yellow);
    let (fill, text, border) = if yellow {
        (YELLOW, DARK, BLUE)
    } else {
        (BLUE, INK, DARK)
    };
    let anim = Spring::new(0.6, 0.0);
    motion::animate_color(world, swatch, ColorProp::Fill, fill, anim);
    motion::animate_color(world, swatch, ColorProp::Text, text, anim);
    motion::animate_color(world, swatch, ColorProp::Border, border, anim);
}

fn toggle_panel(world: &mut World) {
    let Some(scene) = world.resource_mut::<Scene>() else {
        return;
    };
    scene.open = !scene.open;
    let (panel, open) = (scene.panel, scene.open);
    if let Some(layout) = world.get_mut::<Layout>(panel) {
        layout.height = Size::Px(if open { 164.0 } else { 44.0 });
    }
}

fn clicks(world: &mut World) {
    let Some(pointer) = world.resource::<Input>().map(|i| i.pointer) else {
        return;
    };
    let Some([x, y]) = pointer.press_at.filter(|_| pointer.pressed) else {
        return;
    };
    let Some(scene) = world.resource::<Scene>() else {
        return;
    };
    let (swatch, header) = (scene.swatch, scene.header);
    match hit_test(world, x, y) {
        Some(hit) if hit == swatch => toggle_color(world),
        Some(hit) if hit == header => toggle_panel(world),
        _ => {}
    }
}

fn strip(out: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut sim = Sim::new();
    build(&mut sim.app);
    let engine: std::sync::Arc<dyn TextEngine> =
        std::sync::Arc::new(rndr_render::ParleyText::bundled());
    sim.world().insert_resource(engine);
    sim.world().insert_resource(Viewport::new(W, H));
    rndr_paint::frame(&mut sim.app);
    sim.advance(Duration::from_millis(16));
    rndr_paint::frame(&mut sim.app);
    toggle_color(sim.world());
    toggle_panel(sim.world());
    let mut exporter = rndr_render::Exporter::new()?;
    let mut strip = rndr_render::RgbaImage {
        width: W as u32,
        height: 0,
        rgba: Vec::new(),
    };
    for ms in [0u64, 90, 90, 120, 600] {
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
        title: "rndr motion depth".into(),
        size: LogicalSize::new(W, H),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}
