# rndr

## Semantic components and observation

Enable optional feature `ui` to use `rndr::ui::{UiPlugin, UiTheme}` and
`rndr::ui::controls::Button`, or depend on `rndr_ui` directly. No semantic glob
re-export is added: legacy root `Button`, `Theme`, `List` and tray `Item` keep
their types; collection items live at `rndr::ui::collections::Item`.
`UiPlugin` installs canonical interaction, scrolling and styling, not Shell.
See the [UI README](../rndr_ui/README.md) and [current status](../../docs/ui-status.md).

`observe` remains off by default. With an explicit app session/content policy,
it exposes live whole-window surface pixels and the matching flattened
accessibility tree over a Unix socket. It is not Windows transport, UiId/layout
or component-targeted/headless capture, nor compositor presentation proof.
[Native observation](../../docs/native-observation.md) documents fail-closed
whole-frame denial, limitations and the Python client/tests.

Publication is not enabled by this integration: `rndr_ui` is `publish = false`
and this facade's optional local UI dependency has no registry version/release
policy. CI checks facade builds/doctests but intentionally does not claim its
tarball can be published. Existing release instructions below are historical
until that dependency policy is resolved.

Desktop application toolkit. One crate re-exports the entity world, the paint step, the `std::fs` plugin, and the winit + wgpu window.

```rust
use rndr::{App, WindowConfig};

fn main() {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig::default());
    rndr::run(app);
}
```

`hotkey`, `tray`, `parley`, and `accesskit` are on by default, matching `rndr_render`. `dialog` is off by default and shows native open and save dialogs. Turn the defaults off with `--no-default-features`. A headless program can depend on `rndr_core` instead of this crate.

Repository: <https://github.com/EntasisLabs/rndr>

License: MIT.
