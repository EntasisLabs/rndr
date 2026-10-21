//! Idle window.
//!
//! With no arguments the loop uses `ControlFlow::Wait` and should sit near 0% CPU.
//! `--tick` arms a one-second timer so the loop waits with `ControlFlow::WaitUntil`
//! and still should not spin.
//!
//! ```text
//! cargo run -p idle
//! cargo run -p idle -- --tick
//! ```
//!
//! Watch the process in `top` or Activity Monitor after the window is up and you
//! are not moving the cursor over it.

use std::time::Duration;

use rndr_core::{every, App, Event};
use rndr_render::run;

#[derive(Clone, Copy, Debug)]
struct Heartbeat;

impl Event for Heartbeat {}

fn main() {
    let mut app = App::new();
    // Held across `run` so dropping it does not cancel the timer.
    let _heartbeat = std::env::args()
        .any(|arg| arg == "--tick")
        .then(|| every(app.world_mut(), Duration::from_secs(1), Heartbeat));
    run(app);
}
