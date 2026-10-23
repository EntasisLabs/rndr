//! Two windows in one process. Each has a Save button. Pressing it tells the
//! other window, which replaces its status label.

use rndr_core::{
    desktop, events, Axis, Button, Chrome, Color, Desktop, Event, Focus, Label, Layout, Schedule,
    Size, WindowConfig, WindowMsg,
};
use rndr_render::Session;

fn main() {
    let mut first = rndr_core::App::new();
    first.add_plugin(Chrome);
    first.world_mut().insert_resource(window("One"));
    first.add_system(Schedule::Startup, setup);
    first.add_system(Schedule::Update, exchange);
    let mut second = rndr_core::App::new();
    second.add_plugin(Chrome);
    second.world_mut().insert_resource(window("Two"));
    second.add_system(Schedule::Startup, setup);
    second.add_system(Schedule::Update, exchange);
    Session::new(first).window(second).run();
}

fn window(title: &str) -> WindowConfig {
    WindowConfig {
        title: title.into(),
        ..WindowConfig::default()
    }
}

fn setup(world: &mut rndr_core::World) {
    let root = world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 8.0,
            padding: 16.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.12, 0.13, 0.16),
    ));
    let button = world.spawn((
        Layout {
            axis: Axis::Row,
            gap: 0.0,
            padding: 8.0,
            width: Size::Px(120.0),
            height: Size::Px(32.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Button::new("save"),
        Label::new("Save"),
        Color::rgb(0.2, 0.35, 0.55),
    ));
    let status = world.spawn((
        Layout {
            axis: Axis::Row,
            gap: 0.0,
            padding: 0.0,
            width: Size::Fill,
            height: Size::Px(24.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Label::new("waiting"),
        Color::rgb(0.9, 0.9, 0.9),
    ));
    rndr_core::set_parent(world, button, root);
    rndr_core::set_parent(world, status, root);
    world.insert_resource(Status { entity: status });
    world.insert_resource(Focus {
        entity: Some(button),
    });
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Saved {
    text: String,
}

impl Event for Saved {}

struct Status {
    entity: rndr_core::Entity,
}

fn exchange(world: &mut rndr_core::World) {
    let fired = world
        .resource::<rndr_core::Fired>()
        .and_then(|fired| fired.name.clone());
    if fired.as_deref() == Some("save") {
        let text = world
            .resource::<Desktop>()
            .map(|desktop| format!("saved by window {}", desktop.id().0))
            .unwrap_or_else(|| "saved".into());
        if let Some(desktop) = desktop(world) {
            desktop.broadcast(Saved { text });
        }
    }
    let status = world.resource::<Status>().map(|status| status.entity);
    let text = events::<WindowMsg<Saved>>(world)
        .last()
        .map(|message| message.body.text.clone());
    let (Some(status), Some(text)) = (status, text) else {
        return;
    };
    if let Some(label) = world.get_mut::<Label>(status) {
        label.text = text;
    }
}
