# rndr_app

Starter helpers for a desktop application built on [`rndr_core`](https://docs.rs/rndr_core).

This is not the component library. Semantic controls live in [`rndr_ui`](https://docs.rs/rndr_ui). The facade does not depend on this crate. Install [`FormNavigation`] before your own Update systems. It installs core's chronological form policy, scrolling, and scrollbars. A second registration of those scroll systems is safe.

```rust
use rndr_app::FormNavigation;
use rndr_core::App;

let mut app = App::new();
app.add_plugin(FormNavigation);
```

`FormNavigation` walks visible, enabled controls with Tab and Shift-Tab. Fields created by the kit opt into field transactions. Buttons use release activation. The helpers return entity ids. They do not install `Shell`, a second input router, or a renderer.

`Preferences` reads bounded JSON (16 KiB) and saves through a sibling temporary file, then rename. On Unix the temporary file is mode 0600. A failed write leaves the previous file in place. It is not secure storage, encryption, multi-writer synchronization, schema migration, or a crash-durable database. Do not put credentials in these fields.

`ViewState` is explicit Empty, Loading, Ready, Error, and Cancelled copy. It does not own application transactions. The caller owns task cancellation.

The runnable reference is Desktop Workshop, `examples/desktop` in the [rndr repository](https://github.com/EntasisLabs/rndr/blob/main/docs/desktop-apps.md). That example stays `publish = false`. Its macOS bundle version string is example metadata. Native IME, clipboard, VoiceOver, and signed distribution are not certified by the headless tests. See the [readiness notes](https://github.com/EntasisLabs/rndr/blob/main/docs/desktop-readiness.md).

0.4.0 publishes this crate with the rest of the workspace. The version is a 0.y release, not a 1.0 stability promise.

License: MIT.
