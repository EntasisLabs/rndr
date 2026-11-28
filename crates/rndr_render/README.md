# rndr_render

winit window and wgpu presenter for [rndr](https://github.com/EntasisLabs/rndr). The runner writes input in logical points and draws the frame from `rndr_paint`: signed-distance quads, plus paths, images, and layers when the frame records them.

```rust
rndr_render::run(app);
```

## Native child surfaces

`run(app)` remains unchanged. Applications needing a UI-thread child surface can
opt into `Session::new(app).native_host(factory).run()`. The factory receives
`&mut rndr_core::World` and returns `Box<dyn rndr_render::NativeHost>` per window,
including windows opened later. Neither factory nor host needs `Send` or `Sync`.
Keep the native child in the host, not a cross-thread resource.

Implement `NativeHost::frame(&mut self, &mut World, NativeHostContext<'_>) ->
Option<Duration>`. The borrowed context provides:

- `window: &dyn winit::window::Window`: use `ArtifactPreview::attach(&context.window, html)`;
- `physical_size: [u32; 2]`: client extent in physical pixels;
- `scale_factor: f64`: physical pixels per logical point;
- `visible: bool`: false for hidden, occluded or zero-area parents;
- `request_redraw()`: request one full application update and paint when host
  state changes an ECS label or other UI state, also while hidden.

The host runs before waiting, after app/native work. A returned delay requests
another host poll without continuously redrawing the app; `None` waits for the
next external/app event. Zero is clamped to 1 ms. Poll while policy/load work is
pending, and periodically after Ready if asynchronous process failures must be
detected while the app is idle. Convert physical top-left subrects to logical
coordinates by dividing by `scale_factor`; update bounds on every host pass.
Hide the child when `visible` is false. Use a host-owned toolbar outside the
child's native subrect, since native child views overlay RNDR-painted content.

`NativeHost::detach(&mut self, &mut World)` retires children before parent fields
are destroyed, then RNDR immediately drops the host on the UI thread. It also
runs when no native parent was available. Drop/recreate a preview on anchor or
surface changes, and do not retain the borrowed parent/handle past the callback.
Wayland layer-shell windows have no winit parent and receive no `frame` callbacks.

The renderer adds **no browser backend dependency** in production and never
falls back to ordinary browsing. Private WebKit isolation stays compile-time
opt-in in `rndr_browser`. See [isolated artifacts](../rndr_browser/docs/isolated-artifacts.md).
The [native host example](../../examples/native-artifact-host/src/main.rs) demonstrates attach,
physical bounds, scale, asynchronous state, replacement and cleanup:

```sh
# Unsupported/no fallback, but the host lifecycle still runs:
cargo run -p native-artifact-host -- --smoke
# Explicit private-WebKit opt-in on macOS:
cargo run -p native-artifact-host --features rndr_browser/macos-private-webkit-artifacts -- --smoke
```

Omit `--smoke` for local button/input interaction. The timed smoke hides/shows,
replaces, resizes and exits; it does not certify interactive input or a multi-DPI
monitor transition.

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
