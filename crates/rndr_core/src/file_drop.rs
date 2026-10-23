//! Files dragged over a window.
//!
//! winit 0.30 emits these on macOS, Windows, and X11. Wayland does not. The
//! event carries a path and no position. One drop of several files is one
//! event per path. A hover that leaves the window is a single
//! [`FileHoverCancel`], even when more than one path was hovered.

use std::path::PathBuf;

use crate::event::Event;

/// A path is over the window and has not been dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHover {
    /// Path the platform reported. It may not exist yet.
    pub path: PathBuf,
}

impl Event for FileHover {}

/// A path was released over the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileDrop {
    /// Path the platform reported.
    pub path: PathBuf,
}

impl Event for FileDrop {}

/// The pointer left the window while files were hovered.
///
/// One event covers every path from the current hover.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileHoverCancel;

impl Event for FileHoverCancel {}
