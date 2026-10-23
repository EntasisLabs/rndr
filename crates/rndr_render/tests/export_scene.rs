//! Offscreen checks for fills, holes, clips, opacity, shadows, and dashes.
//!
//! The golden is the same image. Mean absolute error absorbs the small
//! differences between a 4× resolve on lavapipe, Metal, and DX12.

use rndr_core::{Color, Rect, World};
use rndr_paint::{Canvas, ClipShape, Effect, FillRule, Path, SceneLayer, Stroke};
use rndr_render::{encode_png, ExportError, Exporter, RgbaImage};

#[test]
fn export_rejects_sizes_past_the_cap_without_a_device() {
    // The size check lives in the library tests. This binary test only needs
    // the constants to stay part of the public surface.
    assert_eq!(rndr_render::EXPORT_MAX_SIDE, 8192);
    assert_eq!(rndr_render::EXPORT_MAX_PIXELS, 32_000_000);
}

#[test]
fn offscreen_scene_matches_regions_and_the_golden() {
    let world = World::new();
    let mut canvas = Canvas::new();
    canvas.fill(
        Path::polygon(&[[0.0, 0.0], [160.0, 0.0], [160.0, 120.0], [0.0, 120.0]]),
        Color::rgb(0.10, 0.11, 0.13),
        FillRule::NonZero,
    );
    canvas.fill(
        Path::polygon(&[[40.0, 36.0], [120.0, 36.0], [120.0, 84.0], [40.0, 84.0]]),
        Color::rgb(1.0, 0.0, 0.0),
        FillRule::NonZero,
    );
    let mut hole = Path::builder();
    hole.move_to([8.0, 8.0])
        .line_to([40.0, 8.0])
        .line_to([40.0, 32.0])
        .line_to([8.0, 32.0])
        .close();
    hole.move_to([16.0, 14.0])
        .line_to([32.0, 14.0])
        .line_to([32.0, 26.0])
        .line_to([16.0, 26.0])
        .close();
    canvas.fill(hole.build(), Color::rgb(0.1, 0.85, 0.3), FillRule::EvenOdd);
    canvas.push_layer(SceneLayer {
        opacity: 1.0,
        clip: Some(ClipShape::RoundRect {
            rect: Rect {
                x: 124.0,
                y: 8.0,
                w: 28.0,
                h: 28.0,
            },
            radius: 6.0,
        }),
        effect: None,
    });
    canvas.fill(
        Path::polygon(&[[110.0, 4.0], [156.0, 4.0], [156.0, 40.0], [110.0, 40.0]]),
        Color::rgb(0.2, 0.45, 0.95),
        FillRule::NonZero,
    );
    canvas.pop_layer();
    canvas.push_layer(SceneLayer::opacity(0.5));
    canvas.fill(
        Path::polygon(&[[8.0, 70.0], [36.0, 70.0], [36.0, 98.0], [8.0, 98.0]]),
        Color::rgb(1.0, 1.0, 1.0),
        FillRule::NonZero,
    );
    canvas.pop_layer();
    canvas.push_layer(SceneLayer::opacity(1.0).with_effect(Effect::DropShadow {
        offset: [8.0, 0.0],
        blur: 2.0,
        color: Color::rgba(0.0, 0.0, 0.0, 0.9),
    }));
    canvas.fill(
        Path::polygon(&[[70.0, 96.0], [100.0, 96.0], [100.0, 112.0], [70.0, 112.0]]),
        Color::rgb(0.95, 0.95, 0.95),
        FillRule::NonZero,
    );
    canvas.pop_layer();
    canvas.stroke(
        Path::line([8.0, 110.0], [150.0, 110.0]),
        Color::rgb(1.0, 0.85, 0.2),
        Stroke::solid(3.0).dashed(vec![8.0, 8.0]),
    );
    let scene = canvas.into_scene();
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let image = exporter
        .render_scene(
            &world,
            &scene,
            Rect {
                x: 0.0,
                y: 0.0,
                w: 160.0,
                h: 120.0,
            },
            1.0,
        )
        .unwrap_or_else(|err| panic!("{err}"));

    let red = pixel(&image, 80, 60);
    assert!(
        red[0] > 200 && red[1] < 40 && red[2] < 40,
        "red center {red:?}"
    );
    let hole_px = pixel(&image, 24, 20);
    assert!(hole_px[1] < 80, "even-odd hole {hole_px:?}");
    let green = pixel(&image, 12, 12);
    assert!(green[1] > 150, "even-odd fill {green:?}");
    let clipped = pixel(&image, 132, 20);
    assert!(clipped[2] > 150, "inside clip {clipped:?}");
    let outside = pixel(&image, 116, 20);
    assert!(outside[2] < 80, "outside clip {outside:?}");
    // 50% white over the dark background blends in linear light, so the
    // stored sRGB channel sits near 188 and the composite alpha is opaque.
    let faded = pixel(&image, 20, 84);
    assert!(
        (160..210).contains(&faded[0]) && faded[3] > 200,
        "group opacity {faded:?}"
    );
    let shadow = pixel(&image, 106, 104);
    let clear = pixel(&image, 62, 104);
    assert!(
        shadow[0] + 8 < clear[0],
        "drop shadow {shadow:?} should be darker than {clear:?}"
    );
    let (ink, gap) = dash_counts(&image, 110);
    assert!(ink > 8 && gap > 8, "dash ink {ink} gap {gap}");

    let png = encode_png(&image).unwrap();
    let golden_path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/goldens/markers.png");
    if std::env::var_os("RNDR_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(golden_path.parent().unwrap()).unwrap();
        std::fs::write(&golden_path, &png).unwrap();
        return;
    }
    let stored = std::fs::read(&golden_path).unwrap_or_else(|_| {
        panic!(
            "missing {} — rerun with RNDR_UPDATE_GOLDEN=1",
            golden_path.display()
        )
    });
    let decoded = rndr_render::decode(&stored).unwrap_or_else(|err| panic!("{err:?}"));
    assert_eq!(decoded.width, image.width);
    assert_eq!(decoded.height, image.height);
    let error = mean_abs(&decoded.rgba, &image.rgba);
    assert!(
        error < 0.08,
        "golden mae {error:.4} on {}",
        exporter.adapter_name()
    );
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

fn dash_counts(image: &RgbaImage, y: u32) -> (u32, u32) {
    let mut ink = 0u32;
    let mut gap = 0u32;
    for x in 16..140 {
        let px = pixel(image, x, y);
        if px[0] > 180 && px[1] > 140 {
            ink += 1;
        } else if px[0] < 80 {
            gap += 1;
        }
    }
    (ink, gap)
}

fn mean_abs(a: &[u8], b: &[u8]) -> f32 {
    let n = a.len().min(b.len()).max(1);
    let mut acc = 0u64;
    for (left, right) in a.iter().zip(b.iter()) {
        acc += u64::from(left.abs_diff(*right));
    }
    acc as f32 / n as f32 / 255.0
}
