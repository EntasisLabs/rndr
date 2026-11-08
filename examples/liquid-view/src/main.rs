//! A Liquid Markdown document rendered with `rndr_liquid_ui`.
//!
//! Buttons print the [`LiquidEvent`] they send to the host. `--light` uses
//! the light theme; `--export out.png` writes one frame and exits.
//!
//! ```sh
//! cargo run -p liquid-view
//! cargo run -p liquid-view -- --export liquid.png
//! ```
use rndr_core::*;
use rndr_liquid_ui::{dispatch, liquid_view, FeedEntry, LiquidEvent, LiquidHost, LiquidUiPlugin};
use rndr_render::{encode_png, Exporter};
use rndr_ui::{apply_theme_bridge, Appearance, UiPlugin, UiTheme};
use std::sync::Arc;

const WIDTH: f32 = 860.0;
const HEIGHT: f32 = 2400.0;

const DOC: &str = include_str!("../sample.md");

struct PrintHost;

impl LiquidHost for PrintHost {
    fn event(&mut self, _: &mut World, event: LiquidEvent) {
        println!("{event:?}");
    }

    fn feed(&mut self, _: &mut World, feed_id: &str, _: &str) -> Vec<FeedEntry> {
        if feed_id != "releases" {
            return Vec::new();
        }
        vec![
            FeedEntry {
                title: "rndr 0.4".into(),
                detail: Some("Markdown host slots and the UI kit.".into()),
                meta: Some("today".into()),
            },
            FeedEntry {
                title: "rndr 0.3".into(),
                detail: None,
                meta: Some("last month".into()),
            },
        ]
    }

    fn tile(&mut self, _: &mut World, feed: &str, field: Option<&str>) -> Option<String> {
        (feed == "ops" && field == Some("uptime")).then(|| "99.98%".into())
    }
}

fn app(light: bool, height: f32) -> App {
    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.add_plugin(LiquidUiPlugin::new(PrintHost));
    let theme = UiTheme {
        appearance: if light {
            Appearance::Light
        } else {
            Appearance::Dark
        },
        ..UiTheme::default()
    };
    apply_theme_bridge(app.world_mut(), theme);
    app.world_mut()
        .insert_resource(Viewport::new(WIDTH, height));
    let view = liquid_view(app.world_mut(), DOC);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    app.add_system(Schedule::Update, |world: &mut World| {
        for request in take_action_intents(world) {
            dispatch(world, &request);
        }
    });
    app
}

fn export(light: bool, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = app(light, HEIGHT);
    for _ in 0..3 {
        rndr_paint::frame(&mut app);
    }
    let frame = rndr_paint::frame(&mut app);
    let mut exporter = Exporter::new()?;
    let image =
        exporter.render_frame(Some(app.world()), &frame, WIDTH as u32, HEIGHT as u32, 1.0)?;
    std::fs::write(path, encode_png(&image)?)?;
    println!("wrote {path} ({}x{})", image.width, image.height);
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let light = args.iter().any(|a| a == "--light");
    if let Some(path) = args
        .iter()
        .position(|a| a == "--export")
        .and_then(|i| args.get(i + 1))
    {
        if let Err(err) = export(light, path) {
            eprintln!("export: {err}");
            std::process::exit(1);
        }
        return;
    }
    let mut app = app(light, 900.0);
    app.world_mut().insert_resource(WindowConfig {
        title: "Liquid view".into(),
        size: LogicalSize {
            width: WIDTH,
            height: 900.0,
        },
        ..WindowConfig::default()
    });
    rndr_render::run(app);
}
