//! Headless pixels for paint decorations: per-corner radii, linear and radial
//! gradients, outer and inset box shadows, and per-side and dashed borders.
//! Writes one PNG; no window.
//!
//! `cargo run -p rndr --example decor_gallery -- OUTPUT.png`
use rndr_core::{
    set_parent, App, Axis, BorderSides, Color, Corners, Entity, Gradient, GradientStop, Layout,
    Radius, Shadow, Shadows, Size, Viewport,
};
use rndr_render::{encode_png, Exporter};

const W: u32 = 1080;
const H: u32 = 200;

fn tile(app: &mut App, row: Entity, fill: Color) -> Entity {
    let tile = app.world_mut().spawn((
        Layout {
            width: Size::Px(140.0),
            height: Size::Px(120.0),
            ..Layout::column()
        },
        fill,
        Radius(16.0),
    ));
    set_parent(app.world_mut(), tile, row);
    tile
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: decor_gallery OUTPUT.png")?;
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let row = app.world_mut().spawn((
        Layout {
            axis: Axis::Row,
            gap: 32.0,
            padding: 40.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::row()
        },
        Color::rgb(0.93, 0.94, 0.96),
    ));
    let white = Color::rgb(1.0, 1.0, 1.0);
    let ink = Color::rgb(0.2, 0.3, 0.75);

    let corners = tile(&mut app, row, white);
    let world = app.world_mut();
    world.insert(corners, Corners([32.0, 4.0, 32.0, 4.0]));
    world.insert(
        corners,
        Shadows(vec![Shadow::drop(
            8.0,
            24.0,
            Color::rgba(0.0, 0.0, 0.0, 0.25),
        )]),
    );

    let linear = tile(&mut app, row, white);
    app.world_mut().insert(
        linear,
        Gradient::Linear {
            angle: -std::f32::consts::FRAC_PI_4,
            stops: vec![
                GradientStop::new(0.0, Color::rgb(0.98, 0.45, 0.35)),
                GradientStop::new(0.5, Color::rgb(0.95, 0.8, 0.3)),
                GradientStop::new(1.0, Color::rgb(0.3, 0.75, 0.6)),
            ],
        },
    );

    let radial = tile(&mut app, row, Color::rgb(0.1, 0.12, 0.2));
    app.world_mut().insert(
        radial,
        Gradient::Radial {
            center: [0.5, 0.4],
            radius: 1.2,
            stops: vec![
                GradientStop::new(0.0, Color::rgba(0.55, 0.65, 1.0, 1.0)),
                GradientStop::new(1.0, Color::rgba(0.55, 0.65, 1.0, 0.0)),
            ],
        },
    );

    let inset = tile(&mut app, row, Color::rgb(0.86, 0.88, 0.92));
    app.world_mut().insert(
        inset,
        Shadows(vec![Shadow {
            offset: [0.0, 4.0],
            blur: 14.0,
            spread: 0.0,
            color: Color::rgba(0.0, 0.0, 0.0, 0.35),
            inset: true,
        }]),
    );

    let dashed = tile(&mut app, row, white);
    app.world_mut()
        .insert(dashed, BorderSides::uniform(ink, 2.0).dashed(8.0, 5.0));

    let sides = tile(&mut app, row, white);
    let world = app.world_mut();
    world.insert(sides, Corners::top_bottom(24.0, 0.0));
    world.insert(
        sides,
        BorderSides {
            color: ink,
            widths: [2.0, 2.0, 10.0, 2.0],
            dash: None,
        },
    );

    let frame = rndr_paint::frame(&mut app);
    let image = Exporter::new()?.render_frame(Some(app.world()), &frame, W, H, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
