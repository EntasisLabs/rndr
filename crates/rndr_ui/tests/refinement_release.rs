use rndr_core::*;
use rndr_ui::{controls::*, *};

#[test]
fn fitting_release_handles_pending_resize_and_preserves_marker_and_takeover() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(2000., 2000.));
    let root = a.world_mut().spawn((Layout::column(),));
    let mode = ModeSelector::mount(
        a.world_mut(),
        root,
        UiId(1),
        "View",
        &["Preview", "Source"],
        TabOrientation::Horizontal,
        0,
    )
    .unwrap();
    a.world_mut().get_mut::<Layout>(mode.group).unwrap().width = Size::Px(717.);
    a.world_mut().get_mut::<Layout>(mode.group).unwrap().gap = 11.;
    a.world_mut().insert(mode.tabs[0].root, ToolbarContentWidth);
    // Fit then cancel before the first layout must not disturb authored fields.
    mode.fit(a.world_mut(), 600.).unwrap();
    mode.clear_fit(a.world_mut()).unwrap();
    assert_eq!(
        a.world().get::<Layout>(mode.group).unwrap().width,
        Size::Px(717.)
    );
    mode.fit(a.world_mut(), 600.).unwrap();
    a.update();
    // A pending new available width has not been applied yet. Clear must use
    // the last ACTUAL width, not the latest request, when releasing ownership.
    mode.fit(a.world_mut(), 700.).unwrap();
    mode.clear_fit(a.world_mut()).unwrap();
    a.update();
    assert_eq!(
        a.world().get::<Layout>(mode.group).unwrap().width,
        Size::Px(717.)
    );
    assert_eq!(a.world().get::<Layout>(mode.group).unwrap().gap, 11.);
    assert!(a.world().has::<ToolbarContentWidth>(mode.tabs[0].root));
    assert!(!a.world().has::<ToolbarContentWidth>(mode.tabs[1].root));
    mode.fit(a.world_mut(), 600.).unwrap();
    a.update();
    a.world_mut().get_mut::<Layout>(mode.group).unwrap().width = Size::Px(809.);
    a.world_mut().get_mut::<Layout>(mode.group).unwrap().gap = 9.;
    a.update();
    mode.clear_fit(a.world_mut()).unwrap();
    a.update();
    assert_eq!(
        a.world().get::<Layout>(mode.group).unwrap().width,
        Size::Px(809.)
    );
    assert_eq!(a.world().get::<Layout>(mode.group).unwrap().gap, 9.);
}

#[test]
fn content_metrics_do_not_shrink_editor_or_field_geometry() {
    let mut w = World::new();
    let field = w.spawn((
        Label::new("legacy"),
        Field {
            kind: EditField::Body,
        },
        LabelBox::CONTENT,
    ));
    assert_eq!(label_box(&w, field), LabelBox::default());
}
