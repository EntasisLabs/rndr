# rndr_term

Embedded terminal session for [rndr](https://github.com/EntasisLabs/rndr).

`TerminalPlugin` registers input and update systems. `open_terminal` spawns a
shell on an entity through a pseudoterminal (ConPTY on Windows) and copies the
visible cells into a `MonoGrid`. Unchanged rows are not shaped again.

The state machine is `alacritty_terminal`. The PTY is `portable-pty`. Neither
dependency is linked from `rndr_core` or `rndr_paint`.

```rust
use rndr_core::App;
use rndr_term::{open_terminal, TerminalOptions, TerminalPlugin};

let mut app = App::new();
app.add_plugin(TerminalPlugin);
let entity = app.world_mut().spawn(rndr_core::Color::rgb(0.05, 0.05, 0.06));
open_terminal(app.world_mut(), entity, TerminalOptions::shell())?;
```

`cargo run -p terminal` opens a window running the login shell.

License: MIT.
