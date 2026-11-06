//! Position admission, hover cancellation, and the file-drag matrix.
//!
//! These tests drive the headless types. They do not open a window and they
//! do not certify macOS, Windows, X11, or Wayland.

use std::path::PathBuf;

use rndr_core::file_drop::{NativeSession, PathDelivery, PositionDelivery};
use rndr_core::{
    events, file_drop, send, App, FileDrop, FileHover, FileHoverCancel, FileHoverSet, Platform,
};

fn bits(position: Option<[f32; 2]>) -> Option<[u32; 2]> {
    position.map(|[x, y]| [x.to_bits(), y.to_bits()])
}

#[test]
fn admission_keeps_a_finite_point_and_leaves_every_other_report_empty() {
    let cases = [
        (None, None),
        (Some([0.0, 0.0]), Some([0.0, 0.0])),
        (Some([12.5, -3.0]), Some([12.5, -3.0])),
        (Some([f32::NAN, 1.0]), None),
        (Some([1.0, f32::NAN]), None),
        (Some([f32::INFINITY, 0.0]), None),
        (Some([0.0, f32::NEG_INFINITY]), None),
    ];
    for (input, expected) in cases {
        let hover = FileHover::from_report("item", input);
        let drop = FileDrop::from_report("item", input);
        assert_eq!(bits(hover.position), bits(expected));
        assert_eq!(bits(drop.position), bits(expected));
        assert_eq!(hover.path, PathBuf::from("item"));
        assert_eq!(drop.path, PathBuf::from("item"));
    }
}

#[test]
fn the_hover_set_rejects_a_non_finite_literal_and_updates_a_repeated_path() {
    let mut set = FileHoverSet::default();
    set.hover(FileHover {
        path: "c".into(),
        position: Some([f32::NAN, 0.0]),
    });
    assert_eq!(set.len(), 1);
    assert!(set.as_slice()[0].position.is_none());
    set.hover(FileHover::from_report("a", Some([4.0, 5.0])));
    set.hover(FileHover::from_report("b", None));
    set.hover(FileHover::from_report("a", Some([1.0, 2.0])));
    assert_eq!(set.len(), 3);
    assert_eq!(set.as_slice()[1].path, PathBuf::from("a"));
    assert_eq!(bits(set.as_slice()[1].position), bits(Some([1.0, 2.0])));
    assert!(set.as_slice()[2].position.is_none());
}

#[test]
fn cancel_clears_every_hovered_path() {
    let mut set = FileHoverSet::default();
    set.hover(FileHover::from_report("a", Some([4.0, 9.0])));
    set.hover(FileHover::from_report("b", None));
    assert_eq!(set.len(), 2);
    set.cancel();
    assert!(set.is_empty());
    set.cancel();
    assert!(set.is_empty());
}

#[test]
fn queued_events_keep_the_admitted_position() {
    let mut app = App::new();
    send(
        app.world_mut(),
        FileHover::from_report("a", Some([8.0, 9.0])),
    );
    send(app.world_mut(), FileDrop::from_report("a", None));
    send(app.world_mut(), FileHoverCancel);
    app.update();
    let hovers = events::<FileHover>(app.world());
    assert_eq!(hovers.len(), 1);
    assert_eq!(hovers[0].path, PathBuf::from("a"));
    assert_eq!(bits(hovers[0].position), bits(Some([8.0, 9.0])));
    let drops = events::<FileDrop>(app.world());
    assert_eq!(drops.len(), 1);
    assert!(drops[0].position.is_none());
    assert_eq!(events::<FileHoverCancel>(app.world()).len(), 1);
}

#[test]
fn the_binding_matrix_is_separate_from_the_native_ledger() {
    for platform in [Platform::Mac, Platform::Windows, Platform::X11] {
        let support = file_drop::platform_support(platform);
        assert_eq!(support.paths, PathDelivery::Forwarded);
        assert_eq!(support.position, PositionDelivery::Unavailable);
        assert_eq!(
            file_drop::native_session(platform),
            NativeSession::Unverified
        );
    }
    let wayland = file_drop::platform_support(Platform::Wayland);
    assert_eq!(wayland.paths, PathDelivery::Unsupported);
    assert_eq!(wayland.position, PositionDelivery::Unsupported);
    assert_eq!(
        file_drop::native_session(Platform::Wayland),
        NativeSession::Unsupported
    );
    for platform in [
        Platform::Mac,
        Platform::Windows,
        Platform::X11,
        Platform::Wayland,
    ] {
        assert_ne!(
            file_drop::platform_support(platform).position,
            PositionDelivery::Supported
        );
        assert_ne!(
            file_drop::native_session(platform),
            NativeSession::Supported
        );
    }
}
