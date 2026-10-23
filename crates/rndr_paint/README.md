# rndr_paint

Turns an [`rndr_core`](https://docs.rs/rndr_core) world into colored quads in logical points, and records a display list of paths, images, and layers. No window and no GPU.

```rust
let frame = rndr_paint::frame(&mut app);
```

`frame` runs one update and returns the quads plus any foreign-surface rectangles. A bitmap atlas for Latin text ships in the crate. It is derived from Inter Regular. The SIL Open Font License text is `Inter-LICENSE.txt`.

The `highlight` feature forwards `rndr_core`'s fenced-code token colors.

License: MIT AND OFL-1.1.
