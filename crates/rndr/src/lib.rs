//! Desktop application toolkit.
//!
//! This crate re-exports [`rndr_core`], [`rndr_paint`], [`rndr_std`], and
//! [`rndr_render`] so `cargo add rndr` builds a window. Headless programs can
//! depend on `rndr_core` alone and skip winit and wgpu.
//!
//! # Features
//!
//! The feature flags match `rndr_render` and are on by default:
//!
//! - `hotkey` grabs system-wide chords on macOS, Windows, X11, and the Wayland
//!   global shortcuts portal.
//! - `tray` adds a menu-bar or notification-area icon.
//! - `parley` shapes and rasterizes text, and links fontconfig on Linux.
//!
//! `rndr::register` and `rndr::toggle_window` install the operating-system
//! hotkey backend. The parser and the registry stay at `rndr::hotkey`.
//!
//! docs.rs builds this crate with every feature. That Linux image provides
//! fontconfig, libxkbcommon, Wayland, and Vulkan. `hotkey` and `tray` do not
//! link libdbus.

#![deny(ambiguous_glob_reexports)]

pub use rndr_core::*;
pub use rndr_paint::*;
pub use rndr_render::*;
pub use rndr_std::*;

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
