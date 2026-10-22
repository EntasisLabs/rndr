# rndr

Desktop application toolkit. One crate re-exports the entity world, the paint step, the `std::fs` plugin, and the winit + wgpu window.

```rust
use rndr::{App, WindowConfig};

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig::default());
    rndr::run(app);
}
```

`hotkey`, `tray`, and `parley` are on by default, matching `rndr_render`. Turn them off with `--no-default-features`. A headless program can depend on `rndr_core` instead of this crate.

Repository: <https://github.com/EntasisLabs/rndr>

License: MIT.
