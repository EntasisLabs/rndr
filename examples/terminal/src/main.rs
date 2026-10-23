//! Shell hosted in a window.
//!
//! ```text
//! cargo run -p terminal
//! cargo run -p terminal -- /bin/bash
//! ```
//!
//! With no arguments this starts `$SHELL` (or `sh` / `cmd.exe`). Extra
//! arguments are passed to that program. The grid follows the window size.
//! Shift+PageUp and Shift+PageDown scroll the history. Copy is Ctrl+Shift+C
//! on Windows and Linux, and Cmd+C on macOS. Paste is Ctrl+Shift+V or Cmd+V.

use rndr_core::{App, Color, LogicalSize, WindowConfig};
use rndr_render::run;
use rndr_term::{open_terminal, TerminalOptions, TerminalPlugin};

fn main() {
    let mut args = std::env::args().skip(1);
    let program = args.next();
    let rest: Vec<String> = args.collect();

    let mut app = App::new();
    app.add_plugin(TerminalPlugin);
    let mut options = TerminalOptions::shell();
    if let Some(program) = program {
        options.program = program;
        options.args = rest;
    }
    let entity = app.world_mut().spawn(Color::rgb(0.05, 0.05, 0.06));
    open_terminal(app.world_mut(), entity, options).expect("open terminal");

    app.world_mut().insert_resource(WindowConfig {
        title: "terminal".into(),
        size: LogicalSize::new(960.0, 640.0),
        ..WindowConfig::default()
    });
    run(app);
}
