use rndr_core::*;
use rndr_ui::collections::Item;
use rndr_ui::{collections::*, controls, *};
fn setup() -> (App, Entity, ViewHandle, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_system_front(Schedule::Input, apply_scroll);
    app.world_mut().insert_resource(Viewport::new(500.0, 500.0));
    let root = app.world_mut().spawn((Layout { width: Size::Fill, height: Size::Fill, ..Layout::column() },));
    let nodes = [TreeNode { item: Item::new(1, "Branch"), parent: None }, TreeNode { item: Item::new(2, "Leaf"), parent: Some(ItemId(1)) }];
    let h = TreeView::mount(app.world_mut(), root, UiId(20), ViewProps::new("Tree"), &nodes, |_, _, _| {}).unwrap();
    let after = controls::Button::mount(app.world_mut(), root, UiId(99), controls::ButtonProps::new("After", controls::HostIntent::Command(99))).unwrap().root;
    rndr_paint::frame(&mut app);
    let row = h.row(app.world(), ItemId(1)).unwrap().root;
    let toggle = children_of(app.world(), children_of(app.world(), row)[0])[0];
    (app, root, h, toggle, after)
}
#[test]
fn disclosure_enter_then_right_in_one_batch_respects_chronology() {
    let (mut app, _, h, toggle, _) = setup();
    assert!(request_focus(app.world_mut(), toggle));
    app.world_mut().resource_mut::<Input>().unwrap().keys = [NamedKey::Enter, NamedKey::ArrowRight].into_iter().map(|k| KeyEvent::named(k, Modifiers::default())).collect();
    rndr_paint::frame(&mut app);
    // Enter expands first; Right then moves to the now-visible child.
    assert!(h.expanded(app.world(), ItemId(1)), "queued Enter must not undo later Right");
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
}
#[test]
fn at_disclosure_activate_then_tab_does_not_steal_focus_back() {
    let (mut app, _, h, toggle, after) = setup();
    assert_eq!(try_apply_access(app.world_mut(), AccessCommand::Focus(toggle)), AccessCommandResult::Applied);
    assert_eq!(try_apply_access(app.world_mut(), AccessCommand::Activate(toggle)), AccessCommandResult::Activation(ActivationResult::Accepted));
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after), "late disclosure drain must not override chronological Tab");
}
#[test]
fn pointer_disclosure_release_then_tab_does_not_steal_focus_back() {
    let (mut app, _, h, toggle, after) = setup();
    let r = *app.world().get::<Rect>(toggle).unwrap();
    let p = [r.x + r.w / 2.0, r.y + r.h / 2.0];
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer { position: p, press_at: Some(p), pressed: true, down: true, ..Pointer::default() };
    rndr_paint::frame(&mut app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer { position: p, released: true, ..Pointer::default() };
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after), "deferred pointer disclosure must not override later Tab");
}
#[test]
fn programmatic_expansion_does_not_cancel_unrelated_armed_host_button() {
    let (mut app, _, h, _, after) = setup();
    request_focus(app.world_mut(), after);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(interaction_pressed(app.world(), after));
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after));
    let mut release = KeyEvent::named(NamedKey::Space, Modifiers::default());
    release.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![release];
    rndr_paint::frame(&mut app);
    let requests = take_action_intents(app.world_mut());
    assert!(requests.iter().any(|i| i.entity == after), "unrelated expansion must not swallow armed host release");
}
#[test]
fn focused_disclosure_becoming_leaf_normalizes_to_surviving_row() {
    let (mut app, root, h, toggle, _) = setup();
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    request_focus(app.world_mut(), toggle);
    let nodes = [TreeNode { item: Item::new(1, "Now leaf"), parent: None }];
    let updated = TreeView::mount(app.world_mut(), root, UiId(20), ViewProps::new("Tree"), &nodes, |_, _, _| {}).unwrap();
    assert_eq!(h, updated);
    assert!(!h.expanded(app.world(), ItemId(1)));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(h.row(app.world(), ItemId(1)).unwrap().root));
    rndr_paint::frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
}
#[test]
fn disclosure_enter_then_right_in_separate_frames_is_control() {
    let (mut app, _, h, toggle, _) = setup();
    request_focus(app.world_mut(), toggle);
    for k in [NamedKey::Enter, NamedKey::ArrowRight] {
        app.world_mut().resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(k, Modifiers::default())];
        rndr_paint::frame(&mut app);
    }
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
}
