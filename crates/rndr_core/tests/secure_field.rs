use rndr_core::{
    access_id, access_tree, App, Axis, CapturesKeys, Caret, Chrome, Clipboard, Color, Focus,
    Focusable, Input, KeyEvent, Label, Layout, Modifiers, Platform, Secure, Shell, Size, TextField,
    TextLayout, Viewport,
};

fn boot(secure: bool) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    app.world_mut().insert_resource(Platform::X11);
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(36.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("pé"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    if secure {
        app.world_mut().insert(entity, Secure);
    }
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn ctrl(app: &mut App, c: char) {
    let mut mods = Modifiers::new();
    mods.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(c, mods));
    app.update();
}

#[test]
fn a_secure_field_edits_its_value_but_shows_and_shares_only_bullets() {
    let (mut app, entity) = boot(true);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push('x');
    app.update();
    let w = app.world();
    assert_eq!(
        w.get::<Label>(entity).unwrap().text,
        "péx",
        "the value is real"
    );
    assert_eq!(w.get::<Caret>(entity).unwrap().index, "péx".len());
    // Shaped as three bullets, with caret stops on the real bytes.
    let layout = w.get::<TextLayout>(entity).unwrap();
    let line = &layout.lines[0];
    assert_eq!((line.start, line.end), (0, "péx".len()));
    let mut stops: Vec<usize> = line.edges.iter().map(|e| e.0).collect();
    stops.sort_unstable();
    stops.dedup();
    assert_eq!(stops, [0, 1, 3, 4]);
    // Assistive tech hears bullets.
    let tree = access_tree(w);
    let node = tree
        .nodes
        .iter()
        .find(|n| n.id == access_id(entity))
        .unwrap();
    assert_eq!(node.value, "•••");

    // Select all, copy and cut: neither the clipboard nor the primary
    // selection carries the value.
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    let clip = app.world().resource::<Clipboard>().unwrap();
    assert!(clip.text.is_empty() && !clip.dirty, "copy is refused");
    assert!(clip.primary.is_empty(), "no primary selection");
    ctrl(&mut app, 'x');
    let clip = app.world().resource::<Clipboard>().unwrap();
    assert!(clip.text.is_empty() && !clip.dirty, "cut is refused");
}

#[test]
fn a_plain_field_still_copies() {
    let (mut app, entity) = boot(false);
    ctrl(&mut app, 'a');
    ctrl(&mut app, 'c');
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "pé");
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "pé");
}
