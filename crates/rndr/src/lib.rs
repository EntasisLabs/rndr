//! Desktop application toolkit.
//!
//! This crate re-exports [`rndr_core`], [`rndr_paint`], [`rndr_std`], and
//! [`rndr_render`] so `cargo add rndr` builds a window. Headless programs can
//! depend on `rndr_core` alone and skip the window and the GPU.
//!
//! # How the crates fit
//!
//! - [`rndr_core`] owns the world: entities, hierarchy, layout, input, text,
//!   events, and tasks. It has no GPU types.
//! - [`rndr_paint`] turns one frame of that world into [`Quad`]s in logical
//!   points. It does not open a window.
//! - [`rndr_std`] inserts an [`Fs`] backend that reads and writes the process
//!   filesystem.
//! - [`rndr_render`] opens the window, writes [`Input`] and [`Viewport`], and
//!   presents those quads.
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
//! The feature flags match `rndr_render` and are on by default:
//!
//! - `hotkey` grabs system-wide chords on macOS, Windows, X11, and the Wayland
//!   global shortcuts portal.
//! - `tray` adds a menu-bar or notification-area icon.
//! - `parley` shapes and rasterizes text, and links fontconfig on Linux.
//! - `term` re-exports the embedded terminal. It is off by default, so a
//!   normal desktop build does not link a pseudoterminal.
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

/// Embedded terminal. Enable the `term` feature.
#[cfg(feature = "term")]
pub use rndr_term::*;

#[cfg(test)]
mod tests {
    #[test]
    fn reexports_cover_the_desktop_stack() {
        let _ = (
            std::any::type_name::<crate::App>(),
            crate::frame as fn(&mut crate::App) -> crate::Frame,
            crate::run as fn(crate::App),
            std::any::type_name::<crate::StdFsPlugin>(),
        );
        #[cfg(feature = "parley")]
        let _ = std::any::type_name::<crate::ParleyText>();
    }
}
