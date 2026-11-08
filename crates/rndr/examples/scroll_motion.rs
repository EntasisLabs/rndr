//! Scroll glides, fling, rubber-banding and shared-element flights.
//!
//! Drag the list and let go to fling it; drag past either end to feel the
//! rubber band. The Top and Bottom buttons glide the list. Click a swatch to
//! fly it into the detail card, and the card to fly it back.
//!
//! `cargo run -p rndr --example scroll_motion --features parley`
//! `cargo run -p rndr --example scroll_motion --features parley -- --png OUT.png`
use std::time::Duration;

use rndr_core::motion::{self, ScrollPhysics, Spring};
use rndr_core::*;

const W: f32 = 640.0;
const H: f32 = 360.0;
const INK: Color = Color::rgb(1.0, 1.0, 1.0);
const DARK: Color = Color::rgb(0.1, 0.1, 0.12);
const CLEAR: Color = Color::rgba(0.0, 0.0, 0.0, 0.0);
const SWATCHES: [Color; 3] = [
    Color::rgb(0.9, 0.3, 0.35),
    Color::rgb(0.2, 0.65, 0.45),
    Color::rgb(0.25, 0.4, 0.95),
];

struct Scene {
    list: Entity,
    top: Entity,
    bottom: Entity,
    swatches: Vec<Entity>,
    detail: Entity,
    open: Option<Entity>,
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

fn button(app: &mut App, parent: Entity, text: &str) -> Entity {
    let b = node(app, parent, px(120.0, 36.0), DARK);
    app.world_mut().insert(b, Label::new(text).with_color(INK));
    app.world_mut().insert(b, Radius(8.0));
    b
}

fn build(app: &mut App) {
    app.add_plugin(Shell);
    let root = app.world_mut().spawn((
        Layout {
            padding: 24.0,
            gap: 24.0,
            width: Size::Px(W),
            height: Size::Px(H),
            ..Layout::row()
        },
        Color::rgb(0.95, 0.95, 0.97),
    ));
    let left = node(
        app,
        root,
        Layout {
            gap: 12.0,
            ..Layout::column()
        },
        CLEAR,
    );
    let buttons = node(
        app,
        left,
        Layout {
            gap: 12.0,
            ..Layout::row()
        },
        CLEAR,
    );
    let top = button(app, buttons, "Top");
    let bottom = button(app, buttons, "Bottom");
    let list = node(
        app,
        left,
        Layout {
            gap: 4.0,
            ..px(252.0, 264.0)
        },
        Color::rgb(0.86, 0.87, 0.9),
    );
    app.world_mut().insert(list, Scroll::default());
    app.world_mut().insert(list, ScrollPhysics::default());
    for i in 0..30 {
        let shade = 0.55 + 0.35 * (i % 2) as f32;
        let row = node(app, list, px(252.0, 40.0), Color::rgb(shade, shade, 0.98));
        app.world_mut()
            .insert(row, Label::new(format!("Row {}", i + 1)).with_color(DARK));
    }

    let right = node(
        app,
        root,
        Layout {
            gap: 16.0,
            ..Layout::column()
        },
        CLEAR,
    );
    let strip = node(
        app,
        right,
        Layout {
            gap: 16.0,
            ..Layout::row()
        },
        CLEAR,
    );
    let swatches = SWATCHES
        .iter()
        .map(|&c| {
            let s = node(app, strip, px(56.0, 56.0), c);
            app.world_mut().insert(s, Corners::all(28.0));
            s
        })
        .collect();
    let detail = node(app, right, px(316.0, 220.0), SWATCHES[0]);
    app.world_mut().insert(detail, Corners::all(16.0));
    app.world_mut()
        .insert(detail, Label::new("Click to fly back").with_color(INK));
    motion::set(app.world_mut(), detail, motion::Prop::Opacity, 0.0);
    app.world_mut().insert_resource(Scene {
        list,
        top,
        bottom,
        swatches,
        detail,
        open: None,
    });
    app.add_system(Schedule::Update, clicks);
}

fn open(world: &mut World, swatch: Entity) {
    let Some(scene) = world.resource_mut::<Scene>() else {
        return;
    };
    if scene.open.is_some() {
        return;
    }
    scene.open = Some(swatch);
    let detail = scene.detail;
    let fill = world.get::<Color>(swatch).copied().unwrap_or(DARK);
    world.insert(detail, fill);
    motion::fly(world, swatch, detail, Spring::new(0.5, 0.15));
}

fn close(world: &mut World) {
    let Some(scene) = world.resource_mut::<Scene>() else {
        return;
    };
    let (detail, Some(swatch)) = (scene.detail, scene.open.take()) else {
        return;
    };
    motion::fly(world, detail, swatch, Spring::new(0.45, 0.0));
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
    let (list, top, bottom, detail) = (scene.list, scene.top, scene.bottom, scene.detail);
    let swatches = scene.swatches.clone();
    let opened = scene.open.is_some();
    let glide = Spring::new(0.6, 0.0);
    match hit_test(world, x, y) {
        Some(hit) if hit == top => motion::scroll_to(world, list, [0.0, 0.0], glide),
        Some(hit) if hit == bottom => motion::scroll_to(world, list, [0.0, -1.0e6], glide),
        Some(hit) if hit == detail && opened => close(world),
        Some(hit) if swatches.contains(&hit) => open(world, hit),
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
    let (list, swatch) = {
        let scene = sim.world().resource::<Scene>().unwrap();
        (scene.list, scene.swatches[2])
    };
    motion::scroll_to(sim.world(), list, [0.0, -400.0], Spring::new(0.6, 0.0));
    open(sim.world(), swatch);
    let mut exporter = rndr_render::Exporter::new()?;
    let mut strip = rndr_render::RgbaImage {
        width: W as u32,
        height: 0,
        rgba: Vec::new(),
    };
    for ms in [16u64, 80, 80, 120, 700] {
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
        title: "rndr scroll motion".into(),
        size: LogicalSize::new(W, H),
        ..Default::default()
    });
    rndr::run(app);
    Ok(())
}
