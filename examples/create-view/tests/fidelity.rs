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

use create_view::{
    export_scene, fixture_paths, install_images, install_text, load_project, pixel_rect,
    scene_bounds, scene_svg,
};
use rndr_core::World;
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
