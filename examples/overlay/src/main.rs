//! Frameless launcher panel.
//!
//! The window is centered, always on top, and hidden on blur and on Escape.
//! Alt+Space toggles it from any application. `RNDR_HOTKEY` or `--hotkey`
//! overrides that chord.
//!
//! ```text
//! cargo run -p overlay
//! cargo run -p overlay -- --hotkey Ctrl+Space
//! ```

use rndr_core::{
    Axis, Color, Dismiss, Label, Layout, LogicalSize, Position, Schedule, Size, WindowConfig,
    WindowKind, World,
};
use rndr_render::{configured_hotkey, run, toggle_window};

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
    // Held across `run`. Dropping it releases the grab.
    let binding = configured_hotkey(std::env::args().skip(1));
    let _hotkey = match toggle_window(app.world_mut(), binding.as_str()) {
        Ok(handle) => handle,
        Err(err) => {
            eprintln!("hotkey {binding}: {err}");
            std::process::exit(1);
        }
    };
    run(app);
}
