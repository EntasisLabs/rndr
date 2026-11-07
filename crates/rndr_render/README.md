# rndr_render

winit window and wgpu presenter for [rndr](https://github.com/EntasisLabs/rndr). The runner writes input in logical points and draws the frame from `rndr_paint`: signed-distance quads, plus paths, images, and layers when the frame records them.

```rust
rndr_render::run(app);
```

Default features:

- `hotkey` — system-wide chords on macOS, Windows, X11, and the Wayland portal
- `tray` — menu bar or notification area icon
- `parley` — text shaping, plus bundled Inter, Noto Sans Arabic, and Noto Color Emoji
- `accesskit` — accessibility tree for the widgets in the window

`dialog` is off by default. It shows native open and save dialogs. On Linux that dialog uses the file-chooser portal.

`notify` is off by default. It presents and dismisses a notification identified by a host-owned token. With the feature off, the call returns unsupported and does not present. A click returns that token and does not focus a window. Linux uses the desktop notification service when it is running. macOS and Windows do not present from this crate: macOS has no recorded permission session, and Windows toast needs an application-owned package identity that this repository does not create. See [notifications](../../docs/notifications.md).

File hover and drop follow winit 0.31 drag events. A finite client point becomes a logical `position`. Wayland is unsupported. The binding table and the native ledger are in [file drop](../../docs/file-drop.md).

`--no-default-features` keeps the builtin bitmap text engine and skips fontconfig, the hotkey backends, the tray icon, and the accessibility adapter.

docs.rs builds every feature. Its Linux image includes fontconfig, libxkbcommon, Wayland, and Vulkan. The hotkey and tray features do not link libdbus.

Opt-in `observe` also exposes `capture::capture` and the portable `rndr-capture`
CLI: offscreen Entity/scoped-UiId pixels plus same-frame subtree layout/identity,
with whole-frame privacy denial. This creates no window/socket and is not native
presentation evidence. See [component capture](../../docs/component-capture.md)
for API, scene format, examples, clipping/scale/error semantics and limitations.

Font licenses are in `fonts/`.

License: MIT AND OFL-1.1.
