//! Map a native file-drag report onto core events.
//!
//! winit 0.30.13 delivers `WindowEvent::DroppedFile` and `HoveredFile` as a
//! path. [`winit_path`] stores no position. A caller that actually has a
//! finite logical point sets [`NativeFileReport::position`] and [`deliver`]
//! keeps that point. This module has no cursor argument.

use std::path::PathBuf;

use rndr_core::{FileDrop, FileHover, FileHoverCancel, FileHoverSet};

/// One path from a native file-drag callback.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeFileReport {
    pub(crate) path: PathBuf,
    /// Logical points, already in window space, when the callback included a
    /// point. Absence stays [`None`].
    pub(crate) position: Option<[f32; 2]>,
}

/// File-drag report ready for one frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NativeFile {
    Hover(NativeFileReport),
    Drop(NativeFileReport),
    Cancel,
}

/// Core event produced from a [`NativeFile`].
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DeliveredFile {
    Hover(FileHover),
    Drop(FileDrop),
    Cancel(FileHoverCancel),
}

/// Path-only report for a winit 0.30.13 file event.
///
/// The event enum carries a [`PathBuf`] and nothing else. The position is
/// [`None`]. The window cursor is not an input to this function.
pub(crate) fn winit_path(path: PathBuf) -> NativeFileReport {
    NativeFileReport {
        path,
        position: None,
    }
}

/// Apply `file` to the current hover and build the core event.
///
/// Hover admits the path into `set`. Cancel clears every path in `set`.
/// Drop does not change `set`.
pub(crate) fn deliver(set: &mut FileHoverSet, file: NativeFile) -> DeliveredFile {
    match file {
        NativeFile::Hover(report) => {
            let event = FileHover::from_report(report.path, report.position);
            set.hover(event.clone());
            DeliveredFile::Hover(event)
        }
        NativeFile::Drop(report) => {
            DeliveredFile::Drop(FileDrop::from_report(report.path, report.position))
        }
        NativeFile::Cancel => {
            set.cancel();
            DeliveredFile::Cancel(FileHoverCancel)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(position: Option<[f32; 2]>) -> Option<[u32; 2]> {
        position.map(|[x, y]| [x.to_bits(), y.to_bits()])
    }

    #[test]
    fn winit_path_events_carry_no_position() {
        let report = winit_path("notes.txt".into());
        assert!(report.position.is_none());
        let mut set = FileHoverSet::default();
        let delivered = deliver(&mut set, NativeFile::Hover(report));
        match delivered {
            DeliveredFile::Hover(hover) => {
                assert_eq!(hover.path.as_os_str(), "notes.txt");
                assert!(hover.position.is_none());
            }
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn a_supplied_logical_point_is_delivered_unchanged() {
        let mut set = FileHoverSet::default();
        let delivered = deliver(
            &mut set,
            NativeFile::Drop(NativeFileReport {
                path: "shot.png".into(),
                position: Some([0.0, 0.0]),
            }),
        );
        match delivered {
            DeliveredFile::Drop(drop) => {
                assert_eq!(bits(drop.position), bits(Some([0.0, 0.0])));
            }
            DeliveredFile::Hover(_) | DeliveredFile::Cancel(_) => panic!("drop"),
        }
        let delivered = deliver(
            &mut set,
            NativeFile::Hover(NativeFileReport {
                path: "shot.png".into(),
                position: Some([10.5, -2.0]),
            }),
        );
        match delivered {
            DeliveredFile::Hover(hover) => {
                assert_eq!(bits(hover.position), bits(Some([10.5, -2.0])));
            }
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
    }

    #[test]
    fn a_non_finite_point_stays_absent() {
        let mut set = FileHoverSet::default();
        let delivered = deliver(
            &mut set,
            NativeFile::Hover(NativeFileReport {
                path: "bad.txt".into(),
                position: Some([f32::NAN, 0.0]),
            }),
        );
        match delivered {
            DeliveredFile::Hover(hover) => assert!(hover.position.is_none()),
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
    }

    #[test]
    fn hover_cancel_clears_every_path_and_a_drop_does_not() {
        let mut set = FileHoverSet::default();
        deliver(
            &mut set,
            NativeFile::Hover(NativeFileReport {
                path: "a".into(),
                position: Some([1.0, 2.0]),
            }),
        );
        deliver(&mut set, NativeFile::Hover(winit_path("b".into())));
        assert_eq!(set.len(), 2);
        deliver(
            &mut set,
            NativeFile::Drop(NativeFileReport {
                path: "a".into(),
                position: Some([3.0, 4.0]),
            }),
        );
        assert_eq!(set.len(), 2);
        deliver(&mut set, NativeFile::Cancel);
        assert!(set.is_empty());
        deliver(&mut set, NativeFile::Cancel);
        assert!(set.is_empty());
    }
}
