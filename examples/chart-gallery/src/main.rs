//! Every `rndr_chart` family in one window, or written to a PNG.
//!
//! Hover a mark to dim the rest of its chart and print its tooltip.
//! `--export out.png` writes the gallery and exits. `--light` uses the light theme.
//!
//! ```text
//! cargo run -p chart-gallery
//! cargo run -p chart-gallery -- --export gallery.png
//! ```

use std::sync::Arc;

use rndr_chart::{
    paint, Anchor, Chart, ChartKind, ChartScene, ChartTheme, Hit, Item, LabelPosition, Matrix,
    Point, Series, Shape, TextItem, ValueLabels,
};
use rndr_core::{
    App, Color, Input, Layout, MotionWatch, Rect, Schedule, Size, SurfaceSlot, TextEngine,
    WindowConfig, World,
};
use rndr_paint::{Affine, Canvas, CanvasPainter, CanvasRegistry, FillRule, Path};
use rndr_render::{encode_png, Exporter};

const SLOT: u64 = 1;
const COLS: usize = 5;
const CELL_W: f32 = 320.0;
const CELL_H: f32 = 300.0;
const PAD: f32 = 16.0;
const TITLE_H: f32 = 28.0;

fn gallery() -> Vec<Chart> {
    let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun"];
    let north = Series::new("north", "North", vec![12.0, 19.0, 15.0, 24.0, 28.0, 31.0]);
    let south = Series::new("south", "South", vec![8.0, 11.0, 14.0, 12.0, 18.0, 22.0]);
    let cartesian = |kind: ChartKind, title: &str| {
        Chart::new(kind)
            .title(title)
            .categories(months)
            .series(north.clone())
            .series(south.clone())
    };
    let share = |kind: ChartKind, title: &str| {
        let mut c = Chart::new(kind)
            .title(title)
            .categories(["Search", "Social", "Email", "Direct"])
            .series(Series::new(
                "visits",
                "Visits",
                vec![42.0, 26.0, 18.0, 14.0],
            ));
        c.labels = ValueLabels::Value;
        c
    };
    let mut stacked = cartesian(ChartKind::Bar, "Bar · stacked");
    stacked.stacked = true;
    let mut donut = share(ChartKind::Donut, "Donut");
    donut.center_label = "Visits".into();
    donut.label_position = LabelPosition::Inside;
    let radar = Chart::new(ChartKind::Radar)
        .title("Radar")
        .categories(["Speed", "Power", "Range", "Comfort", "Price"])
        .series(Series::new("a", "Model A", vec![8.0, 6.0, 7.0, 5.0, 6.0]))
        .series(Series::new("b", "Model B", vec![5.0, 8.0, 5.0, 8.0, 4.0]));
    let radial = Chart::new(ChartKind::Radial)
        .title("Radial")
        .categories(["Done", "Goal"])
        .series(Series::new("progress", "Complete", vec![72.0, 100.0]));
    let mut scatter = Chart::new(ChartKind::Scatter).title("Scatter");
    scatter.points = (0..16)
        .map(|i| {
            let x = i as f64;
            Point {
                x,
                y: x * 1.6 + ((i * 7) % 5) as f64 * 2.0,
                group: Some(if i % 2 == 0 { "Even" } else { "Odd" }.into()),
            }
        })
        .collect();
    let mut heat = Chart::new(ChartKind::Heatmap).title("Heatmap");
    heat.labels = ValueLabels::Value;
    heat.matrix = Some(Matrix {
        rows: vec!["Mon".into(), "Tue".into(), "Wed".into(), "Thu".into()],
        cols: vec!["AM".into(), "Noon".into(), "PM".into(), "Eve".into()],
        values: vec![
            vec![3.0, 8.0, 5.0, 1.0],
            vec![4.0, 9.0, 6.0, 2.0],
            vec![2.0, 7.0, 8.0, 3.0],
            vec![5.0, 6.0, 9.0, 4.0],
        ],
    });
    vec![
        stacked,
        cartesian(ChartKind::Line, "Line"),
        cartesian(ChartKind::Area, "Area"),
        cartesian(ChartKind::Combo, "Combo"),
        share(ChartKind::Pie, "Pie"),
        donut,
        radar,
        radial,
        scatter,
        heat,
    ]
    .into_iter()
    .map(|c| c.validate().expect("gallery charts are valid"))
    .collect()
}

struct Gallery {
    charts: Vec<Chart>,
    theme: ChartTheme,
}

/// Hovered chart index and hit key.
#[derive(Default)]
struct Hover(Option<(usize, Option<String>)>);

impl Gallery {
    fn size() -> [f32; 2] {
        let rows = 10usize.div_ceil(COLS);
        [COLS as f32 * CELL_W, rows as f32 * CELL_H]
    }

    fn cell(i: usize) -> Rect {
        let (col, row) = (i % COLS, i / COLS);
        Rect {
            x: col as f32 * CELL_W + PAD,
            y: row as f32 * CELL_H + PAD + TITLE_H,
            w: CELL_W - PAD * 2.0,
            h: CELL_H - PAD * 2.0 - TITLE_H,
        }
    }

    fn draw(&self, world: &World, canvas: &mut Canvas, hover: &Hover) {
        let [w, h] = Self::size();
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        };
        canvas.fill(
            Path::round_rect(bounds, 0.0),
            background(&self.theme),
            FillRule::NonZero,
        );
        for i in 0..self.charts.len() {
            let r = Self::cell(i);
            let card = Rect {
                x: r.x - 8.0,
                y: r.y - TITLE_H - 4.0,
                w: r.w + 16.0,
                h: r.h + TITLE_H + 12.0,
            };
            canvas.fill(
                Path::round_rect(card, 12.0),
                self.theme.card,
                FillRule::NonZero,
            );
        }
        let titles = ChartScene {
            width: w,
            height: h,
            view: Affine::IDENTITY,
            items: self
                .charts
                .iter()
                .enumerate()
                .map(|(i, chart)| {
                    let r = Self::cell(i);
                    Item {
                        shape: Shape::Text(TextItem {
                            text: chart.title.clone(),
                            x: r.x,
                            y: r.y - TITLE_H / 2.0,
                            size: 15.0,
                            weight: 700,
                            color: self.theme.fg,
                            anchor: Anchor::Start,
                        }),
                        key: None,
                        active: true,
                        dim: 1.0,
                    }
                })
                .collect(),
            hits: Vec::new(),
            legend: Vec::new(),
            interactive: false,
        };
        paint(&titles, &self.theme, world, canvas, [0.0, 0.0], None);
        for i in 0..self.charts.len() {
            let (scene, origin) = self.placed(i);
            let key = match &hover.0 {
                Some((h, key)) if *h == i => key.as_deref(),
                _ => None,
            };
            paint(&scene, &self.theme, world, canvas, origin, key);
        }
    }

    fn placed(&self, i: usize) -> (ChartScene, [f32; 2]) {
        let r = Self::cell(i);
        let chart = &self.charts[i];
        let scene = ChartScene::new(chart, &self.theme, r.w, chart.default_height(r.w).min(r.h));
        let y = r.y + (r.h - scene.height).max(0.0) / 2.0;
        (scene, [r.x, y])
    }

    fn hit(&self, p: [f32; 2]) -> Option<(usize, Hit)> {
        (0..self.charts.len()).find_map(|i| {
            let (scene, [x, y]) = self.placed(i);
            scene.hit([p[0] - x, p[1] - y]).cloned().map(|h| (i, h))
        })
    }
}

fn background(theme: &ChartTheme) -> Color {
    if theme.fg.r > 0.5 {
        Color::rgb(0.035, 0.05, 0.09)
    } else {
        Color::rgb(0.94, 0.95, 0.97)
    }
}

struct Painter(Arc<Gallery>);

impl CanvasPainter for Painter {
    fn paint(&self, world: &World, canvas: &mut Canvas) {
        let empty = Hover::default();
        let hover = world.resource::<Hover>().unwrap_or(&empty);
        self.0.draw(world, canvas, hover);
    }
}

fn install_text(world: &mut World) {
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    world.insert_resource(engine);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let theme = if args.iter().any(|a| a == "--light") {
        ChartTheme::light()
    } else {
        ChartTheme::dark()
    };
    let theme = ChartTheme {
        family: Some("Inter".into()),
        ..theme
    };
    let gallery = Arc::new(Gallery {
        charts: gallery(),
        theme,
    });
    let export = args
        .iter()
        .position(|a| a == "--export")
        .and_then(|i| args.get(i + 1).cloned());
    if let Some(path) = export {
        if let Err(err) = run_export(&gallery, &path) {
            eprintln!("export: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut app = App::new();
    install_text(app.world_mut());
    let mut registry = CanvasRegistry::new();
    registry.insert(SLOT, Painter(Arc::clone(&gallery)));
    app.world_mut().insert_resource(registry);
    app.world_mut().insert_resource(Hover::default());
    app.world_mut()
        .insert_resource(MotionWatch { active: true });
    app.world_mut().insert_resource(gallery);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, track_hover);
    let [w, h] = Gallery::size();
    app.world_mut().insert_resource(WindowConfig {
        title: "Chart gallery".into(),
        size: rndr_core::LogicalSize {
            width: w,
            height: h,
        },
        ..WindowConfig::default()
    });
    rndr_render::run(app);
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

fn track_hover(world: &mut World) {
    let input = world.resource::<Input>().cloned().unwrap_or_default();
    let Some(gallery) = world.resource::<Arc<Gallery>>().cloned() else {
        return;
    };
    let hit = gallery.hit(input.pointer.position);
    let next = hit.as_ref().map(|(i, h)| (*i, h.key.clone()));
    let Some(hover) = world.resource_mut::<Hover>() else {
        return;
    };
    if hover.0 != next {
        if let Some((_, h)) = &hit {
            let lines: Vec<String> = h
                .lines
                .iter()
                .map(|l| format!("{}: {}", l.label, l.value))
                .collect();
            println!("{} — {}", h.title, lines.join(", "));
        }
        hover.0 = next;
    }
}

fn run_export(gallery: &Gallery, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    install_text(app.world_mut());
    let mut canvas = Canvas::new();
    gallery.draw(app.world(), &mut canvas, &Hover::default());
    let scene = canvas.into_scene();
    let [w, h] = Gallery::size();
    let mut exporter = Exporter::new()?;
    let image = exporter.render_scene(
        app.world(),
        &scene,
        Rect {
            x: 0.0,
            y: 0.0,
            w,
            h,
        },
        2.0,
    )?;
    std::fs::write(path, encode_png(&image)?)?;
    println!("wrote {path} ({}x{})", image.width, image.height);
    Ok(())
}
