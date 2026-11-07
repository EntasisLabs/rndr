//! Map a native file-drag report onto core events.
//!
//! winit 0.30.13 delivers `WindowEvent::DroppedFile` and `HoveredFile` as a
//! path. [`winit_path`] therefore passes [`DragPoint::Absent`]. The scale is
//! accepted and does not invent a point.
//!
//! [`from_drag`] attaches a logical point when a drag callback actually
//! supplied one. Client physical pixels are divided by the window scale.
//! An AppKit content point is already in points. A missing or non-finite
//! sample stays [`None`]. This module has no cursor argument.
//!
//! The 0.30.13 callbacks do not carry the sample. Reading it means either
//! moving to the winit 0.31 drag events or replacing those private handlers.
//! Both are outside this crate. See `docs/file-drop.md`.

use std::path::PathBuf;

use rndr_core::file_drop::admitted_position;
use rndr_core::{FileDrop, FileHover, FileHoverCancel, FileHoverSet};

use crate::positive_scale;

/// One path from a native file-drag callback.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NativeFileReport {
    pub(crate) path: PathBuf,
    /// Logical points in the client area, top-left origin, when the callback
    /// included a finite point.
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

/// Where a drag callback's point lives, before it becomes a logical point.
///
/// `Absent` is the winit 0.30.13 case. The other variants are the samples
/// those handlers currently discard. Nothing in the runner constructs them,
/// because this process does not sit inside the callback.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DragPoint {
    /// The callback had no point. winit 0.31 drag events carry a point or
    /// `None`; this variant remains for a report that has no sample.
    #[allow(dead_code)]
    Absent,
    /// Top-left client pixels. Windows client coordinates and translated
    /// X11 window coordinates are this space. winit 0.30.13 does not
    /// construct this; the unit tests do.
    #[allow(dead_code)]
    ClientPhysical([f32; 2]),
    /// Top-left client points. An AppKit content view that is flipped is
    /// already in this space after `convertPoint:fromView:`. winit 0.30.13
    /// does not construct this; the unit tests do.
    #[allow(dead_code)]
    ClientLogical([f32; 2]),
}

/// Path-only report for a winit 0.30.13 file event.
///
/// `scale` is the window's physical pixels per point. It is not applied,
/// because the event has no pixel sample. The cursor is not an argument.
#[allow(dead_code)]
pub(crate) fn winit_path(path: PathBuf, scale: f64) -> NativeFileReport {
    from_drag(path, DragPoint::Absent, scale)
}

/// Build a report from one drag callback's path and point.
///
/// `scale` converts [`DragPoint::ClientPhysical`] into logical points. A
/// non-positive or non-finite scale is read as 1, matching the rest of the
/// runner. [`DragPoint::ClientLogical`] is not divided again.
#[allow(dead_code)]
pub(crate) fn from_drag(path: PathBuf, point: DragPoint, scale: f64) -> NativeFileReport {
    NativeFileReport {
        path,
        position: logical_point(point, scale),
    }
}

/// Logical client point for `point` at `scale`.
pub(crate) fn logical_point(point: DragPoint, scale: f64) -> Option<[f32; 2]> {
    match point {
        DragPoint::Absent => None,
        DragPoint::ClientLogical(logical) => admitted_position(Some(logical)),
        DragPoint::ClientPhysical(physical) => {
            let scale = positive_scale(scale);
            admitted_position(Some([physical[0] / scale, physical[1] / scale]))
        }
    }
}

/// AppKit window-base point to a flipped content view's top-left points.
///
/// `content_origin` is the content view's frame origin in window-base
/// coordinates, and `content_height` is that frame's height. winit's content
/// view returns true from `isFlipped`, so the result matches
/// `convertPoint:fromView:` for a view whose frame sits in an unflipped
/// window. A non-finite input or a non-positive height has no point.
///
/// The runner does not call this until a drag callback supplies the window
/// point. The unit tests cover the frame math.
#[allow(dead_code)]
pub(crate) fn appkit_flipped_content(
    window: [f64; 2],
    content_origin: [f64; 2],
    content_height: f64,
) -> Option<[f64; 2]> {
    if !window[0].is_finite()
        || !window[1].is_finite()
        || !content_origin[0].is_finite()
        || !content_origin[1].is_finite()
        || !content_height.is_finite()
        || content_height <= 0.0
    {
        return None;
    }
    let x = window[0] - content_origin[0];
    let y = content_height - (window[1] - content_origin[1]);
    if !x.is_finite() || !y.is_finite() {
        return None;
    }
    Some([x, y])
}

/// Screen pixels to client pixels. `client_origin` is the client origin in
/// screen pixels, the value `ScreenToClient` subtracts.
///
/// The runner does not call this until a drag callback supplies the screen
/// point. The unit tests cover the subtraction.
#[allow(dead_code)]
pub(crate) fn client_from_screen(screen: [i32; 2], client_origin: [i32; 2]) -> [i32; 2] {
    [
        screen[0].wrapping_sub(client_origin[0]),
        screen[1].wrapping_sub(client_origin[1]),
    ]
}

/// Unpack the root coordinates stored in an `XdndPosition` `data.l[2]`.
///
/// The high 16 bits are x and the low 16 bits are y, both signed. This is
/// the root window, not the client window.
///
/// The runner does not call this until a drag callback supplies the packed
/// word. The unit tests cover the signed unpack.
#[allow(dead_code)]
pub(crate) fn unpack_xdnd_root(packed: i64) -> [i32; 2] {
    let x = ((packed >> 16) as i16) as i32;
    let y = (packed & 0xffff) as i16 as i32;
    [x, y]
}

/// Root-window pixels to window pixels using an origin already translated
/// onto that root. This does not call `XTranslateCoordinates` and does not
/// read the pointer.
///
/// The runner does not call this until a drag callback supplies that origin.
/// The unit tests cover the subtraction.
#[allow(dead_code)]
pub(crate) fn window_from_root(root: [i32; 2], window_origin: [i32; 2]) -> [i32; 2] {
    client_from_screen(root, window_origin)
}

/// One step of a winit 0.31 drag, after the point has been converted.
///
/// `position` is already logical. `None` means that callback had no finite
/// point. Paths are the files from that transfer, one event each.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum DragNotice {
    /// The drag entered this window.
    Entered { id: i64, position: Option<[f32; 2]> },
    /// The drag point moved. This is not a cursor sample from another event.
    Moved { id: i64, position: Option<[f32; 2]> },
    /// The transfer's file list arrived.
    Paths { id: i64, paths: Vec<PathBuf> },
    /// The drag was released over the window.
    Dropped { id: i64 },
    /// The drag left or was cancelled.
    Left { id: i64 },
}

/// The one drag currently over the window.
///
/// A new id replaces the previous drag. Every path in the transfer shares
/// that drag's point. A drop does not clear the hover set; [`DragNotice::Left`]
/// does, through [`NativeFile::Cancel`].
#[derive(Clone, Debug, Default)]
pub(crate) struct DragTracker {
    id: Option<i64>,
    position: Option<[f32; 2]>,
    paths: Vec<PathBuf>,
    drop_waiting: bool,
}

impl DragTracker {
    /// Apply `notice` and return the file events it produces, in order.
    pub(crate) fn apply(&mut self, notice: DragNotice) -> Vec<NativeFile> {
        match notice {
            DragNotice::Entered { id, position } => {
                self.begin(id, position);
                self.hover_events()
            }
            DragNotice::Moved { id, position } => {
                if self.id != Some(id) {
                    self.begin(id, position);
                } else if position.is_some() {
                    self.position = position;
                }
                self.hover_events()
            }
            DragNotice::Paths { id, paths } => {
                if self.id != Some(id) {
                    self.begin(id, None);
                }
                self.paths = paths;
                if self.drop_waiting {
                    self.finish_drop()
                } else {
                    self.hover_events()
                }
            }
            DragNotice::Dropped { id } => {
                if self.id != Some(id) {
                    self.begin(id, None);
                }
                if self.paths.is_empty() {
                    self.drop_waiting = true;
                    Vec::new()
                } else {
                    self.finish_drop()
                }
            }
            DragNotice::Left { id } => {
                if self.id.is_some() && self.id != Some(id) {
                    return Vec::new();
                }
                self.clear();
                vec![NativeFile::Cancel]
            }
        }
    }

    fn begin(&mut self, id: i64, position: Option<[f32; 2]>) {
        if self.id != Some(id) {
            self.paths.clear();
            self.drop_waiting = false;
            self.position = None;
        }
        self.id = Some(id);
        if position.is_some() {
            self.position = position;
        }
    }

    fn hover_events(&self) -> Vec<NativeFile> {
        self.paths
            .iter()
            .cloned()
            .map(|path| {
                NativeFile::Hover(NativeFileReport {
                    path,
                    position: self.position,
                })
            })
            .collect()
    }

    fn finish_drop(&mut self) -> Vec<NativeFile> {
        let position = self.position;
        let events = self
            .paths
            .iter()
            .cloned()
            .map(|path| NativeFile::Drop(NativeFileReport { path, position }))
            .collect();
        self.clear();
        events
    }

    fn clear(&mut self) {
        *self = Self::default();
    }
}

/// A `file:` URI from a drag transfer, as a path on this operating system.
///
/// A non-file URI, an empty URI, and a broken percent-escape are skipped.
/// Windows keeps a UNC authority and a drive letter. Unix keeps the absolute
/// path, including a path that only looks like a drive, and keeps bytes that
/// are not UTF-8. This does not look at the cursor.
pub(crate) fn path_from_file_uri(uri: &str) -> Option<PathBuf> {
    file_uri_path(uri, cfg!(windows))
}

/// `windows` selects the path form. Tests use both values on one host.
pub(crate) fn file_uri_path(uri: &str, windows: bool) -> Option<PathBuf> {
    let bytes = file_uri_bytes(uri, windows)?;
    Some(path_from_bytes(bytes))
}

fn file_uri_bytes(uri: &str, windows: bool) -> Option<Vec<u8>> {
    let uri = uri.trim();
    let rest = uri.strip_prefix("file://")?;
    let (authority, path) = if let Some(path) = rest.strip_prefix('/') {
        ("", path)
    } else {
        let (host, path) = rest.split_once('/')?;
        (host, path)
    };
    if authority.eq_ignore_ascii_case("localhost") {
        return local_file_bytes(path, windows);
    }
    if authority.is_empty() {
        return local_file_bytes(path, windows);
    }
    if !windows {
        return None;
    }
    let mut unc = Vec::from(&b"\\\\"[..]);
    unc.extend_from_slice(authority.as_bytes());
    unc.push(b'\\');
    let decoded = decode_percent_bytes(path)?;
    for byte in decoded {
        unc.push(if byte == b'/' { b'\\' } else { byte });
    }
    Some(unc)
}

fn local_file_bytes(path: &str, windows: bool) -> Option<Vec<u8>> {
    let decoded = decode_percent_bytes(path)?;
    if decoded.is_empty() {
        return None;
    }
    if windows {
        let drive = decoded.len() >= 2
            && decoded[0].is_ascii_alphabetic()
            && decoded[1] == b':'
            && (decoded.len() == 2 || decoded[2] == b'/' || decoded[2] == b'\\');
        if drive {
            return Some(decoded);
        }
    }
    let mut absolute = Vec::with_capacity(decoded.len() + 1);
    absolute.push(b'/');
    absolute.extend_from_slice(&decoded);
    Some(absolute)
}

fn path_from_bytes(bytes: Vec<u8>) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_vec(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(&bytes).as_ref())
    }
}

fn decode_percent_bytes(input: &str) -> Option<Vec<u8>> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len() {
                return None;
            }
            let high = hex(bytes[index + 1])?;
            let low = hex(bytes[index + 2])?;
            out.push((high << 4) | low);
            index += 3;
            continue;
        }
        out.push(bytes[index]);
        index += 1;
    }
    Some(out)
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Apply `file` to the current hover and build the core event.
///
/// Hover admits the path into `set`. Cancel clears every path in `set`.
/// Drop does not change `set`. The position on the event is the position on
/// that report.
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

    fn f32_point(point: [f64; 2]) -> [f32; 2] {
        [point[0] as f32, point[1] as f32]
    }

    fn f64_bits(point: [f64; 2]) -> [u64; 2] {
        [point[0].to_bits(), point[1].to_bits()]
    }

    #[test]
    fn winit_path_events_carry_no_position_at_any_scale() {
        for scale in [1.0, 2.0, 0.0, f64::NAN] {
            let report = winit_path("notes.txt".into(), scale);
            assert!(report.position.is_none(), "{scale}");
        }
        let mut set = FileHoverSet::default();
        let delivered = deliver(
            &mut set,
            NativeFile::Hover(winit_path("notes.txt".into(), 2.0)),
        );
        match delivered {
            DeliveredFile::Hover(hover) => {
                assert_eq!(hover.path.as_os_str(), "notes.txt");
                assert!(hover.position.is_none());
            }
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
    }

    #[test]
    fn client_physical_pixels_divide_by_the_window_scale() {
        let logical = logical_point(DragPoint::ClientPhysical([10.0, 20.0]), 2.0);
        assert_eq!(bits(logical), bits(Some([5.0, 10.0])));
        let origin = logical_point(DragPoint::ClientPhysical([0.0, 0.0]), 2.0);
        assert_eq!(bits(origin), bits(Some([0.0, 0.0])));
        let missing = logical_point(DragPoint::Absent, 2.0);
        assert!(missing.is_none());
        let non_finite = logical_point(DragPoint::ClientPhysical([f32::NAN, 0.0]), 2.0);
        assert!(non_finite.is_none());
        let infinite = logical_point(DragPoint::ClientPhysical([f32::INFINITY, 1.0]), 1.0);
        assert!(infinite.is_none());
        // A non-positive scale is 1, so the pixels stay pixels and the origin
        // stays the origin.
        let unit = logical_point(DragPoint::ClientPhysical([8.0, 4.0]), 0.0);
        assert_eq!(bits(unit), bits(Some([8.0, 4.0])));
    }

    #[test]
    fn each_event_keeps_the_point_from_its_own_callback() {
        let mut set = FileHoverSet::default();
        let hover = from_drag("a.txt".into(), DragPoint::ClientPhysical([20.0, 40.0]), 2.0);
        let drop = from_drag("b.txt".into(), DragPoint::ClientPhysical([0.0, 0.0]), 2.0);
        let bare = from_drag("c.txt".into(), DragPoint::Absent, 2.0);
        match deliver(&mut set, NativeFile::Hover(hover)) {
            DeliveredFile::Hover(event) => {
                assert_eq!(event.path.as_os_str(), "a.txt");
                assert_eq!(bits(event.position), bits(Some([10.0, 20.0])));
            }
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
        match deliver(&mut set, NativeFile::Drop(drop)) {
            DeliveredFile::Drop(event) => {
                assert_eq!(event.path.as_os_str(), "b.txt");
                assert_eq!(bits(event.position), bits(Some([0.0, 0.0])));
            }
            DeliveredFile::Hover(_) | DeliveredFile::Cancel(_) => panic!("drop"),
        }
        match deliver(&mut set, NativeFile::Hover(bare)) {
            DeliveredFile::Hover(event) => {
                assert_eq!(event.path.as_os_str(), "c.txt");
                assert!(event.position.is_none());
            }
            DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
        }
        assert_eq!(set.len(), 2);
        assert_eq!(set.as_slice()[0].path.as_os_str(), "a.txt");
        assert_eq!(bits(set.as_slice()[0].position), bits(Some([10.0, 20.0])));
        assert!(set.as_slice()[1].position.is_none());
        deliver(&mut set, NativeFile::Cancel);
        assert!(set.is_empty());
    }

    #[test]
    fn appkit_flipped_content_is_logical_and_is_not_scaled_again() {
        let top_left = appkit_flipped_content([0.0, 200.0], [0.0, 0.0], 200.0).unwrap();
        assert_eq!(f64_bits(top_left), f64_bits([0.0, 0.0]));
        let bottom_left = appkit_flipped_content([0.0, 0.0], [0.0, 0.0], 200.0).unwrap();
        assert_eq!(f64_bits(bottom_left), f64_bits([0.0, 200.0]));
        let inside = appkit_flipped_content([16.0, 180.0], [0.0, 40.0], 200.0).unwrap();
        // Window y 180 is 140 above the content origin, so flipped y is 60.
        assert_eq!(f64_bits(inside), f64_bits([16.0, 60.0]));
        assert!(appkit_flipped_content([0.0, 0.0], [0.0, 0.0], 0.0).is_none());
        assert!(appkit_flipped_content([f64::NAN, 0.0], [0.0, 0.0], 10.0).is_none());

        let report = from_drag(
            "note.txt".into(),
            DragPoint::ClientLogical(f32_point(inside)),
            2.0,
        );
        assert_eq!(bits(report.position), bits(Some([16.0, 60.0])));
    }

    #[test]
    fn win32_screen_point_and_xdnd_root_point_share_the_client_scale() {
        let client = client_from_screen([110, 80], [10, 20]);
        assert_eq!(client, [100, 60]);
        let report = from_drag(
            "from-windows.txt".into(),
            DragPoint::ClientPhysical([client[0] as f32, client[1] as f32]),
            2.0,
        );
        assert_eq!(bits(report.position), bits(Some([50.0, 30.0])));

        let packed = (100i64 << 16) | 40;
        assert_eq!(unpack_xdnd_root(packed), [100, 40]);
        let negative = ((u16::MAX as i64) << 16) | (u16::MAX as i64 - 1);
        assert_eq!(unpack_xdnd_root(negative), [-1, -2]);
        let window = window_from_root(unpack_xdnd_root(negative), [10, 30]);
        assert_eq!(window, [-11, -32]);
        let report = from_drag(
            "from-x11.txt".into(),
            DragPoint::ClientPhysical([window[0] as f32, window[1] as f32]),
            2.0,
        );
        assert_eq!(bits(report.position), bits(Some([-5.5, -16.0])));
    }

    #[test]
    fn a_drag_correlates_one_point_with_every_path_and_cancel_clears_them() {
        let mut tracker = DragTracker::default();
        let mut set = FileHoverSet::default();
        let entered = tracker.apply(DragNotice::Entered {
            id: 7,
            position: logical_point(DragPoint::ClientPhysical([0.0, 0.0]), 2.0),
        });
        assert!(entered.is_empty());
        let hovers = tracker.apply(DragNotice::Paths {
            id: 7,
            paths: vec!["a.txt".into(), "b.txt".into()],
        });
        assert_eq!(hovers.len(), 2);
        for event in hovers {
            match deliver(&mut set, event) {
                DeliveredFile::Hover(hover) => {
                    assert_eq!(bits(hover.position), bits(Some([0.0, 0.0])));
                }
                DeliveredFile::Drop(_) | DeliveredFile::Cancel(_) => panic!("hover"),
            }
        }
        let moved = tracker.apply(DragNotice::Moved {
            id: 7,
            position: logical_point(DragPoint::ClientPhysical([20.0, 40.0]), 2.0),
        });
        assert_eq!(moved.len(), 2);
        match &moved[0] {
            NativeFile::Hover(report) => {
                assert_eq!(report.path.as_os_str(), "a.txt");
                assert_eq!(bits(report.position), bits(Some([10.0, 20.0])));
            }
            NativeFile::Drop(_) | NativeFile::Cancel => panic!("hover"),
        }
        match &moved[1] {
            NativeFile::Hover(report) => {
                assert_eq!(report.path.as_os_str(), "b.txt");
                assert_eq!(bits(report.position), bits(Some([10.0, 20.0])));
            }
            NativeFile::Drop(_) | NativeFile::Cancel => panic!("hover"),
        }
        let missing = tracker.apply(DragNotice::Moved {
            id: 7,
            position: logical_point(DragPoint::ClientPhysical([f32::NAN, 1.0]), 2.0),
        });
        match &missing[0] {
            NativeFile::Hover(report) => {
                assert_eq!(bits(report.position), bits(Some([10.0, 20.0])));
            }
            NativeFile::Drop(_) | NativeFile::Cancel => panic!("hover"),
        }
        let drops = tracker.apply(DragNotice::Dropped { id: 7 });
        assert_eq!(drops.len(), 2);
        match &drops[1] {
            NativeFile::Drop(report) => {
                assert_eq!(report.path.as_os_str(), "b.txt");
                assert_eq!(bits(report.position), bits(Some([10.0, 20.0])));
            }
            NativeFile::Hover(_) | NativeFile::Cancel => panic!("drop"),
        }
        assert_eq!(set.len(), 2);
        for event in tracker.apply(DragNotice::Left { id: 7 }) {
            deliver(&mut set, event);
        }
        assert!(set.is_empty());
    }

    #[test]
    fn file_uris_become_paths_and_other_uris_are_skipped() {
        assert_eq!(
            path_from_file_uri("file:///tmp/a%20b.txt").unwrap(),
            PathBuf::from("/tmp/a b.txt")
        );
        assert_eq!(
            path_from_file_uri("file://localhost/tmp/a.txt").unwrap(),
            PathBuf::from("/tmp/a.txt")
        );
        assert!(path_from_file_uri("https://example.com/a").is_none());
        assert!(path_from_file_uri("file:///bad%").is_none());
        assert!(path_from_file_uri("").is_none());
    }

    #[test]
    fn file_uris_keep_windows_unc_and_unix_absolute_paths() {
        let unc = file_uri_path("file://server/share/dir/file.txt", true).unwrap();
        assert_eq!(unc, PathBuf::from(r"\\server\share\dir\file.txt"));
        let drive = file_uri_path("file:///C:/notes.txt", true).unwrap();
        assert_eq!(drive, PathBuf::from("C:/notes.txt"));
        let unix_drive = file_uri_path("file:///C:/notes.txt", false).unwrap();
        assert_eq!(unix_drive, PathBuf::from("/C:/notes.txt"));
        assert!(file_uri_path("file://server/share/dir/file.txt", false).is_none());
        let raw = file_uri_bytes("file:///tmp/a%FFb.txt", false).unwrap();
        assert_eq!(raw, b"/tmp/a\xffb.txt");
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let path = path_from_file_uri("file:///tmp/a%FFb.txt").unwrap();
            assert_eq!(path.as_os_str().as_bytes(), b"/tmp/a\xffb.txt");
        }
    }
}
