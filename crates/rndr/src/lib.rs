//! Desktop application toolkit.
//!
//! This crate re-exports [`rndr_core`], [`rndr_paint`], [`rndr_std`], and
//! [`rndr_render`] so `cargo add rndr` builds a window. `term` and `ui` are
//! extra features. `rndr_app` is a separate published starter crate. Headless
//! programs can depend on `rndr_core` alone and skip the window and the GPU.
//!
//! 0.4.0 publishes this crate together with `rndr_core`, `rndr_paint`,
//! `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, and `rndr_render`. `ui` and
//! `observe` are off by default. `ui` uses `rndr_ui`. This is a 0.y release,
//! not a 1.0 stability promise. Examples in the repository are not published.
//!
//! # How the crates fit
//!
//! - [`rndr_core`] owns the world: entities, hierarchy, layout, input, text,
//!   events, and tasks. It has no GPU types.
//! - [`rndr_paint`] turns one frame of that world into [`Quad`]s in logical
//!   points, plus a display list of paths, images, and layers. It does not
//!   open a window.
//! - [`rndr_std`] inserts an [`Fs`] backend that reads and writes the process
//!   filesystem.
//! - [`rndr_render`] is the only OS window and GPU. It writes [`Input`] and
//!   [`Viewport`], presents the frame, and (with `observe`) can capture a
//!   component offscreen.
//! - `rndr_term` is the optional pseudoterminal, re-exported when `term` is on.
//! - `rndr_ui` and `rndr_app` are opt-in kits. Both depend only on `rndr_core`.
//!   `rndr_ui` is the component library. `rndr_app` is a starter, not that library.
//!
//! # Example
//!
//! ```no_run
//! use rndr::{
//!     run, App, Axis, Color, Label, Layout, LogicalSize, Schedule, Size, WindowConfig,
//! };
//!
//! fn setup(world: &mut rndr::World) {
//!     world.spawn((
//!         Layout {
//!             axis: Axis::Column,
//!             gap: 0.0,
//!             padding: 24.0,
//!             width: Size::Fill,
//!             height: Size::Fill,
//!             min_width: 0.0,
//!             min_height: 0.0,
//!         },
//!         Color::rgb(0.12, 0.13, 0.16),
//!         Label::new("hello"),
//!     ));
//! }
//!
//! fn main() {
//!     let mut app = App::new();
//!     app.world_mut().insert_resource(WindowConfig {
//!         title: "hello".into(),
//!         size: LogicalSize::new(640.0, 480.0),
//!         ..WindowConfig::default()
//!     });
//!     app.add_system(Schedule::Startup, setup);
//!     run(app);
//! }
//! ```
//!
//! # Features
//!
//! Desktop defaults mirror `rndr_render`; additional opt-ins are listed below:
//!
//! - `hotkey` grabs system-wide chords on macOS, Windows, X11, and the Wayland
//!   global shortcuts portal.
//! - `tray` adds a menu-bar or notification-area icon.
//! - `parley` shapes and rasterizes text, and links fontconfig on Linux.
//! - `accesskit` publishes the accessibility tree. It is on by default.
//! - `dialog` shows native open and save dialogs. It is off by default.
//! - `notify` presents and dismisses notifications and reports a click token.
//!   It is off by default. With the feature off, the entry point returns
//!   [`rndr_render::NotifyError::Unsupported`] and does not present. A click
//!   does not activate a window. Enabling it does not enable `dialog` or `browser`.
//! - `term` re-exports the embedded terminal. It is off by default, so a
//!   normal desktop build does not link a pseudoterminal.
//! - `browser` re-exports the embedded browser. It is off by default, so a
//!   normal desktop build does not link a webview. It does not enable `dialog`.
//!   macOS WKWebView, Windows WebView2, and WebKitGTK on X11 live in that crate.
//!   WebKitGTK on Wayland, find, and script stay unsupported. A missing WebView2
//!   runtime or GTK display is an error. The crate does not download a runtime.
//!   A headless test does not certify a platform view.
//! - `ui` is off by default and exposes semantic components at `rndr::ui`, never
//!   glob-reexporting their names into the legacy root API.
//! - `observe` is off by default. It enables two different checks. The Unix
//!   socket probe records what the live window submitted, plus a flattened
//!   accessibility tree. Headless component capture renders a target in
//!   process: `capture` crops the viewport composite, and `capture_isolated`
//!   paints that subtree on its own. The live probe has no non-Unix transport.
//!   Capture metadata is geometry. It does not include accessible names.
//! - `tokio` is off by default. It forwards to `rndr_core` and spawns a feed
//!   producer on a tokio runtime the application already started.
//!
//! [`register`] and [`toggle_window`] install the operating-system hotkey
//! backend. The parser and the registry stay at [`hotkey`].
//!
//! docs.rs builds this crate with every feature. That Linux image provides
//! fontconfig, libxkbcommon, Wayland, and Vulkan. `hotkey` and `tray` do not
//! link libdbus.

#![deny(ambiguous_glob_reexports)]
#![deny(missing_docs)]

pub use rndr_core::*;
pub use rndr_paint::*;
pub use rndr_render::*;
pub use rndr_std::*;

/// Semantic UI components, enabled with the `ui` feature.
///
/// Legacy core names stay at the root; semantic names are explicitly scoped.
/// Collections keep a bounded retained list. `ListView::mount_virtual` is the
/// viewport window over a longer logical list.
///
/// ```
/// use rndr::{App, Button, Item, List, Theme}; // existing core/tray API
/// use rndr::ui::{UiPlugin, UiTheme};
/// use rndr::ui::controls::Button as UiButton;
/// use rndr::ui::collections::{Item as UiItem, ListView};
/// let mut app = App::new();
/// app.add_plugin(UiPlugin);
/// let _ = std::any::type_name::<(Button, Item, List, Theme)>();
/// let _ = std::any::type_name::<(UiButton, UiItem, ListView, UiTheme)>();
/// ```
#[cfg(feature = "ui")]
pub use rndr_ui as ui;

/// Embedded terminal. Enable the `term` feature.
#[cfg(feature = "term")]
pub use rndr_term::*;

/// Embedded browser. Enable the `browser` feature.
///
/// The feature is off by default. WebKit is linked only on macOS, and only
/// through that feature. The re-export is the host session, not an application
/// command bus.
#[cfg(feature = "browser")]
pub use rndr_browser::*;

/// Video and audio playback. Enable the `media` feature.
#[cfg(feature = "media")]
pub use rndr_media as media;

#[cfg(test)]
mod tests {
    #[cfg(feature = "ui")]
    #[test]
    fn semantic_namespace_preserves_legacy_types() {
        use std::any::{type_name, TypeId};
        assert_eq!(
            TypeId::of::<crate::Button>(),
            TypeId::of::<rndr_core::Button>()
        );
        assert_eq!(
            TypeId::of::<crate::Theme>(),
            TypeId::of::<rndr_core::Theme>()
        );
        assert_eq!(TypeId::of::<crate::List>(), TypeId::of::<rndr_core::List>());
        assert_eq!(TypeId::of::<crate::Item>(), TypeId::of::<rndr_core::Item>());
        assert_eq!(
            type_name::<crate::ui::UiPlugin>(),
            type_name::<rndr_ui::UiPlugin>()
        );
        assert_ne!(
            TypeId::of::<crate::Button>(),
            TypeId::of::<crate::ui::controls::Button>()
        );
        assert_ne!(
            TypeId::of::<crate::Item>(),
            TypeId::of::<crate::ui::collections::Item>()
        );
    }

    #[test]
    fn reexports_cover_the_desktop_stack() {
        let _ = (
            std::any::type_name::<crate::App>(),
            crate::frame as fn(&mut crate::App) -> crate::Frame,
            crate::run as fn(crate::App),
            std::any::type_name::<crate::StdFsPlugin>(),
            std::any::type_name::<crate::Notifications>(),
        );
        #[cfg(feature = "parley")]
        let _ = std::any::type_name::<crate::ParleyText>();
    }
}
