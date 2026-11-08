//! An editable canvas: tools on top, layers and style on the right.
//!
//! ```text
//! cargo run -p canvas-editor                 # sample board in memory
//! cargo run -p canvas-editor -- ./boards     # FsStore folder, document "board"
//! cargo run -p canvas-editor -- --export out.png
//! ```

use rndr_canvas::*;
use rndr_canvas_ui::editor::CanvasEditor;
use rndr_canvas_ui::panels;
use rndr_canvas_ui::view::{self, CanvasView};
use rndr_core::{
    set_parent, take_action_intents, App, Layout, LogicalSize, Schedule, Scroll, Size, SurfaceSlot,
    TextEngine, Viewport, WindowConfig, World,
};
use rndr_render::{encode_png, Exporter};
use rndr_ui::{apply_theme_bridge, Surface, UiId, UiPlugin, UiTheme};
use std::sync::Arc;

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;
const SLOT: u64 = 1;
const SIDE: f32 = 400.0;

fn shape(id: &str, kind: ShapeKind, fill: &str, t: Transform) -> Node {
    Node::new(
        id,
        Content::Shape {
            shape: kind,
            fill: fill.into(),
            stroke: "#1e1e1e".into(),
            stroke_width: 2.0,
            line: None,
        },
        t,
    )
}

fn sample() -> Scene {
    let t = |x, y, width, height, rotation| Transform {
        x,
        y,
        width,
        height,
        rotation,
    };
    let mut frame = Node::new(
        "frame",
        Content::Frame {
            name: "Board".into(),
            background: "#ffffff".into(),
        },
        t(40.0, 40.0, 640.0, 420.0, 0.0),
    );
    frame.appearance = Some(Appearance {
        corner_radius: 16.0,
        shadow: Some(Shadow {
            x: 0.0,
            y: 6.0,
            blur: 12.0,
            color: "#000000".into(),
            opacity: 0.18,
        }),
    });
    let mut note = shape(
        "note",
        ShapeKind::Rectangle,
        "#ffe066",
        t(40.0, 60.0, 200.0, 140.0, -4.0),
    )
    .with_parent("frame");
    note.appearance = Some(Appearance {
        corner_radius: 8.0,
        shadow: None,
    });
    let title = Node::new(
        "title",
        Content::Text {
            text: "Plan the launch".into(),
            color: "#1e1e1e".into(),
            font_family: "Inter".into(),
            font_size: 22.0,
            font_weight: Some(600),
            font_style: None,
        },
        t(56.0, 80.0, 170.0, 30.0, -4.0),
    )
    .with_parent("frame");
    let circle = shape(
        "circle",
        ShapeKind::Ellipse,
        "#a5d8ff",
        t(330.0, 80.0, 120.0, 120.0, 0.0),
    )
    .with_parent("frame");
    let arrow = rndr_canvas::geometry::set_line_endpoints(
        &Node::new(
            "arrow",
            Content::Shape {
                shape: ShapeKind::Line,
                fill: "transparent".into(),
                stroke: "#1971c2".into(),
                stroke_width: 3.0,
                line: Some(Line {
                    start: LinePoint::default(),
                    end: LinePoint::default(),
                    start_arrow: false,
                    end_arrow: true,
                    dashed: true,
                }),
            },
            Transform::default(),
        ),
        Vec2::new(250.0, 130.0),
        Vec2::new(320.0, 140.0),
    )
    .with_parent("frame");
    let points = (0..40)
        .map(|i| {
            let x = i as f64 * 9.0;
            InkPoint {
                pressure: Some(0.3 + 0.5 * (i as f64 / 40.0)),
                ..InkPoint::new(x, 30.0 + (x / 30.0).sin() * 24.0)
            }
        })
        .collect();
    let ink = Node::new(
        "ink",
        Content::Ink {
            color: "#e03131".into(),
            input: InputKind::Pen,
            brush: Brush {
                size: 6.0,
                ..Brush::default()
            },
            points,
        },
        t(80.0, 280.0, 351.0, 60.0, 0.0),
    )
    .with_parent("frame");
    Scene {
        nodes: vec![frame, note, title, circle, arrow, ink],
        ..Scene::default()
    }
}

fn open_store(args: &[String]) -> Arc<dyn CanvasStore + Send + Sync> {
    let folder = args
        .iter()
        .find(|a| !a.starts_with("--") && !a.ends_with(".png"));
    let store: Arc<dyn CanvasStore + Send + Sync> = match folder {
        Some(path) => Arc::new(FsStore::open(path).expect("open store folder")),
        None => Arc::new(MemoryStore::new()),
    };
    let request = CreateRequest {
        id: "board".into(),
        title: "Board".into(),
        scene: sample(),
    };
    if store.get("board").is_err() {
        store.create(request).expect("create board");
    }
    store
}

fn app(store: Arc<dyn CanvasStore + Send + Sync>) -> App {
    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.add_plugin(UiPlugin);
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    app.world_mut()
        .insert_resource(Viewport::new(WIDTH, HEIGHT));
    let world = app.world_mut();
    let root = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 8.0,
        ..Layout::column()
    },));
    let toolbar = Surface::Navigation
        .mount(world, root, UiId(1))
        .expect("toolbar surface")
        .content;
    let body = world.spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 8.0,
        ..Layout::row()
    },));
    set_parent(world, body, root);
    let slot = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
            ..Layout::column()
        },
        SurfaceSlot { id: SLOT },
    ));
    set_parent(world, slot, body);
    let side = Surface::Raised
        .mount(world, body, UiId(2))
        .expect("side surface");
    if let Some(layout) = world.get_mut::<Layout>(side.root) {
        layout.width = Size::Px(SIDE);
        layout.height = Size::Fill;
    }
    let side = side.content;
    if let Some(layout) = world.get_mut::<Layout>(side) {
        layout.height = Size::Fill;
        layout.min_height = 0.0;
    }
    world.insert(side, Scroll::default());
    let editor = CanvasEditor::open(store, "board").expect("open board");
    view::install(world, CanvasView::new(editor, SLOT));
    panels::mount(world, toolbar, side, [WIDTH - 32.0, SIDE - 48.0]);
    app.add_system(Schedule::Update, |world: &mut World| {
        for request in take_action_intents(world) {
            panels::dispatch(world, &request);
        }
        view::update(world);
        panels::refresh(world);
    });
    app
}

fn export(path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = app(open_store(&[]));
    if let Some(v) = app.world_mut().resource_mut::<CanvasView>() {
        v.editor.camera = Camera {
            pan_x: 20.0,
            pan_y: 20.0,
            zoom: 1.0,
        };
        v.editor.select(&["circle".into()]);
    }
    for _ in 0..3 {
        rndr_paint::frame(&mut app);
    }
    let frame = rndr_paint::frame(&mut app);
    let image = Exporter::new()?.render_frame(
        Some(app.world()),
        &frame,
        WIDTH as u32,
        HEIGHT as u32,
        1.0,
    )?;
    std::fs::write(path, encode_png(&image)?)?;
    println!("wrote {path} ({}x{})", image.width, image.height);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(path) = args
        .iter()
        .position(|a| a == "--export")
        .and_then(|i| args.get(i + 1))
    {
        if let Err(err) = export(path) {
            eprintln!("export: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut app = app(open_store(&args));
    app.world_mut().insert_resource(WindowConfig {
        title: "Canvas editor".into(),
        size: LogicalSize {
            width: WIDTH,
            height: HEIGHT,
        },
        ..WindowConfig::default()
    });
    rndr_render::run(app);
}
