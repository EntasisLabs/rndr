# rndr_core

Entity world for [rndr](https://github.com/EntasisLabs/rndr): hierarchy, layout, input, text, events, and tasks. No window and no GPU.

```rust
use rndr_core::App;

let mut app = App::new();
app.update();
```

The `pool` feature adds a small thread-pool executor. The desktop runner turns it on. This crate does not depend on tokio.

License: MIT.
