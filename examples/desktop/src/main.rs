//! Native runner and reproducible offscreen GPU evidence.
use rndr_core::{App, LogicalSize, TextEngine, Viewport, WindowConfig};
use std::{path::PathBuf, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut export: Option<PathBuf> = None;
    let mut preferences = None;
    let mut isolated = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--export" => export = Some(args.next().ok_or("--export needs a directory")?.into()),
            "--preferences" => {
                preferences = Some(rndr_app::Preferences::at(
                    args.next().ok_or("--preferences needs a path")?,
                ));
                isolated = true;
            }
            "--memory" => {
                isolated = true;
                preferences = None;
            }
            "-h" | "--help" => {
                println!("Desktop Workshop [--memory | --preferences PATH] [--export DIRECTORY]\nNo network. --export writes GPU-rendered PNGs and exits.");
                return Ok(());
            }
            // Finder supplies process serial arguments on some macOS versions.
            _ if arg.starts_with("-psn_") => {}
            _ => return Err(format!("unknown argument: {arg}").into()),
        }
    }
    if !isolated && export.is_none() {
        preferences = Some(rndr_app::Preferences::for_app("org.rndr.desktop-workshop")?);
    }
    let mut app = App::new();
    desktop::install(&mut app, preferences);
    app.world_mut().insert_resource(WindowConfig {
        title: "Desktop Workshop".into(),
        size: LogicalSize::new(880.0, 760.0),
        min_size: Some(LogicalSize::new(420.0, 640.0)),
        ..WindowConfig::default()
    });
    if let Some(dir) = export {
        std::fs::create_dir_all(&dir)?;
        app.world_mut()
            .insert_resource(Arc::new(rndr_render::ParleyText::new()) as Arc<dyn TextEngine>);
        let mut exporter = rndr_render::Exporter::new()?;
        for (name, width, height, scale, settings) in [
            ("workspace-1x", 880.0, 760.0, 1.0, false),
            ("workspace-2x", 880.0, 760.0, 2.0, false),
            ("settings-narrow", 420.0, 640.0, 1.0, true),
        ] {
            app.world_mut().insert_resource(Viewport {
                scale,
                ..Viewport::new(width, height)
            });
            app.update();
            if settings {
                use rndr_core::{request_focus, Button, Input, Key};
                let entity = app
                    .world()
                    .entities_with::<Button>()
                    .into_iter()
                    .find(|e| app.world().get::<Button>(*e).unwrap().action == "settings")
                    .unwrap();
                request_focus(app.world_mut(), entity);
                app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Enter);
            }
            app.update();
            app.update();
            let frame = rndr_paint::frame(&mut app);
            let image = exporter.render_frame(
                Some(app.world()),
                &frame,
                (width * scale) as u32,
                (height * scale) as u32,
                scale,
            )?;
            // Readback sanity, not an aesthetic or native-window quality assertion.
            let first = &image.rgba[..4];
            let different = image
                .rgba
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p.as_slice() != first)
                .count();
            if different < 1000 {
                return Err("render readback is unexpectedly uniform".into());
            }
            let path = dir.join(format!("{name}.png"));
            std::fs::write(&path, rndr_render::encode_png(&image)?)?;
            println!(
                "{}: {}x{}, {different} pixels differ from first",
                path.display(),
                image.width,
                image.height
            );
        }
        return Ok(());
    }
    rndr_render::run(app);
    Ok(())
}
