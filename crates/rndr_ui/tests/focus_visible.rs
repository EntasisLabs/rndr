use rndr_core::*;
use rndr_ui::{
    controls::{self, *},
    *,
};

fn ring_at(f: &rndr_paint::Frame, r: Rect, theme: &UiTheme) -> bool {
    f.quads
        .iter()
        .any(|q| q.rect == r && q.color == theme.color(Token::Focus))
}

#[test]
fn pointer_focus_hides_the_ring_until_the_keyboard_is_used() {
    let theme = UiTheme::default();
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(InteractionMotion::snap());
    a.world_mut().insert_resource(theme.clone());
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(240.0),
        ..Layout::column()
    },));
    let first = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("First", HostIntent::Command(1)),
    )
    .unwrap();
    let second = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(2),
        ButtonProps::new("Second", HostIntent::Command(2)),
    )
    .unwrap();
    a.update();
    let r = *a.world().get::<Rect>(first.root).unwrap();

    // A click focuses the button but draws no keyboard ring.
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.position = [r.x + 12.0, r.y + 12.0];
    i.pointer.pressed = true;
    i.pointer.down = true;
    i.pointer.press_at = Some(i.pointer.position);
    let f = rndr_paint::frame(&mut a);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(first.root)
    );
    assert!(!focus_ring_visible(a.world(), first.root));
    assert!(!ring_at(&f, r, &theme));

    // Tab moves focus by keyboard, so the ring returns.
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.pressed = false;
    i.pointer.down = false;
    i.pointer.press_at = None;
    i.pointer.position = [-1.0, -1.0];
    i.keys = vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    let f = rndr_paint::frame(&mut a);
    let r2 = *a.world().get::<Rect>(second.root).unwrap();
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(second.root)
    );
    assert!(focus_ring_visible(a.world(), second.root));
    assert!(ring_at(&f, r2, &theme));
}

#[test]
fn tooltips_follow_keyboard_focus_not_pointer_focus() {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(InteractionMotion::snap());
    let root = a.world_mut().spawn((Layout {
        width: Size::Px(240.0),
        ..Layout::column()
    },));
    let first = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(1),
        ButtonProps::new("First", HostIntent::Command(1)),
    )
    .unwrap();
    let second = controls::Button::mount(
        a.world_mut(),
        root,
        UiId(2),
        ButtonProps::new("Second", HostIntent::Command(2)),
    )
    .unwrap();
    let tip = inputs::Tooltip::attach(a.world_mut(), second.root, "More about second").unwrap();
    a.update();
    // Click the first button, then move focus to the second in code, as a
    // popup does when it opens: no tooltip without keyboard use.
    let r = *a.world().get::<Rect>(first.root).unwrap();
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.position = [r.x + 4.0, r.y + 4.0];
    i.pointer.pressed = true;
    i.pointer.press_at = Some(i.pointer.position);
    a.update();
    let i = a.world_mut().resource_mut::<Input>().unwrap();
    i.pointer.pressed = false;
    i.pointer.press_at = None;
    i.pointer.position = [-1.0, -1.0];
    assert!(request_focus(a.world_mut(), second.root));
    a.update();
    assert!(a.world().has::<Hidden>(tip.root));
    // Shift+Tab then Tab returns by keyboard: the tooltip shows.
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, shift)];
    a.update();
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    a.update();
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(second.root)
    );
    assert!(!a.world().has::<Hidden>(tip.root));
}
