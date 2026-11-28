//! Idle window.
//!
//! With no arguments the loop uses `ControlFlow::Wait` and should sit near 0% CPU.
//! `--tick` arms a one-second timer so the loop waits with `ControlFlow::WaitUntil`
//! and still should not spin.
//!
//! ```text
//! cargo run -p idle
//! cargo run -p idle -- --tick
//! cargo run --release -p idle -- --panel
//! ```
//!
//! For a native memory comparison, run the release build once normally and
//! once with `--panel` (a 120×120-point, half-opacity group). Keep the window
//! size and display scale the same and let each process settle before reading
//! its physical footprint, e.g. `footprint --noCategories <pid>` on macOS.
//!
//! Watch the process in `top` or Activity Monitor after the window is up and you
//! are not moving the cursor over it.

use std::time::Duration;

use rndr_core::{every, set_parent, App, Color, Event, Layout, Opacity, Size};
use rndr_render::run;

#[derive(Clone, Copy, Debug)]
struct Heartbeat;

impl Event for Heartbeat {}

fn main() {
    let mut app = App::new();
    if std::env::args().any(|arg| arg == "--panel") {
        let root = app.world_mut().spawn((Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 32.0,
            ..Layout::row()
        },));
        let panel = app.world_mut().spawn((
            Layout {
                width: Size::Px(120.0),
                height: Size::Px(120.0),
                ..Layout::row()
            },
            Color::rgb(0.2, 0.45, 1.0),
            Opacity(0.5),
        ));
        set_parent(app.world_mut(), panel, root);
    }
    // Held across `run` so dropping it does not cancel the timer.
    let _heartbeat = std::env::args()
        .any(|arg| arg == "--tick")
        .then(|| every(app.world_mut(), Duration::from_secs(1), Heartbeat));
    run(app);
}
