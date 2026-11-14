//! Window entry for the investigation console.
//!
//! ```text
//! cargo run -p investigation
//! cargo run -p investigation -- --export console.png
//! cargo run -p investigation --features observe -- --observe
//! ```
//!
//! `--export` writes the headless surface composited over a synthetic desktop.
//! That composite is a preview: the window server's behind-window blur is not
//! in a headless frame. The raw straight-alpha surface is written beside it.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use rndr_core::{TextEngine, Viewport};
use rndr_render::{encode_png, Exporter};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut export: Option<PathBuf> = None;
    let mut observe = false;
    let mut exit_after = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--export" => {
                export = Some(PathBuf::from(
                    args.next().ok_or("--export needs a png path")?,
                ));
            }
            "--observe" => observe = true,
            "--observe-exit-after-15s" => exit_after = true,
            "-h" | "--help" => {
                println!(
                    "investigation [--export PATH.png] [--observe] [--observe-exit-after-15s]"
                );
                return Ok(());
            }
            _ if arg.starts_with("-psn_") => {}
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }

    let mut app = rndr_core::App::new();
    if export.is_some() {
        rndr_paint::install_icons();
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
    }
    investigation::install(&mut app);
    if let Some(path) = export {
        return write_export(&mut app, &path);
    }
    if observe {
        enable_observe(&mut app)?;
    }
    let _exit = arm_exit(&mut app, observe && exit_after);
    rndr_render::run(app);
    Ok(())
}

fn write_export(app: &mut rndr_core::App, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    const WIDTH: f32 = investigation::WIDTH;
    const HEIGHT: f32 = investigation::HEIGHT;
    const SCALE: f32 = 2.0;
    app.world_mut().insert_resource(Viewport {
        scale: SCALE,
        ..Viewport::new(WIDTH, HEIGHT)
    });
    for _ in 0..4 {
        app.update();
    }
    let mut exporter = Exporter::new()?;
    let frame = rndr_paint::frame(app);
    let image = exporter.render_frame(
        Some(app.world()),
        &frame,
        (WIDTH * SCALE) as u32,
        (HEIGHT * SCALE) as u32,
        SCALE,
    )?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let raw = surface_path(path);
    std::fs::write(&raw, encode_png(&image)?)?;
    let preview = composite_over_desktop(&image);
    std::fs::write(path, encode_png(&preview)?)?;
    println!(
        "preview {} ({}x{}), surface {}",
        path.display(),
        preview.width,
        preview.height,
        raw.display()
    );
    Ok(())
}

fn surface_path(path: &Path) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("console");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("png");
    path.with_file_name(format!("{stem}-surface.{ext}"))
}

/// Straight-alpha surface over a blurred stand-in desktop.
///
/// The blur here is only so a screenshot without a window server still reads
/// as frosted glass. It is not `Backdrop` and not [`rndr_core::BackdropBlur`].
fn composite_over_desktop(image: &rndr_render::RgbaImage) -> rndr_render::RgbaImage {
    let width = image.width;
    let height = image.height;
    let mut desk = desktop(width, height);
    desk = box_blur(&desk, width, height, 18);
    desk = box_blur(&desk, width, height, 10);
    let (pixels, _) = image.rgba.as_chunks::<4>();
    let (desk_pixels, _) = desk.as_chunks_mut::<4>();
    for (pixel, over) in pixels.iter().zip(desk_pixels) {
        let alpha = f32::from(pixel[3]) / 255.0;
        for channel in 0..3 {
            let src = f32::from(pixel[channel]);
            let dst = f32::from(over[channel]);
            over[channel] = (src * alpha + dst * (1.0 - alpha)).round() as u8;
        }
        over[3] = 255;
    }
    rndr_render::RgbaImage {
        width,
        height,
        rgba: desk,
    }
}

fn desktop(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let u = x as f32 / width as f32;
            let v = y as f32 / height as f32;
            let i = ((y * width + x) * 4) as usize;
            rgba[i] = (28.0 + 40.0 * u + 18.0 * v) as u8;
            rgba[i + 1] = (42.0 + 30.0 * (1.0 - u) + 10.0 * v) as u8;
            rgba[i + 2] = (72.0 + 50.0 * u) as u8;
            rgba[i + 3] = 255;
        }
    }
    fill_rect(&mut rgba, width, 70, 90, 340, 220, [232, 196, 148, 255]);
    fill_rect(&mut rgba, width, 460, 140, 520, 300, [126, 196, 168, 255]);
    fill_rect(&mut rgba, width, 180, 420, 280, 180, [196, 160, 214, 255]);
    fill_rect(
        &mut rgba,
        width,
        0,
        height.saturating_sub(70),
        width,
        70,
        [20, 24, 32, 255],
    );
    rgba
}

fn fill_rect(rgba: &mut [u8], width: u32, x: u32, y: u32, w: u32, h: u32, color: [u8; 4]) {
    let height = rgba.len() as u32 / (width * 4);
    for row in y..y.saturating_add(h).min(height) {
        for col in x..x.saturating_add(w).min(width) {
            let i = ((row * width + col) * 4) as usize;
            rgba[i..i + 4].copy_from_slice(&color);
        }
    }
}

fn box_blur(src: &[u8], width: u32, height: u32, radius: i32) -> Vec<u8> {
    let mut horizontal = vec![0u8; src.len()];
    blur_x(src, &mut horizontal, width, height, radius);
    let mut out = vec![0u8; src.len()];
    blur_y(&horizontal, &mut out, width, height, radius);
    out
}

fn blur_x(src: &[u8], dst: &mut [u8], width: u32, height: u32, radius: i32) {
    let radius = radius.max(0) as u32;
    let span = radius * 2 + 1;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            for offset in x.saturating_sub(radius)..=(x + radius).min(width - 1) {
                let i = ((y * width + offset) * 4) as usize;
                for channel in 0..4 {
                    sum[channel] += u32::from(src[i + channel]);
                }
            }
            let i = ((y * width + x) * 4) as usize;
            for channel in 0..4 {
                dst[i + channel] = (sum[channel] / span) as u8;
            }
        }
    }
}

fn blur_y(src: &[u8], dst: &mut [u8], width: u32, height: u32, radius: i32) {
    let radius = radius.max(0) as u32;
    let span = radius * 2 + 1;
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            for offset in y.saturating_sub(radius)..=(y + radius).min(height - 1) {
                let i = ((offset * width + x) * 4) as usize;
                for channel in 0..4 {
                    sum[channel] += u32::from(src[i + channel]);
                }
            }
            let i = ((y * width + x) * 4) as usize;
            for channel in 0..4 {
                dst[i + channel] = (sum[channel] / span) as u8;
            }
        }
    }
}

fn enable_observe(app: &mut rndr_core::App) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(feature = "observe")]
    {
        use rndr_render::observe::{ObservationContent, ObservationSession, ObservationTheme};
        app.world_mut().insert_resource(ObservationContent::Public);
        app.world_mut().insert_resource(ObservationTheme(
            "dark translucent investigation console".into(),
        ));
        let descriptor =
            ObservationSession::enable(app.world_mut(), "dark / standard contrast / 100% text")?;
        eprintln!("observation descriptor: {}", descriptor.display());
        Ok(())
    }
    #[cfg(not(feature = "observe"))]
    {
        let _ = app;
        Err("--observe requires --features observe".into())
    }
}

fn arm_exit(app: &mut rndr_core::App, enabled: bool) -> Option<rndr_core::TimerHandle> {
    if !enabled {
        return None;
    }
    #[derive(Clone)]
    struct Exit;
    impl rndr_core::Event for Exit {}
    app.add_system(rndr_core::Schedule::Update, |world| {
        if !rndr_core::events::<Exit>(world).is_empty() {
            if let Some(input) = world.resource_mut::<rndr_core::Input>() {
                input.quit = true;
            }
        }
    });
    Some(rndr_core::after(
        app.world_mut(),
        std::time::Duration::from_secs(15),
        Exit,
    ))
}
