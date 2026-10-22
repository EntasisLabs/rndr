# rndr_paint

Turns an [`rndr_core`](https://docs.rs/rndr_core) world into colored quads in logical points. No window and no GPU.

```rust
let frame = rndr_paint::frame(&mut app);
```

`frame` runs one update and returns the quads plus any foreign-surface rectangles. A bitmap atlas for Latin text ships in the crate.

License: MIT.
