use rndr_core::*;
use rndr_ui::{
    controls::{self, ButtonProps, HostIntent},
    *,
};

fn setup() -> (App, Entity, MarkdownSlot, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 300.));
    let view = markdown_view(app.world_mut(), "Live\n\nFallback paragraph");
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    rndr_paint::frame(&mut app);
    let block = app.world().get::<MarkdownView>(view).unwrap().blocks()[0].clone();
    let slot = mount_markdown_slot(app.world_mut(), view, 0, &block, MarkdownSlotId(1)).unwrap();
    let button = controls::Button::mount(
        app.world_mut(),
        slot.root,
        UiId(1),
        ButtonProps::new("Live button", HostIntent::Command(1)),
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    (app, view, slot, button.root)
}
fn press(app: &mut App, entity: Entity) {
    let r = *app.world().get::<Rect>(entity).unwrap();
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position = [r.x + 5., r.y + 5.];
    p.pressed = true;
    p.down = true;
    rndr_paint::frame(app);
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.pressed = false;
    p.down = false;
    p.released = true;
    rndr_paint::frame(app);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .released = false;
}
#[test]
fn clicking_fallback_after_embedded_focus_restores_markdown_copy() {
    let (mut app, view, _, button) = setup();
    request_focus(app.world_mut(), button);
    let fallback = children_of(app.world(), view)[1];
    press(&mut app, fallback);
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .select_all();
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Copy);
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "Live\n\nFallback paragraph"
    );
    // The text click must also stop Space/Enter from firing the old control.
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(take_action_intents(app.world_mut()).is_empty());
}
#[test]
fn embedded_editor_drag_does_not_scroll_markdown_parent() {
    let (mut app, view, slot, _) = setup();
    slot.set_height(app.world_mut(), 600.).unwrap();
    let editor = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(100.),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(
            "first line\nsecond line\nthird line\nfourth line\nfifth line\nsixth line",
        ),
        Editor::default(),
        CapturesKeys,
        Focusable,
    ));
    set_parent(app.world_mut(), editor, slot.root);
    rndr_paint::frame(&mut app);
    request_focus(app.world_mut(), editor);
    let r = *app.world().get::<Rect>(editor).unwrap();
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position = [r.x + TEXT_ORIGIN_X + 10., r.y + TEXT_ORIGIN_Y + 30.];
    p.pressed = true;
    p.down = true;
    rndr_paint::frame(&mut app);
    let before = app.world().get::<Scroll>(view).unwrap().offset;
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position[1] -= 20.;
    p.pressed = false;
    p.down = true;
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Scroll>(view).unwrap().offset, before);
    let buffer = app.world().get::<Buffer>(editor).unwrap();
    assert_ne!(buffer.caret, buffer.anchor);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.down = false;
    input.pointer.released = true;
    rndr_paint::frame(&mut app);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.released = false;
    input.scroll = [0., -20.];
    let editor_before = app.world().get::<Editor>(editor).unwrap().offset;
    rndr_paint::frame(&mut app);
    assert!(app.world().get::<Editor>(editor).unwrap().offset > editor_before);
    assert_eq!(app.world().get::<Scroll>(view).unwrap().offset, before);
    // Only the remainder at the editor's canonical boundary reaches markdown.
    app.world_mut().resource_mut::<Input>().unwrap().scroll = [0., -100_000.];
    rndr_paint::frame(&mut app);
    assert!(app.world().get::<Scroll>(view).unwrap().offset[1] < before[1]);
}
#[test]
fn text_press_notifies_focus_boundary_once_and_cancels_armed_space() {
    fn lost(world: &mut World, _: Entity, next: Option<Entity>) {
        assert_eq!(next, None);
        *world.resource_mut::<usize>().unwrap() += 1;
    }
    let (mut app, view, slot, button) = setup();
    app.world_mut().insert_resource(0usize);
    app.world_mut().insert(slot.root, FocusBoundary(lost));
    request_focus(app.world_mut(), button);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .clear();
    let fallback = children_of(app.world(), view)[1];
    press(&mut app, fallback);
    assert_eq!(*app.world().resource::<usize>().unwrap(), 1);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default()).released()];
    rndr_paint::frame(&mut app);
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert_eq!(*app.world().resource::<usize>().unwrap(), 1);
}
#[test]
fn text_click_in_another_view_reclaims_copy_from_old_host_focus() {
    let (mut app, view, _, button) = setup();
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    app.world_mut().get_mut::<Layout>(view).unwrap().height = Size::Px(150.);
    set_parent(app.world_mut(), view, root);
    let other = markdown_view(app.world_mut(), "Second view");
    app.world_mut().get_mut::<Layout>(other).unwrap().height = Size::Px(150.);
    set_parent(app.world_mut(), other, root);
    rndr_paint::frame(&mut app);
    request_focus(app.world_mut(), button);
    let fallback = children_of(app.world(), other)[0];
    press(&mut app, fallback);
    app.world_mut()
        .get_mut::<MarkdownView>(other)
        .unwrap()
        .select_all();
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Copy);
    rndr_paint::frame(&mut app);
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "Second view"
    );
}
#[test]
fn source_range_stays_with_the_original_block() {
    let (mut app, view, slot, button) = setup();
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_markdown("Live\n\nLive\n\nFallback paragraph");
    rndr_paint::frame(&mut app);
    // The slot stays on the first "Live": the inserted copy is a later range.
    assert!(slot.is_mounted(app.world()));
    assert!(app.world().is_alive(button));
    // Replacing the anchored paragraph deletes its source range.
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_markdown("Different\n\nLive\n\nLive");
    rndr_paint::frame(&mut app);
    assert!(!slot.is_mounted(app.world()));
    assert!(!app.world().is_alive(button));
}
#[test]
fn reparented_descendant_is_no_longer_owned_by_slot_subtree() {
    let (mut app, view, slot, button) = setup();
    let outside = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), button, outside);
    slot.detach(app.world_mut()).unwrap();
    assert!(app.world().is_alive(button)); // Not in the owned subtree anymore.
    despawn(app.world_mut(), outside);
    assert!(!app.world().is_alive(button));
    // Ordinary markdown drag/wheel behavior remains covered by core markdown/layout tests.
    assert!(app.world().is_alive(view));
}
#[test]
fn fabricated_public_handle_fields_cannot_mutate_another_binding() {
    let (mut app, view, slot, button) = setup();
    let other = markdown_view(app.world_mut(), "Other");
    rndr_paint::frame(&mut app);
    for forged in [
        MarkdownSlot {
            root: button,
            ..slot
        },
        MarkdownSlot {
            view: other,
            ..slot
        },
        MarkdownSlot {
            id: MarkdownSlotId(42),
            ..slot
        },
    ] {
        assert!(!forged.is_mounted(app.world()));
        assert_eq!(
            forged.set_height(app.world_mut(), 30.),
            Err(MarkdownSlotError::StaleHandle)
        );
        assert_eq!(
            forged.detach(app.world_mut()),
            Err(MarkdownSlotError::StaleHandle)
        );
        assert!(slot.is_mounted(app.world()));
        assert!(app.world().is_alive(button));
    }
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .push_str(" pending");
    assert!(!slot.is_mounted(app.world()));
    slot.detach(app.world_mut()).unwrap();
    assert!(!app.world().is_alive(button));
    rndr_paint::frame(&mut app);
    assert_eq!(children_of(app.world(), view).len(), 2);
}
