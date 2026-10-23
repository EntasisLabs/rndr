//! Pan and zoom a vector scene, or write it to a PNG.
//!
//! Drag to pan. The wheel zooms around the cursor. `--export out.png` writes the world at 1:1
//! and exits. `--bench` times ten thousand filled paths and prints frames per
//! second. It does not open a window.
//!
//! ```text
//! cargo run -p canvas-scene
//! cargo run -p canvas-scene -- --export out.png
//! cargo run -p canvas-scene -- --bench
//! ```

use std::sync::Arc;
use std::time::Instant;

use rndr_core::{
    App, Color, Input, Layout, Rect, Schedule, Size, SurfaceSlot, TextAlign, TextEngine, TextSpan,
    TextStyle, WindowConfig, World, FONT_WEIGHT_BOLD,
};
use rndr_paint::{
    Affine, Canvas, CanvasPainter, CanvasRegistry, Cap, ClipShape, Effect, FillRule, Join, Marker,
    Path, SceneLayer, Stroke, TextSource, ZoomCamera,
};
use rndr_render::{encode_png, load_rgba, Exporter};

const SLOT: u64 = 1;
const PHOTO: u64 = 1;
const WORLD_W: f32 = 960.0;
const WORLD_H: f32 = 600.0;

fn main() {
    let mut args = std::env::args().skip(1);
    let mut export_path = None;
    let mut bench = false;
    while let Some(arg) = args.next() {
        if arg == "--bench" {
            bench = true;
        } else if arg == "--export" {
            export_path = args.next();
        } else if let Some(path) = arg.strip_prefix("--export=") {
            export_path = Some(path.to_string());
        }
    }
    if bench {
        if let Err(err) = run_bench() {
            eprintln!("bench: {err}");
            std::process::exit(1);
        }
        return;
    }
    if let Some(path) = export_path {
        if let Err(err) = run_export(&path) {
            eprintln!("export: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut app = App::new();
    install_text(app.world_mut());
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, navigate);
    app.world_mut().insert_resource(WindowConfig {
        title: "Canvas scene".into(),
        ..WindowConfig::default()
    });
    rndr_render::run(app);
}

fn setup(world: &mut World) {
    load_photo(world);
    let mut registry = CanvasRegistry::new();
    registry.insert(SLOT, ScenePainter);
    world.insert_resource(registry);
    world.insert_resource(ZoomCamera::new([24.0, 24.0]));
    world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
            ..Layout::column()
        },
        SurfaceSlot { id: SLOT },
    ));
}

fn navigate(world: &mut World) {
    let input = world.resource::<Input>().cloned().unwrap_or_default();
    let Some(view) = world.resource_mut::<ZoomCamera>() else {
        return;
    };
    view.drag(input.pointer.down, input.pointer.position);
    view.apply_wheel(input.pointer.position, input.scroll[1]);
}

struct ScenePainter;

impl CanvasPainter for ScenePainter {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        let view = world.resource::<ZoomCamera>().copied().unwrap_or_default();
        canvas.set_camera(view.affine());
        draw_demo(canvas, world);
    }
}

fn draw_demo(canvas: &mut Canvas, world: &World) {
    canvas.fill(
        Path::polygon(&[
            [0.0, 0.0],
            [WORLD_W, 0.0],
            [WORLD_W, WORLD_H],
            [0.0, WORLD_H],
        ]),
        Color::rgb(0.09, 0.10, 0.13),
        FillRule::NonZero,
    );

    canvas.push_transform(Affine::rotate(0.35).then(Affine::translate(180.0, 210.0)));
    canvas.fill(
        Path::ellipse([0.0, 0.0], [78.0, 46.0]),
        Color::rgb(0.20, 0.45, 0.72),
        FillRule::NonZero,
    );
    canvas.stroke(
        Path::polygon(&[
            [0.0, -70.0],
            [66.0, -22.0],
            [40.0, 62.0],
            [-40.0, 62.0],
            [-66.0, -22.0],
        ]),
        Color::rgb(0.93, 0.78, 0.35),
        Stroke::solid(4.0).with_join(Join::Round),
    );
    canvas.pop_transform();

    canvas.stroke(
        Path::line([70.0, 80.0], [280.0, 120.0]),
        Color::rgb(0.95, 0.95, 0.97),
        Stroke::solid(3.0)
            .dashed(vec![10.0, 7.0])
            .with_cap(Cap::Butt)
            .with_end_marker(Marker::Chevron {
                length: 16.0,
                spread: 8.0,
            }),
    );
    canvas.stroke(
        Path::line([70.0, 150.0], [280.0, 150.0]),
        Color::rgb(0.55, 0.85, 0.70),
        Stroke::hairline(1.5),
    );

    canvas.fill(
        ink_ribbon([360.0, 90.0], [560.0, 160.0]),
        Color::rgb(0.95, 0.55, 0.35),
        FillRule::NonZero,
    );

    let mut hole = Path::builder();
    hole.move_to([620.0, 70.0])
        .line_to([760.0, 70.0])
        .line_to([760.0, 190.0])
        .line_to([620.0, 190.0])
        .close();
    hole.move_to([655.0, 105.0])
        .line_to([725.0, 105.0])
        .line_to([725.0, 155.0])
        .line_to([655.0, 155.0])
        .close();
    canvas.fill(
        hole.build(),
        Color::rgb(0.85, 0.28, 0.32),
        FillRule::EvenOdd,
    );

    canvas.image(
        PHOTO,
        Rect {
            x: 620.0,
            y: 220.0,
            w: 150.0,
            h: 110.0,
        },
        [0.15, 0.10, 0.85, 0.90],
        18.0,
    );

    canvas.push_layer(SceneLayer {
        opacity: 1.0,
        clip: Some(ClipShape::RoundRect {
            rect: Rect {
                x: 70.0,
                y: 250.0,
                w: 180.0,
                h: 120.0,
            },
            radius: 22.0,
        }),
        effect: None,
    });
    canvas.fill(
        Path::ellipse([160.0, 250.0], [120.0, 80.0]),
        Color::rgb(0.35, 0.75, 0.55),
        FillRule::NonZero,
    );
    canvas.pop_layer();

    canvas.push_layer(SceneLayer::opacity(0.45));
    canvas.fill(
        Path::round_rect(
            Rect {
                x: 280.0,
                y: 250.0,
                w: 140.0,
                h: 90.0,
            },
            12.0,
        ),
        Color::rgb(0.95, 0.95, 0.98),
        FillRule::NonZero,
    );
    canvas.pop_layer();

    canvas.push_layer(SceneLayer::opacity(1.0).with_effect(Effect::DropShadow {
        offset: [10.0, 14.0],
        blur: 16.0,
        color: Color::rgba(0.0, 0.0, 0.0, 0.55),
    }));
    canvas.fill(
        Path::round_rect(
            Rect {
                x: 460.0,
                y: 250.0,
                w: 130.0,
                h: 88.0,
            },
            14.0,
        ),
        Color::rgb(0.93, 0.94, 0.96),
        FillRule::NonZero,
    );
    canvas.pop_layer();

    canvas.push_transform(Affine::rotate(-0.28).then(Affine::translate(80.0, 470.0)));
    canvas.text(
        world,
        TextSource {
            text: "Create\nvector".into(),
            style: TextStyle {
                family: "Inter".into(),
                size: 42.0,
                weight: FONT_WEIGHT_BOLD,
                letter_spacing: 1.5,
                paragraph_spacing: 8.0,
                align: TextAlign::Start,
                ..TextStyle::default()
            },
            spans: vec![TextSpan::new(0, 6, Color::rgb(0.97, 0.84, 0.42)).underline()],
            max_width: f32::INFINITY,
            origin: [0.0, 0.0],
            color: Color::rgb(0.93, 0.94, 0.96),
        },
    );
    canvas.pop_transform();
}

fn ink_ribbon(from: [f32; 2], to: [f32; 2]) -> Path {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let px = -dy / len;
    let py = dx / len;
    let w0 = 18.0;
    let w1 = 4.0;
    Path::polygon(&[
        [from[0] + px * w0, from[1] + py * w0],
        [from[0] - px * w0, from[1] - py * w0],
        [to[0] - px * w1, to[1] - py * w1],
        [to[0] + px * w1, to[1] + py * w1],
    ])
}

fn load_photo(world: &mut World) {
    let n = 64u32;
    let mut rgba = vec![0u8; (n * n * 4) as usize];
    for y in 0..n {
        for x in 0..n {
            let i = ((y * n + x) * 4) as usize;
            let checker = ((x / 8) + (y / 8)) % 2 == 0;
            let rgb = if x < 16 && y < 16 {
                [220, 64, 54]
            } else if checker {
                [210, 196, 160]
            } else {
                [64, 92, 140]
            };
            rgba[i..i + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
        }
    }
    let _ = load_rgba(world, PHOTO, n, n, rgba);
}

fn install_text(world: &mut World) {
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    world.insert_resource(engine);
}

fn run_export(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    install_text(app.world_mut());
    load_photo(app.world_mut());
    let mut canvas = Canvas::new();
    draw_demo(&mut canvas, app.world());
    let scene = canvas.into_scene();
    let mut exporter = Exporter::new()?;
    let image = exporter.render_scene(
        app.world(),
        &scene,
        Rect {
            x: 0.0,
            y: 0.0,
            w: WORLD_W,
            h: WORLD_H,
        },
        1.0,
    )?;
    let png = encode_png(&image)?;
    std::fs::write(path, png)?;
    println!("wrote {path} ({}x{})", image.width, image.height);
    Ok(())
}

fn run_bench() -> Result<(), Box<dyn std::error::Error>> {
    let world = World::new();
    let mut canvas = Canvas::new();
    let ink = Color::rgb(0.85, 0.35, 0.28);
    for index in 0..10_000 {
        let col = (index % 100) as f32;
        let row = (index / 100) as f32;
        let x = col * 12.0;
        let y = row * 7.0;
        canvas.fill(
            Path::polygon(&[[x, y], [x + 8.0, y], [x + 8.0, y + 5.0], [x, y + 5.0]]),
            ink,
            FillRule::NonZero,
        );
    }
    let scene = canvas.into_scene();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        w: 1280.0,
        h: 720.0,
    };
    let mut exporter = Exporter::new()?;
    exporter.render_scene(&world, &scene, rect, 1.0)?;
    let frames = 20u32;
    let start = Instant::now();
    for _ in 0..frames {
        exporter.render_scene(&world, &scene, rect, 1.0)?;
    }
    let elapsed = start.elapsed().as_secs_f64();
    let fps = f64::from(frames) / elapsed;
    let ms = elapsed * 1000.0 / f64::from(frames);
    println!(
        "10k paths: {fps:.1} fps ({ms:.2} ms) over {frames} frames, 1280x720, {}",
        exporter.adapter_name()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_wheel_notches_in_and_out_restore_the_camera() {
        let mut view = ZoomCamera::new([24.0, 24.0]);
        let cursor = [120.0, 80.0];
        let pan = view.pan();
        let scale = view.scale();
        for _ in 0..10 {
            view.apply_wheel(cursor, 32.0);
        }
        for _ in 0..10 {
            view.apply_wheel(cursor, -32.0);
        }
        assert_eq!(view.scale().to_bits(), scale.to_bits());
        assert_eq!(view.pan(), pan);
    }
}
