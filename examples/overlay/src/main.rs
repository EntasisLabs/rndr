//! Frameless launcher panel.
//!
//! The window is centered, always on top, and hidden on blur and on Escape.
//! A two-second timer shows it again. That timer stands in for a global hotkey.
//!
//! ```text
//! cargo run -p overlay
//! ```

use std::time::Duration;

use rndr_core::{
    events, every, Axis, Color, Dismiss, Event, Label, Layout, LogicalSize, Position, Schedule,
    Size, WindowConfig, WindowKind, World,
};
use rndr_render::run;

#[derive(Clone, Copy, Debug)]
struct Reveal;

impl Event for Reveal {}

fn setup(world: &mut World) {
    world.spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 24.0,
            width: Size::Fill,
            height: Size::Fill,
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.12, 0.13, 0.16),
        Label::new("overlay"),
    ));
}

fn reveal(world: &mut World) {
    if events::<Reveal>(world).is_empty() || rndr_core::window::is_visible(world) {
        return;
    }
    rndr_core::window::show(world);
    rndr_core::window::focus(world);
}

fn main() {
    let mut app = rndr_core::App::new();
    app.world_mut().insert_resource(WindowConfig {
        title: "overlay".to_string(),
        size: LogicalSize::new(480.0, 270.0),
        position: Position::Center,
        resizable: false,
        decorations: false,
        always_on_top: true,
        skip_taskbar: true,
        kind: WindowKind::Panel,
        hide_on_blur: true,
        dismiss: Dismiss::Hide,
        accessory: true,
        ..WindowConfig::default()
    });
    app.add_system(Schedule::Startup, setup);
    app.add_system(Schedule::Update, reveal);
    // Held across `run` so dropping it does not cancel the timer.
    let _reveal = every(app.world_mut(), Duration::from_secs(2), Reveal);
    run(app);
}
