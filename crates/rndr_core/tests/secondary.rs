use rndr_core::{
    events, App, Button, Color, ContextTarget, Entity, Fired, Focus, Focusable, GenericInteraction,
    Input, KeyEvent, Label, Layout, Modifiers, NamedKey, Platform, PointerButton, Rect,
    SecondaryPress, Size, Viewport,
};

fn boot(platform: Platform) -> (App, Entity, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(platform);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let card = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(80.0),
            padding: 10.0,
            ..Layout::column()
        },
        Color::rgb(0.2, 0.2, 0.2),
        ContextTarget,
        Focusable,
        Button {
            action: "open".into(),
            enabled: true,
        },
    ));
    rndr_core::set_parent(app.world_mut(), card, root);
    let child = app.world_mut().spawn((
        Layout {
            width: Size::Px(60.0),
            height: Size::Px(20.0),
            ..Layout::column()
        },
        Label::new("Inside"),
    ));
    rndr_core::set_parent(app.world_mut(), child, card);
    let plain = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Color::rgb(0.3, 0.3, 0.3),
    ));
    rndr_core::set_parent(app.world_mut(), plain, root);
    app.update();
    (app, card, child, plain)
}

fn center(app: &App, e: Entity) -> [f32; 2] {
    let r = *app.world().get::<Rect>(e).unwrap();
    [r.x + r.w / 2.0, r.y + r.h / 2.0]
}

/// Press then release; returns what the press frame sent and fired.
fn press(
    app: &mut App,
    at: [f32; 2],
    button: PointerButton,
    ctrl: bool,
) -> (Vec<SecondaryPress>, Option<String>) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.button = button;
    input.pointer.pressed = true;
    input.pointer.down = true;
    input.ctrl = ctrl;
    app.update();
    let sent = events::<SecondaryPress>(app.world()).to_vec();
    let mut action = fired(app);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.down = false;
    input.pointer.released = true;
    app.update();
    action = action.or(fired(app));
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .released = false;
    (sent, action)
}

fn key(app: &mut App, event: KeyEvent) -> Vec<SecondaryPress> {
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![event];
    app.update();
    events::<SecondaryPress>(app.world()).to_vec()
}

fn fired(app: &App) -> Option<String> {
    app.world().resource::<Fired>().and_then(|f| f.name.clone())
}

#[test]
fn right_click_reports_the_nearest_target_without_activating_it() {
    let (mut app, card, child, plain) = boot(Platform::X11);
    let at = center(&app, child);
    let (sent, action) = press(&mut app, at, PointerButton::Right, false);
    assert_eq!(action, None);
    assert_eq!(
        sent,
        vec![SecondaryPress {
            entity: card,
            at,
            keyboard: false
        }]
    );
    let at = center(&app, plain);
    assert!(press(&mut app, at, PointerButton::Right, false)
        .0
        .is_empty());
}

#[test]
fn control_click_is_secondary_only_on_mac() {
    let (mut app, card, _, _) = boot(Platform::Mac);
    let at = center(&app, card);
    let (sent, action) = press(&mut app, at, PointerButton::Left, true);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].entity, card);
    assert_eq!(action, None);
    assert_ne!(
        app.world().resource::<Focus>().and_then(|f| f.entity),
        Some(card)
    );

    let (mut app, card, _, _) = boot(Platform::X11);
    let at = center(&app, card);
    let (sent, action) = press(&mut app, at, PointerButton::Left, true);
    assert!(sent.is_empty());
    assert_eq!(action.as_deref(), Some("open"));
}

#[test]
fn menu_key_and_shift_f10_open_for_the_focused_target() {
    let (mut app, card, _, _) = boot(Platform::X11);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(card);
    app.update();
    let r = *app.world().get::<Rect>(card).unwrap();
    let expected = vec![SecondaryPress {
        entity: card,
        at: [r.x, r.y + r.h],
        keyboard: true,
    }];
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..none
    };
    assert_eq!(
        key(&mut app, KeyEvent::named(NamedKey::ContextMenu, none)),
        expected
    );
    assert_eq!(
        key(&mut app, KeyEvent::named(NamedKey::F(10), shift)),
        expected
    );
    assert!(key(&mut app, KeyEvent::named(NamedKey::F(10), none)).is_empty());
    let ctrl_shift = Modifiers {
        ctrl: true,
        ..shift
    };
    assert!(key(&mut app, KeyEvent::named(NamedKey::F(10), ctrl_shift)).is_empty());
    let mut repeat = KeyEvent::named(NamedKey::ContextMenu, none);
    repeat.repeat = true;
    assert!(key(&mut app, repeat).is_empty());
    assert_eq!(fired(&app), None);

    app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
    app.update();
    assert!(key(&mut app, KeyEvent::named(NamedKey::ContextMenu, none)).is_empty());
}
