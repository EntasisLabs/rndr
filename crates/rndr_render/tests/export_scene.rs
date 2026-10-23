//! Offscreen checks for fills, holes, clips, opacity, shadows, and dashes.
//!
//! The golden is the same image. Mean absolute error absorbs the small
//! differences between a 4× resolve on lavapipe, Metal, and DX12.

use std::f32::consts::PI;

use rndr_core::{Color, Rect, World};
use rndr_paint::{Affine, Canvas, ClipShape, Effect, FillRule, ImageOp, Path, SceneLayer, Stroke};
use rndr_render::load_rgba;
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

#[test]
fn a_rotated_child_stays_with_a_scaled_camera() {
    let world = World::new();
    let mut canvas = Canvas::new();
    canvas.push_transform(Affine::scale(2.0));
    canvas.push_transform(Affine::rotate(PI / 2.0).then(Affine::translate(30.0, 10.0)));
    canvas.fill(
        Path::polygon(&[[8.0, -2.0], [12.0, -2.0], [12.0, 2.0], [8.0, 2.0]]),
        Color::rgb(1.0, 0.0, 0.0),
        FillRule::NonZero,
    );
    canvas.pop_transform();
    canvas.pop_transform();
    let image = render(&world, canvas, 80.0, 80.0);
    let hit = pixel(&image, 60, 40);
    let miss = pixel(&image, 30, 30);
    assert!(hit[0] > 200 && hit[3] > 200, "child under camera {hit:?}");
    assert!(miss[3] < 40, "old composition {miss:?}");
}

#[test]
fn a_rounded_image_masks_its_corner_and_its_clip() {
    let mut world = World::new();
    let mut rgba = Vec::with_capacity(8 * 8 * 4);
    for _ in 0..8 * 8 {
        rgba.extend_from_slice(&[255, 0, 0, 255]);
    }
    load_rgba(&mut world, 1, 8, 8, rgba).unwrap();
    let mut canvas = Canvas::new();
    canvas.image_opacity(ImageOp {
        id: 1,
        dest: Rect {
            x: 10.0,
            y: 10.0,
            w: 40.0,
            h: 40.0,
        },
        uv: [0.0, 0.0, 1.0, 1.0],
        radius: 16.0,
        opacity: 1.0,
        clip: None,
    });
    canvas.image_opacity(ImageOp {
        id: 1,
        dest: Rect {
            x: 60.0,
            y: 10.0,
            w: 24.0,
            h: 24.0,
        },
        uv: [0.0, 0.0, 1.0, 1.0],
        radius: 0.0,
        opacity: 1.0,
        clip: Some(Rect {
            x: 60.0,
            y: 10.0,
            w: 24.0,
            h: 12.0,
        }),
    });
    let image = render(&world, canvas, 100.0, 50.0);
    let corner = pixel(&image, 11, 11);
    let center = pixel(&image, 30, 30);
    let inside = pixel(&image, 70, 14);
    let below = pixel(&image, 70, 30);
    assert!(corner[3] < 40, "rounded corner {corner:?}");
    assert!(
        center[0] > 200 && center[3] > 200,
        "image center {center:?}"
    );
    assert!(inside[0] > 200 && inside[3] > 200, "inside clip {inside:?}");
    assert!(below[3] < 40, "below clip {below:?}");
}

#[test]
fn a_zoomed_drop_shadow_falls_off_without_flat_steps() {
    let world = World::new();
    let mut canvas = Canvas::new();
    // Scale 8 turns a sigma of 8 into 64 device pixels. The old kernel stepped
    // about sigma/3 texels, which is a run of identical pixels through the
    // steep part of the falloff.
    const SCALE: f32 = 8.0;
    const BLUR: f32 = 8.0;
    canvas.push_layer(SceneLayer::opacity(1.0).with_effect(Effect::DropShadow {
        offset: [0.0, 0.0],
        blur: BLUR,
        color: Color::rgba(0.0, 0.0, 0.0, 0.9),
    }));
    canvas.fill(
        Path::polygon(&[[30.0, 16.0], [54.0, 16.0], [54.0, 40.0], [30.0, 40.0]]),
        Color::rgb(0.95, 0.95, 0.95),
        FillRule::NonZero,
    );
    canvas.pop_layer();
    let image = render_at(&world, canvas, 96.0, 56.0, SCALE);
    let sigma = BLUR * SCALE;
    let edge = (54.0 * SCALE).round() as u32;
    let y = (28.0 * SCALE).round() as u32;
    let start = edge + 4;
    let steep_end = start + (sigma * 1.2) as u32;
    assert!(steep_end + 8 < image.width, "image {}", image.width);
    let mut longest = 1u32;
    let mut run = 1u32;
    let mut prev = pixel(&image, start, y)[3];
    for x in (start + 1)..=steep_end {
        let alpha = pixel(&image, x, y)[3];
        assert!(
            alpha <= prev.saturating_add(3),
            "alpha rose from {prev} to {alpha} at x={x}"
        );
        if alpha == prev {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
        prev = alpha;
    }
    assert!(
        longest <= 6,
        "flat run of {longest} px in the steep falloff (sigma {sigma})"
    );
    let near = pixel(&image, start + (sigma * 0.25) as u32, y)[3];
    let mid = pixel(&image, start + (sigma * 1.5) as u32, y)[3];
    let far = pixel(&image, start + (sigma * 3.0) as u32, y)[3];
    assert!(near > mid + 20, "near {near} mid {mid}");
    assert!(mid > far + 8, "mid {mid} far {far}");
    assert!(far < 30, "tail still clipped or truncated, alpha {far}");
}

#[test]
fn a_rounded_image_keeps_its_edge_color_and_a_device_pixel_radius() {
    let mut world = World::new();
    let mut rgba = Vec::with_capacity(16 * 16 * 4);
    for _ in 0..16 * 16 {
        rgba.extend_from_slice(&[220, 40, 40, 255]);
    }
    load_rgba(&mut world, 3, 16, 16, rgba).unwrap();
    let mut canvas = Canvas::new();
    canvas.fill(
        Path::polygon(&[[0.0, 0.0], [80.0, 0.0], [80.0, 40.0], [0.0, 40.0]]),
        Color::rgb(0.12, 0.13, 0.15),
        FillRule::NonZero,
    );
    canvas.image_opacity(ImageOp {
        id: 3,
        dest: Rect {
            x: 8.0,
            y: 8.0,
            w: 16.0,
            h: 16.0,
        },
        uv: [0.0, 0.0, 1.0, 1.0],
        radius: 4.0,
        opacity: 1.0,
        clip: None,
    });
    let image = render_at(&world, canvas, 80.0, 40.0, 1.0);
    let center = pixel(&image, 16, 16);
    let edge = pixel(&image, 16, 8);
    assert!(center[0] > 180 && center[3] > 200, "center {center:?}");
    assert!(
        edge[0] + 12 > center[0] && edge[1] < 80 && edge[3] > 200,
        "top edge {edge:?} picked up a dark fringe, center {center:?}"
    );
    let corner = pixel(&image, 8, 8);
    let backdrop = pixel(&image, 2, 2);
    assert!(
        corner[0].abs_diff(backdrop[0]) < 8
            && corner[1].abs_diff(backdrop[1]) < 8
            && corner[2].abs_diff(backdrop[2]) < 8,
        "radius 4 should clear the corner pixel, got {corner:?} backdrop {backdrop:?}"
    );

    let mut canvas = Canvas::new();
    canvas.image_opacity(ImageOp {
        id: 3,
        dest: Rect {
            x: 2.0,
            y: 2.0,
            w: 16.0,
            h: 16.0,
        },
        uv: [0.0, 0.0, 1.0, 1.0],
        radius: 4.0,
        opacity: 1.0,
        clip: None,
    });
    // Scale 3: the 16px image is 48 device pixels and the radius is 12.
    // One device pixel in from the corner is outside that arc, and would be
    // covered if the radius stayed a single pixel.
    let zoomed = render_at(&world, canvas, 24.0, 24.0, 3.0);
    let origin = (2.0_f32 * 3.0).round() as u32;
    let cut = pixel(&zoomed, origin + 1, origin + 1);
    let flat = pixel(&zoomed, origin + 24, origin + 2);
    assert!(
        cut[3] < 48,
        "device-pixel radius should still be outside at +1px, got {cut:?}"
    );
    assert!(
        flat[0] > 180 && flat[3] > 200,
        "flat edge {flat:?} should stay the icon color"
    );
}

fn render(world: &World, canvas: Canvas, w: f32, h: f32) -> RgbaImage {
    render_at(world, canvas, w, h, 1.0)
}

fn render_at(world: &World, canvas: Canvas, w: f32, h: f32, scale: f32) -> RgbaImage {
    let scene = canvas.into_scene();
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    exporter
        .render_scene(
            world,
            &scene,
            Rect {
                x: 0.0,
                y: 0.0,
                w,
                h,
            },
            scale,
        )
        .unwrap_or_else(|err| panic!("{err}"))
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
