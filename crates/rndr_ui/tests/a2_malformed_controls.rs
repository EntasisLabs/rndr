use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};
fn app() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(320.0),
        ..Layout::column()
    },));
    (a, root)
}
#[test]
fn removed_root_group_or_panel_layout_is_stale_not_a_partial_update() {
    let (mut a, root) = app();
    let h = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("old", HostIntent::Command(1)),
    )
    .unwrap();
    a.world_mut().remove::<Layout>(h.root);
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
    assert_eq!(
        controls::Button::mount(
            a.world_mut(),
            root,
            UiId(1),
            ButtonProps::new("new", HostIntent::Command(2))
        ),
        Err(UiError::Stale)
    );
    assert_eq!(h.destroy(a.world_mut()), Err(UiError::Stale));
    a.update();
    for part in 0..3 {
        let (mut a, root) = app();
        let h = ModeSelector::mount(
            a.world_mut(),
            root,
            UiId(1),
            "Mode",
            &["Source", "Preview"],
            TabOrientation::Horizontal,
            0,
        )
        .unwrap();
        let target = [h.root, h.group, h.panels[0]][part];
        a.world_mut().remove::<Layout>(target);
        assert_eq!(h.validate(a.world()), Err(UiError::Stale));
        assert_eq!(
            ModeSelector::mount(
                a.world_mut(),
                root,
                UiId(1),
                "Changed",
                &["Other", "Names"],
                TabOrientation::Vertical,
                0
            ),
            Err(UiError::Stale)
        );
        assert_eq!(h.destroy(a.world_mut()), Err(UiError::Stale));
        a.update();
    }
}
#[test]
fn toolbar_measurement_tolerates_malformed_borrowed_control_and_checks_own_layout() {
    let (mut a, root) = app();
    let t = Toolbar::mount(a.world_mut(), root, UiId(1), 320.0).unwrap();
    let h = controls::Button::mount(
        a.world_mut(),
        t.root,
        UiId(1),
        ButtonProps::new("Action", HostIntent::Command(1)),
    )
    .unwrap();
    a.update();
    a.world_mut().remove::<Label>(h.label);
    a.update();
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
    assert_eq!(a.world().get::<Layout>(t.root).unwrap().axis, Axis::Column);
    a.world_mut().remove::<Layout>(t.root);
    assert_eq!(
        Toolbar::mount(a.world_mut(), root, UiId(1), 160.0),
        Err(UiError::Stale)
    );
    assert_eq!(t.destroy(a.world_mut()), Err(UiError::Stale));
}
