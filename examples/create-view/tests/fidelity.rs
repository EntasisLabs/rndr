//! Rasterize each fixture the way the Create canvas exports SVG, then diff it
//! against the offscreen renderer.
//!
//! Mean absolute error is per channel in `0.0..=1.0`. Shapes, arrows, ink,
//! images, and layers should stay close. Text uses the fixture's line height
//! on both sides. The SVG places glyphs in a fixed em box with side bearings,
//! while the renderer uses the face metrics, so that fixture is reported and
//! only fails when the picture is unrelated.

use std::fs;
use std::path::PathBuf;

use std::collections::BTreeMap;

use create_view::{
    export_scene, fixture_dir, fixture_paths, install_images, install_text, load_project,
    paint_scene, pixel_rect, scene_bounds, scene_svg, CreativeNode, Project,
};
use rndr_core::{Rect, World};
use rndr_paint::{Affine, Canvas, Scene, ZoomCamera};
use rndr_render::{encode_png, ExportError, Exporter, RgbaImage};

struct Report {
    name: String,
    mae: f32,
    over_16: f32,
    over_48: f32,
    width: u32,
    height: u32,
}

#[test]
fn fixtures_match_the_svg_raster() {
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let mut world = World::new();
    install_text(&mut world);
    let mut reports = Vec::new();
    let out_dir = artifact_dir();
    let _ = fs::create_dir_all(&out_dir);
    for path in fixture_paths() {
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("scene")
            .to_string();
        let project = load_project(&path).unwrap_or_else(|err| panic!("{err}"));
        let rect = pixel_rect(scene_bounds(&project.scene));
        let svg = scene_svg(&project.scene, &project.assets, rect);
        let reference = rasterize_svg(&svg, &name);
        let images = install_images(&mut world, &project).unwrap();
        let (scene, exported) = export_scene(&world, &project.scene, &images);
        assert_eq!(
            (exported.x, exported.y, exported.w, exported.h),
            (rect.x, rect.y, rect.w, rect.h),
            "{name} export rect"
        );
        let image = exporter
            .render_scene(&world, &scene, rect, 1.0)
            .unwrap_or_else(|err| panic!("{name}: {err}"));
        assert_eq!(image.width, reference.width, "{name} width");
        assert_eq!(image.height, reference.height, "{name} height");
        let (mae, over_16, over_48) = diff(&reference.rgba, &image.rgba);
        save_compare(&out_dir, &name, &reference, &image);
        println!(
            "{name:8}  {width}x{height}  mae {mae:.4}  >16 {over_16:.2}%  >48 {over_48:.2}%",
            width = image.width,
            height = image.height,
        );
        reports.push(Report {
            name,
            mae,
            over_16,
            over_48,
            width: image.width,
            height: image.height,
        });
    }
    let mut summary = String::from("fixture  size  mae  pct>16  pct>48\n");
    for report in &reports {
        summary.push_str(&format!(
            "{:8}  {}x{}  {:.4}  {:.2}  {:.2}\n",
            report.name, report.width, report.height, report.mae, report.over_16, report.over_48
        ));
    }
    let _ = fs::write(out_dir.join("report.txt"), &summary);
    println!("{summary}");

    for report in &reports {
        match report.name.as_str() {
            // Interiors match. Edges differ by antialiasing and by the ellipse
            // cubic against a true ellipse.
            "shapes" | "arrows" | "ink" | "image" | "layers" => {
                assert!(
                    report.mae < 0.04,
                    "{} mae {} exceeds 0.04",
                    report.name,
                    report.mae
                );
                assert!(
                    report.over_48 < 2.0,
                    "{} has {:.2}% of pixels off by more than 48",
                    report.name,
                    report.over_48
                );
            }
            // The SVG em box and side bearings still differ from the face
            // metrics, so the ceiling only catches a blank or swapped picture.
            "text" => {
                assert!(
                    report.mae < 0.25,
                    "{} mae {} exceeds 0.25",
                    report.name,
                    report.mae
                );
            }
            other => panic!("unexpected fixture {other}"),
        }
    }
}

#[test]
fn a_rounded_frame_corner_clips_its_child() {
    let project = load_project(&fixture_dir().join("layers.json")).unwrap();
    let board = project
        .scene
        .nodes
        .iter()
        .find(|node| node.id == "board")
        .unwrap();
    let corner = project
        .scene
        .nodes
        .iter()
        .find(|node| node.id == "corner")
        .unwrap();
    let radius = board
        .appearance
        .as_ref()
        .map(|item| item.corner_radius as f32)
        .unwrap();
    let origin = [board.transform.x as f32, board.transform.y as f32];
    let arc = [origin[0] + radius, origin[1] + radius];
    // Inside the child and the frame box, outside the quarter circle.
    let outside = [origin[0] + 2.0, origin[1] + 2.0];
    // Inside the quarter circle and the child.
    let inside = [origin[0] + radius, origin[1] + radius * 0.45];
    assert!(
        (outside[0] - arc[0]).hypot(outside[1] - arc[1]) > radius + 4.0,
        "outside sample is too close to the arc"
    );
    assert!(
        (inside[0] - arc[0]).hypot(inside[1] - arc[1]) + 4.0 < radius,
        "inside sample is outside the arc"
    );
    assert!(
        covers(corner, origin, outside) && covers(corner, origin, inside),
        "the corner child must cover both samples"
    );

    let rect = pixel_rect(scene_bounds(&project.scene));
    let reference = rasterize_svg(
        &scene_svg(&project.scene, &project.assets, rect),
        "layers-corner",
    );
    let mut world = World::new();
    install_text(&mut world);
    let images = install_images(&mut world, &project).unwrap();
    let (scene, _) = export_scene(&world, &project.scene, &images);
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let image = exporter
        .render_scene(&world, &scene, rect, 1.0)
        .unwrap_or_else(|err| panic!("{err}"));
    assert_eq!(image.width, reference.width);
    assert_eq!(image.height, reference.height);

    let (out_x, out_y) = image_px(rect, outside);
    let (in_x, in_y) = image_px(rect, inside);
    let svg_out = pixel(&reference, out_x, out_y);
    let gpu_out = pixel(&image, out_x, out_y);
    let svg_in = pixel(&reference, in_x, in_y);
    let gpu_in = pixel(&image, in_x, in_y);
    assert!(
        is_child(svg_in),
        "svg missed the child inside the rounded corner {svg_in:?}"
    );
    assert!(
        is_child(gpu_in),
        "renderer missed the child inside the rounded corner {gpu_in:?}"
    );
    assert!(
        !is_child(svg_out),
        "svg left the corner unclipped {svg_out:?}"
    );
    assert!(
        !is_child(gpu_out),
        "rounded frame corner did not clip its child {gpu_out:?}"
    );
    assert!(
        near(svg_out, gpu_out),
        "corner pixel renderer {gpu_out:?} svg {svg_out:?}"
    );
    assert!(
        near(svg_in, gpu_in),
        "interior pixel renderer {gpu_in:?} svg {svg_in:?}"
    );
}

/// The shadow card in `layers.json`. World axis-aligned box, before blur.
const SHADOW_CARD: Rect = Rect {
    x: 36.0,
    y: 230.0,
    w: 170.0,
    h: 100.0,
};

#[test]
fn a_drop_shadow_does_not_change_when_the_view_pans() {
    let project = load_project(&fixture_dir().join("layers.json")).unwrap();
    let mut world = World::new();
    install_text(&mut world);
    let images = install_images(&mut world, &project).unwrap();
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    // The viewer opens at 800×600 with the camera at (32, 32). Both of these
    // pans keep the shadow card on screen. The third slides it off the bottom
    // and the left, which is where a viewport-sized blur used to change.
    let view = Rect {
        x: 0.0,
        y: 0.0,
        w: 800.0,
        h: 600.0,
    };
    let base = [32.0, 32.0];
    let shifted = [72.0, 52.0];
    let clipped = [-48.0, 340.0];
    let base_image = render_panned(&mut exporter, &world, &project, &images, base, view);
    let shifted_image = render_panned(&mut exporter, &world, &project, &images, shifted, view);
    let clipped_image = render_panned(&mut exporter, &world, &project, &images, clipped, view);

    let shifted_delta = overlap_delta(&base_image, base, &shifted_image, shifted);
    assert!(
        shifted_delta.compared > 100_000,
        "overlap {} is too small",
        shifted_delta.compared
    );
    assert_eq!(
        shifted_delta.max_channel,
        0,
        "integer pan changed {} of {} overlap pixels, worst {} at world {:?}",
        shifted_delta.changed,
        shifted_delta.compared,
        shifted_delta.max_channel,
        shifted_delta.worst
    );

    let clipped_delta = overlap_delta(&base_image, base, &clipped_image, clipped);
    assert!(
        card_crosses_the_edge(clipped, view),
        "the clipped pan should cut the shadow card"
    );
    assert!(
        clipped_delta.compared > 10_000,
        "clipped overlap {} is too small",
        clipped_delta.compared
    );
    assert_eq!(
        clipped_delta.max_channel,
        0,
        "pan off the edge changed {} of {} overlap pixels, worst {} at world {:?}",
        clipped_delta.changed,
        clipped_delta.compared,
        clipped_delta.max_channel,
        clipped_delta.worst
    );

    // Ten notches in and ten out restore the camera bit-for-bit, and the
    // picture with them.
    let mut camera = ZoomCamera::new(base);
    let cursor = [400.0, 300.0];
    for _ in 0..10 {
        camera.apply_wheel(cursor, 32.0);
    }
    for _ in 0..10 {
        camera.apply_wheel(cursor, -32.0);
    }
    assert_eq!(camera.pan(), base);
    assert_eq!(camera.scale().to_bits(), 1.0f32.to_bits());
    let round_trip = render_panned(&mut exporter, &world, &project, &images, camera.pan(), view);
    assert_eq!(round_trip.rgba, base_image.rgba);

    // `--export` is the scene bounds at 1:1. A window aimed at that same rect
    // is the same picture.
    let (scene, rect) = export_scene(&world, &project.scene, &images);
    let exported = exporter
        .render_scene(&world, &scene, rect, 1.0)
        .unwrap_or_else(|err| panic!("{err}"));
    let screen = render_panned(
        &mut exporter,
        &world,
        &project,
        &images,
        [-rect.x, -rect.y],
        Rect {
            x: 0.0,
            y: 0.0,
            w: rect.w,
            h: rect.h,
        },
    );
    assert_eq!(exported.width, screen.width);
    assert_eq!(exported.height, screen.height);
    // The window wraps the scene in one more full-frame layer than `--export`.
    // Resolving that layer moves a few edge pixels by one level (7 on lavapipe).
    let mut changed = 0u32;
    let mut max_channel = 0u8;
    for (a, b) in exported.rgba.chunks(4).zip(screen.rgba.chunks(4)) {
        let delta = a
            .iter()
            .zip(b)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        if delta > 0 {
            changed += 1;
        }
        max_channel = max_channel.max(delta);
    }
    assert!(
        max_channel <= 1 && changed <= 64,
        "export and the window differ in {changed} pixels, worst {max_channel}"
    );
}

fn card_crosses_the_edge(pan: [f32; 2], view: Rect) -> bool {
    let x0 = SHADOW_CARD.x + pan[0];
    let y0 = SHADOW_CARD.y + pan[1];
    let x1 = x0 + SHADOW_CARD.w;
    let y1 = y0 + SHADOW_CARD.h;
    let on = x0 < view.w && y0 < view.h && x1 > view.x && y1 > view.y;
    let off = x0 < view.x || y0 < view.y || x1 > view.x + view.w || y1 > view.y + view.h;
    on && off
}

fn render_panned(
    exporter: &mut Exporter,
    world: &World,
    project: &Project,
    images: &BTreeMap<String, u64>,
    pan: [f32; 2],
    view: Rect,
) -> RgbaImage {
    let mut canvas = Canvas::new();
    canvas.set_camera(Affine::translate(pan[0], pan[1]));
    paint_scene(&mut canvas, world, &project.scene, images);
    let scene = Scene {
        camera: Affine::IDENTITY,
        commands: canvas.finish_clipped(view),
    };
    exporter
        .render_scene(world, &scene, view, 1.0)
        .unwrap_or_else(|err| panic!("{err}"))
}

struct Overlap {
    compared: u32,
    changed: u32,
    max_channel: u8,
    worst: [f32; 2],
}

/// World pixels that land on an integer sample in both images.
fn overlap_delta(a: &RgbaImage, pan_a: [f32; 2], b: &RgbaImage, pan_b: [f32; 2]) -> Overlap {
    let (ax0, ax1) = world_span(pan_a[0], a.width);
    let (ay0, ay1) = world_span(pan_a[1], a.height);
    let (bx0, bx1) = world_span(pan_b[0], b.width);
    let (by0, by1) = world_span(pan_b[1], b.height);
    let x0 = ax0.max(bx0);
    let y0 = ay0.max(by0);
    let x1 = ax1.min(bx1);
    let y1 = ay1.min(by1);
    let mut compared = 0u32;
    let mut changed = 0u32;
    let mut max_channel = 0u8;
    let mut worst = [0.0, 0.0];
    for y in y0..y1 {
        for x in x0..x1 {
            let pa = pixel_at(a, x, y, pan_a);
            let pb = pixel_at(b, x, y, pan_b);
            let mut delta = 0u8;
            for channel in 0..4 {
                delta = delta.max(pa[channel].abs_diff(pb[channel]));
            }
            if delta > 0 {
                changed += 1;
            }
            if delta > max_channel {
                max_channel = delta;
                worst = [x as f32, y as f32];
            }
            compared += 1;
        }
    }
    Overlap {
        compared,
        changed,
        max_channel,
        worst,
    }
}

fn world_span(pan: f32, size: u32) -> (i32, i32) {
    let start = (-pan).ceil() as i32;
    let end = (-pan + size as f32).floor() as i32;
    (start, end)
}

fn pixel_at(image: &RgbaImage, world_x: i32, world_y: i32, pan: [f32; 2]) -> [u8; 4] {
    let x = world_x as f32 + pan[0];
    let y = world_y as f32 + pan[1];
    pixel(image, x.round() as u32, y.round() as u32)
}

fn covers(node: &CreativeNode, parent_origin: [f32; 2], world: [f32; 2]) -> bool {
    let x = world[0] - parent_origin[0];
    let y = world[1] - parent_origin[1];
    let transform = &node.transform;
    x >= transform.x as f32
        && y >= transform.y as f32
        && x <= (transform.x + transform.width) as f32
        && y <= (transform.y + transform.height) as f32
}

fn image_px(rect: Rect, world: [f32; 2]) -> (u32, u32) {
    (
        (world[0] - rect.x).floor().max(0.0) as u32,
        (world[1] - rect.y).floor().max(0.0) as u32,
    )
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

fn is_child(pixel: [u8; 4]) -> bool {
    pixel[0] > 150 && pixel[1] < 90 && pixel[2] < 110 && pixel[3] > 200
}

fn near(left: [u8; 4], right: [u8; 4]) -> bool {
    left.iter()
        .zip(right)
        .all(|(left, right)| left.abs_diff(right) < 24)
}

fn rasterize_svg(svg: &str, name: &str) -> RgbaImage {
    let mut fontdb = resvg::usvg::fontdb::Database::new();
    let fonts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/rndr_render/fonts");
    for file in ["Inter-Regular.ttf", "Inter-Bold.ttf", "Inter-Italic.ttf"] {
        let bytes = fs::read(fonts.join(file)).unwrap_or_else(|err| panic!("{file}: {err}"));
        fontdb.load_font_data(bytes);
    }
    let options = resvg::usvg::Options {
        fontdb: std::sync::Arc::new(fontdb),
        ..resvg::usvg::Options::default()
    };
    let tree =
        resvg::usvg::Tree::from_str(svg, &options).unwrap_or_else(|err| panic!("{name}: {err}"));
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height())
        .unwrap_or_else(|| panic!("{name}: pixmap"));
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    unpremultiply(&mut rgba);
    RgbaImage {
        width: size.width(),
        height: size.height(),
        rgba,
    }
}

fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 {
            pixel[0] = 0;
            pixel[1] = 0;
            pixel[2] = 0;
        } else if alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u32 * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

fn diff(left: &[u8], right: &[u8]) -> (f32, f32, f32) {
    assert_eq!(left.len(), right.len());
    let mut sum = 0.0f32;
    let mut over_16 = 0u32;
    let mut over_48 = 0u32;
    let pixels = (left.len() / 4) as u32;
    for (a, b) in left.as_chunks::<4>().0.iter().zip(right.as_chunks::<4>().0) {
        let mut max = 0u8;
        for channel in 0..4 {
            let delta = a[channel].abs_diff(b[channel]);
            sum += f32::from(delta) / 255.0;
            max = max.max(delta);
        }
        if max > 16 {
            over_16 += 1;
        }
        if max > 48 {
            over_48 += 1;
        }
    }
    let count = (pixels.max(1) * 4) as f32;
    (
        sum / count,
        over_16 as f32 * 100.0 / pixels.max(1) as f32,
        over_48 as f32 * 100.0 / pixels.max(1) as f32,
    )
}

fn save_compare(dir: &std::path::Path, name: &str, reference: &RgbaImage, image: &RgbaImage) {
    let gap = 8u32;
    let width = reference.width * 3 + gap * 2;
    let height = reference.height;
    let mut rgba = vec![255u8; (width * height * 4) as usize];
    blit(&mut rgba, width, 0, reference);
    blit(&mut rgba, width, reference.width + gap, image);
    let mut diff = reference.rgba.clone();
    for (slot, (a, b)) in diff.as_chunks_mut::<4>().0.iter_mut().zip(
        reference
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .zip(image.rgba.as_chunks::<4>().0),
    ) {
        for channel in 0..3 {
            slot[channel] = a[channel].abs_diff(b[channel]).saturating_mul(4);
        }
        slot[3] = 255;
    }
    blit(
        &mut rgba,
        width,
        reference.width * 2 + gap * 2,
        &RgbaImage {
            width: reference.width,
            height: reference.height,
            rgba: diff,
        },
    );
    let png = encode_png(&RgbaImage {
        width,
        height,
        rgba,
    })
    .unwrap();
    let _ = fs::write(dir.join(format!("{name}.png")), png);
}

fn blit(dest: &mut [u8], dest_w: u32, x0: u32, image: &RgbaImage) {
    for y in 0..image.height {
        for x in 0..image.width {
            let from = ((y * image.width + x) * 4) as usize;
            let to = ((y * dest_w + x0 + x) * 4) as usize;
            dest[to..to + 4].copy_from_slice(&image.rgba[from..from + 4]);
        }
    }
}

fn artifact_dir() -> PathBuf {
    let preferred = PathBuf::from("/opt/cursor/artifacts/create-view");
    if preferred.parent().is_some_and(|parent| parent.exists())
        || fs::create_dir_all(&preferred).is_ok()
    {
        return preferred;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/fidelity")
}
