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

0.6.0 publishes this crate with the sixteen-crate SDK set (including
`rndr-flow` as a separate optional dependency, not a facade feature). `ui`,
`observe`, `browser`, `media`, `chart`, `liquid`, and `canvas` are off by
default. `ui` depends on published `rndr_ui`. `observe` enables the Unix
whole-window probe and headless component capture. Examples stay unpublished.
This is a 0.y release, not a 1.0 stability promise. See
[package policy](../../docs/ui-status.md#package-policy).

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
