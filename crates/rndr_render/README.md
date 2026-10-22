# rndr_render

winit window and wgpu presenter for [rndr](https://github.com/EntasisLabs/rndr). The runner writes input in logical points and draws the quad list from `rndr_paint`.

```rust
rndr_render::run(app);
```

Default features:

- `hotkey` — system-wide chords on macOS, Windows, X11, and the Wayland portal
- `tray` — menu bar or notification area icon
- `parley` — text shaping, plus bundled Inter, Noto Sans Arabic, and Noto Color Emoji

`--no-default-features` keeps the builtin bitmap text engine and skips fontconfig, the hotkey backends, and the tray icon.

docs.rs builds every feature. Its Linux image includes fontconfig, libxkbcommon, Wayland, and Vulkan. The hotkey and tray features do not link libdbus.

Font licenses are in `fonts/`.

License: MIT AND OFL-1.1.
