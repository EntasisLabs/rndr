//! Files dragged over a window.
//!
//! [`FileHover`] and [`FileDrop`] carry the path the platform reported and
//! `position: Option<[f32; 2]>` in logical points. [`None`] means that report
//! did not include a finite point. The origin is stored only when the report
//! included the origin. A non-finite component is dropped at
//! [`FileHover::from_report`] and [`FileDrop::from_report`].
//!
//! The desktop runner forwards winit 0.30.13 `HoveredFile` and `DroppedFile`.
//! Those variants are a [`std::path::PathBuf`] with no coordinates, so the
//! forwarded events use [`None`]. The runner does not read the cursor to fill
//! the point. Wayland is [`PathDelivery::Unsupported`]: winit does not emit
//! the events, and this crate does not add a Wayland drop path.
//!
//! [`FileHoverCancel`] is one event for the whole hover. [`FileHoverSet::cancel`]
//! drops every path currently in the set. Hit-testing a point into a row stays
//! with the host. This module does not map a point onto an entity.
//!
//! # Compatibility
//!
//! A struct literal now names `position`. `FileHover { path }` and
//! `FileDrop { path }` do not compile. Pass [`None`] for a path-only event, or
//! call [`FileHover::from_report`] / [`FileDrop::from_report`]. The facade
//! re-exports the same types. Neither struct implements [`Eq`]: a logical point
//! is `f32`, and [`PartialEq`] remains. [`FileHoverCancel`] is unchanged.
//!
//! No feature flag and no extra crate read a position. With the current
//! binding, default and `--no-default-features` desktop builds both forward
//! path-only events.
//!
//! [`platform_support`] describes that binding. [`native_session`] is the
//! certification ledger. Headless tests do not move a native cell to
//! [`NativeSession::Supported`]. The repository note `docs/file-drop.md`
//! records the same split.

use std::path::PathBuf;

use crate::event::Event;
use crate::Platform;

/// A point from a platform file report, in logical points.
///
/// [`None`] stays [`None`]. A component that is NaN or infinite is not a
/// point, so the result is [`None`]. A finite origin stays `[0.0, 0.0]`.
pub const fn admitted_position(position: Option<[f32; 2]>) -> Option<[f32; 2]> {
    match position {
        Some([x, y]) if x.is_finite() && y.is_finite() => Some([x, y]),
        _ => None,
    }
}

/// A path is over the window and has not been dropped.
///
/// Construct it with [`Self::from_report`] so a non-finite point becomes
/// [`None`]. A public-field literal can still store a non-finite point;
/// [`FileHoverSet::hover`] admits the event again.
#[derive(Clone, Debug, PartialEq)]
pub struct FileHover {
    /// Path the platform reported. It may not exist yet.
    pub path: PathBuf,
    /// Logical point attached to this path, when the report included one.
    pub position: Option<[f32; 2]>,
}

impl FileHover {
    /// Hover from a platform report.
    ///
    /// `position` is kept when both components are finite. Otherwise the
    /// stored position is [`None`].
    pub fn from_report(path: impl Into<PathBuf>, position: Option<[f32; 2]>) -> Self {
        Self {
            path: path.into(),
            position: admitted_position(position),
        }
    }
}

impl Event for FileHover {}

/// A path was released over the window.
///
/// Construction follows [`FileHover::from_report`].
#[derive(Clone, Debug, PartialEq)]
pub struct FileDrop {
    /// Path the platform reported.
    pub path: PathBuf,
    /// Logical point attached to this path, when the report included one.
    pub position: Option<[f32; 2]>,
}

impl FileDrop {
    /// Drop from a platform report.
    ///
    /// `position` is kept when both components are finite. Otherwise the
    /// stored position is [`None`].
    pub fn from_report(path: impl Into<PathBuf>, position: Option<[f32; 2]>) -> Self {
        Self {
            path: path.into(),
            position: admitted_position(position),
        }
    }
}

impl Event for FileDrop {}

/// The pointer left the window while files were hovered.
///
/// One event covers every path in the current [`FileHoverSet`]. The event
/// itself has no path list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileHoverCancel;

impl Event for FileHoverCancel {}

/// Paths currently hovered, in first-seen order.
///
/// The desktop runner owns the resource on each window and replaces it when
/// it applies file events. A later frame writes the runner's set back, so a
/// system write does not stick. A headless host can apply the same rules with
/// [`Self::hover`] and [`Self::cancel`].
///
/// A repeated path updates that entry's position and keeps its place. Cancel
/// clears every entry. A drop is not a hover and is not applied here.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FileHoverSet {
    entries: Vec<FileHover>,
}

impl FileHoverSet {
    /// Admit `event` into the current hover.
    ///
    /// The stored position passes through [`admitted_position`] again.
    pub fn hover(&mut self, event: FileHover) {
        let event = FileHover::from_report(event.path, event.position);
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.path == event.path)
        {
            existing.position = event.position;
            return;
        }
        self.entries.push(event);
    }

    /// Remove every hovered path.
    pub fn cancel(&mut self) {
        self.entries.clear();
    }

    /// Hovered paths, in first-seen order.
    pub fn as_slice(&self) -> &[FileHover] {
        &self.entries
    }

    /// Number of hovered paths.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// `true` when no path is hovered.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// How the current windowing binding treats file drag on one platform.
///
/// This is the code path. It is not a live-session result. [`native_session`]
/// is the certification ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileDropSupport {
    /// Path hover, path drop, and a single hover-cancel.
    pub paths: PathDelivery,
    /// Logical point on hover and drop.
    pub position: PositionDelivery,
}

/// Whether file-drag paths are forwarded for one platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathDelivery {
    /// The runner forwards a path when winit emits `HoveredFile`, `DroppedFile`,
    /// or `HoveredFileCancelled`.
    Forwarded,
    /// The binding does not emit those events, and this crate does not add a
    /// replacement.
    Unsupported,
}

/// Whether a forwarded file event includes a logical point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PositionDelivery {
    /// A named session observed a finite logical point on the event.
    /// [`platform_support`] does not return this.
    Supported,
    /// The binding's event has no point. The runner stores [`None`].
    Unavailable,
    /// The runner converts a client point from the drag event when that
    /// event includes a finite one. This is not a live-session result.
    Mapped,
    /// No file-drag event is emitted, so there is no point to attach.
    Unsupported,
}

/// Native certification for file drag on one platform.
///
/// Headless tests leave this where [`native_session`] puts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSession {
    /// A named live session showed the drag the ledger describes.
    /// [`native_session`] does not return this.
    Supported,
    /// No named session is recorded for this platform.
    Unverified,
    /// This unit does not implement file drag on this platform.
    Unsupported,
}

/// Binding behavior for file drag.
///
/// macOS, Windows, and X11 forward winit's path events. Those events carry
/// no point, so [`FileDropSupport::position`] is [`PositionDelivery::Mapped`]
/// only as a code path: a finite client point is converted, and a missing
/// point stays [`None`]. [`native_session`] stays [`NativeSession::Unverified`].
/// Wayland is [`PathDelivery::Unsupported`] and
/// [`PositionDelivery::Unsupported`].
pub fn platform_support(platform: Platform) -> FileDropSupport {
    match platform {
        Platform::Mac | Platform::Windows | Platform::X11 => FileDropSupport {
            paths: PathDelivery::Forwarded,
            // Client pixels from the drag event are converted when present.
            // A live drag has not been run, so this is not Supported.
            position: PositionDelivery::Mapped,
        },
        Platform::Wayland => FileDropSupport {
            paths: PathDelivery::Unsupported,
            position: PositionDelivery::Unsupported,
        },
    }
}

/// Certification ledger for file drag.
///
/// macOS, Windows, and X11 are [`NativeSession::Unverified`]. Wayland is
/// [`NativeSession::Unsupported`]. A passing headless test does not change
/// either value.
pub fn native_session(platform: Platform) -> NativeSession {
    match platform {
        Platform::Wayland => NativeSession::Unsupported,
        Platform::Mac | Platform::Windows | Platform::X11 => NativeSession::Unverified,
    }
}
