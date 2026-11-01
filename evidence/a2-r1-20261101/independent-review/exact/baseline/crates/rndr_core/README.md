# rndr_core

Entity world for [rndr](https://github.com/EntasisLabs/rndr): hierarchy, layout, input, text, events, and tasks. No window and no GPU.

```rust
use rndr_core::App;

let mut app = App::new();
app.update();
```

The `pool` feature adds a small thread-pool executor. The desktop runner turns it on. The `tokio` feature spawns a feed on a runtime the application already started. The default build depends on neither.

License: MIT.
