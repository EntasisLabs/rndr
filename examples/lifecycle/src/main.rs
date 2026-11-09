//! Lifecycle: run it twice and the second launch hands over to the first.
//!
//! `cargo run -p lifecycle -- myapp://hello` while one runs shows the link in
//! the running window; a launch without a link brings that window forward.
//! On macOS, register the scheme in a bundle's `CFBundleURLTypes` and run
//! `open myapp://hello`; clicking the Dock icon sends `Reopened`. The window
//! and Dock show the icon set with `set_app_icon`.

use rndr_core::{
    events, App, Entity, Label, Layout, Reopened, Schedule, Size, UrlOpened, WindowConfig, World,
};
use rndr_render::{set_app_icon, single_instance, Session};
use rndr_ui::{Token, UiPlugin, UiStyle};

struct Log(Entity);

fn main() {
    if !single_instance("dev.rndr.lifecycle-example") {
        println!("handed over to the running instance");
        return;
    }
    if let Err(error) = set_app_icon(include_bytes!("../icon.png").as_slice()) {
        eprintln!("app icon: {error:?}");
    }
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "Lifecycle".into(),
        ..WindowConfig::default()
    });
    app.add_plugin(UiPlugin);
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, show_arrivals);
    Session::new(app).run();
}

fn setup(world: &mut World) {
    let root = world.spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 24.0,
            gap: 8.0,
            ..Layout::column()
        },
        UiStyle {
            background: Some(Token::Canvas.into()),
            ..UiStyle::default()
        },
    ));
    let hint = world.spawn((
        Layout::row(),
        Label::new("Launch again with a myapp:// link, or without one."),
        ink(),
    ));
    rndr_core::set_parent(world, hint, root);
    let log = world.spawn((Layout {
        gap: 4.0,
        ..Layout::column()
    },));
    rndr_core::set_parent(world, log, root);
    world.insert_resource(Log(log));
}

fn show_arrivals(world: &mut World) {
    let mut lines: Vec<String> = events::<UrlOpened>(world)
        .iter()
        .map(|url| format!("Opened {}", url.0))
        .collect();
    lines.extend(
        events::<Reopened>(world)
            .iter()
            .map(|_| "Reopened".to_owned()),
    );
    let Some(log) = world.resource::<Log>().map(|log| log.0) else {
        return;
    };
    for line in lines {
        println!("{line}");
        let row = world.spawn((Layout::row(), Label::new(line), ink()));
        rndr_core::set_parent(world, row, log);
    }
}

fn ink() -> UiStyle {
    UiStyle {
        foreground: Some(Token::Text.into()),
        ..UiStyle::default()
    }
}
