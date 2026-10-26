# rndr_paint

Turns an [`rndr_core`](https://docs.rs/rndr_core) world into colored quads in logical points, and records a display list of paths, images, and layers. No window and no GPU.

```rust
let frame = rndr_paint::frame(&mut app);
```

`frame` runs one update and returns the quads plus any foreign-surface rectangles. A bitmap atlas for Latin text ships in the crate. It is derived from Inter Regular. The SIL Open Font License text is `Inter-LICENSE.txt`.

## Text and backgrounds

A `Label` needs layout geometry (`Rect`, or `Layout` when using `frame`),
but does not need a `Color` component. Text uses `Label::color` when set,
otherwise `Theme::text`. An entity's `TextStyle` controls shaping, not its
background or text color.

`Color` is an optional background fill. Omitting it emits no background quad;
an explicit transparent `Color` keeps its transparent fill and does not hide
text. Use `Hidden` to hide an entity and its descendants. Existing clipping,
selection, caret, and decoration behavior applies with or without a background.

The headless `uncolored_text` integration tests exercise both `paint`'s fallback
measurement and `frame`'s layout path, checking glyph UVs, visible color, and
positive render geometry. They do not replace native screenshot/GPU inspection.

The `highlight` feature forwards `rndr_core`'s fenced-code token colors.

License: MIT AND OFL-1.1.
