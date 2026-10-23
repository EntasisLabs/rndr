//! Open a Create scene, or write it to a PNG.
//!
//! Drag to pan. The wheel zooms around the cursor, and ten notches in followed
//! by ten notches out land on the same camera. `--export out.png` renders the
//! scene's bounds at 1:1 and exits. `--svg out.svg` writes the same picture as
//! SVG.
//!
//! ```text
//! cargo run -p create-view -- fixtures/shapes.json
//! cargo run -p create-view -- --export out.png fixtures/shapes.json
//! ```

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use create_view::{
    export_scene, install_images, install_text, load_project, scene_svg, LoadError, Project,
};
use rndr_core::{App, Input, Layout, Schedule, Size, SurfaceSlot, WindowConfig, World};
use rndr_paint::{Canvas, CanvasPainter, CanvasRegistry, ZoomCamera};
use rndr_render::{encode_png, Exporter};

const SLOT: u64 = 1;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), LoadError> {
    let mut export_path = None;
    let mut svg_path = None;
    let mut scene_path = None;
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--export" {
            export_path = args.next().map(PathBuf::from);
        } else if let Some(path) = arg.strip_prefix("--export=") {
            export_path = Some(PathBuf::from(path));
        } else if arg == "--svg" {
            svg_path = args.next().map(PathBuf::from);
        } else if let Some(path) = arg.strip_prefix("--svg=") {
            svg_path = Some(PathBuf::from(path));
        } else if arg.starts_with('-') {
            return Err(LoadError(format!("unknown argument {arg}")));
        } else {
            scene_path = Some(PathBuf::from(arg));
        }
    }
    let scene_path = scene_path.unwrap_or_else(|| create_view::fixture_dir().join("shapes.json"));
    let project = load_project(&scene_path)?;
    if export_path.is_some() || svg_path.is_some() {
        if let Some(path) = svg_path {
            write_svg(&project, &path)?;
        }
        if let Some(path) = export_path {
            write_png(&project, &path)?;
        }
        return Ok(());
    }
    let mut app = App::new();
    install_text(app.world_mut());
    let ids = install_images(app.world_mut(), &project)?;
    app.world_mut().insert_resource(project);
    app.world_mut().insert_resource(ids);
    app.world_mut()
        .insert_resource(ZoomCamera::new([32.0, 32.0]));
    app.world_mut().insert_resource(WindowConfig {
        title: format!(
            "Create — {}",
            scene_path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("scene")
        ),
        ..WindowConfig::default()
    });
    let mut registry = CanvasRegistry::new();
    registry.insert(SLOT, ViewPainter);
    app.world_mut().insert_resource(registry);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, navigate);
    rndr_render::run(app);
    Ok(())
}

fn setup(world: &mut World) {
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
    let Some(camera) = world.resource_mut::<ZoomCamera>() else {
        return;
    };
    camera.drag_from(
        input.pointer.down,
        input.pointer.position,
        input.pointer.press_at,
    );
    camera.apply_wheel(input.pointer.position, input.scroll[1]);
}

struct ViewPainter;

impl CanvasPainter for ViewPainter {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        let camera = world
            .resource::<ZoomCamera>()
            .copied()
            .unwrap_or_else(|| ZoomCamera::new([32.0, 32.0]));
        canvas.set_camera(camera.affine());
        let Some(project) = world.resource::<Project>() else {
            return;
        };
        let Some(images) = world.resource::<std::collections::BTreeMap<String, u64>>() else {
            return;
        };
        create_view::paint_scene(canvas, world, &project.scene, images);
    }
}

fn write_svg(project: &Project, path: &std::path::Path) -> Result<(), LoadError> {
    let rect = create_view::pixel_rect(create_view::scene_bounds(&project.scene));
    let svg = scene_svg(&project.scene, &project.assets, rect);
    fs::write(path, svg).map_err(|err| LoadError(format!("{}: {err}", path.display())))?;
    println!("wrote {}", path.display());
    Ok(())
}

fn write_png(project: &Project, path: &std::path::Path) -> Result<(), LoadError> {
    let mut world = World::new();
    install_text(&mut world);
    let images = install_images(&mut world, project)?;
    let (scene, rect) = export_scene(&world, &project.scene, &images);
    let mut exporter = Exporter::new().map_err(|err| LoadError(err.to_string()))?;
    let image = exporter
        .render_scene(&world, &scene, rect, 1.0)
        .map_err(|err| LoadError(err.to_string()))?;
    let png = encode_png(&image).map_err(|err| LoadError(err.to_string()))?;
    fs::write(path, png).map_err(|err| LoadError(format!("{}: {err}", path.display())))?;
    println!(
        "wrote {} ({}x{})",
        path.display(),
        image.width,
        image.height
    );
    Ok(())
}
