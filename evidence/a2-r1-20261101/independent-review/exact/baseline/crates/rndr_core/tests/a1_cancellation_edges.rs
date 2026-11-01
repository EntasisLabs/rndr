use rndr_core::*;
fn scene() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    let button = app.world_mut().spawn((
        Layout {
            width: Size::Px(160.0),
            height: Size::Px(50.0),
            ..Layout::column()
        },
        Button::new("action"),
        ReleaseActivation,
    ));
    app.update();
    (app, button)
}
#[test]
fn matching_pointer_is_not_enough_a_release_must_match_primary_button() {
    let (mut app, button) = scene();
    let pointer = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    pointer.position = [5.0, 5.0];
    pointer.pressed = true;
    pointer.down = true;
    app.update();
    let pointer = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    pointer.pressed = false;
    pointer.released = true;
    pointer.button = PointerButton::Right;
    app.update();
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(button)
    );
}
#[test]
fn scroll_capture_wins_over_an_armed_button() {
    let (mut app, button) = scene();
    let pointer = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    pointer.position = [5.0, 5.0];
    pointer.pressed = true;
    pointer.down = true;
    app.update();
    app.world_mut().insert_resource(ScrollDrag {
        entity: Some(button),
        active: true,
        ..ScrollDrag::default()
    });
    let pointer = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    pointer.pressed = false;
    pointer.down = false;
    pointer.released = true;
    app.update();
    assert!(take_action_intents(app.world_mut()).is_empty());
}
#[test]
fn semantic_tab_requires_live_panel_and_relations_survive_no_stale_generation() {
    let (mut app, _) = scene();
    let group = app.world_mut().spawn((Tabs { selected: 1 }, Layout::row()));
    let tab = app.world_mut().spawn((Tab { id: 1 }, Layout::column()));
    set_parent(app.world_mut(), tab, group);
    let panel = app.world_mut().spawn((TabPanel, Layout::column()));
    link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
    assert!(access_details(app.world(), tab).activate);
    despawn(app.world_mut(), panel);
    assert!(!access_details(app.world(), tab).activate);
    assert_eq!(
        try_activate(app.world_mut(), tab),
        ActivationResult::Rejected
    );
}
