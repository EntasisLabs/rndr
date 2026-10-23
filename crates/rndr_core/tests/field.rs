use rndr_core::{
    events, App, Axis, CapturesKeys, Caret, Chrome, Clipboard, Color, Focus, Focusable, Input, Key,
    KeyEvent, Label, Layout, Modifiers, NamedKey, Platform, Rect, Schedule, Shell, Size, Submitted,
    TextChanged, TextField, Viewport, World, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};

fn field_layout() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(200.0),
        height: Size::Px(36.0),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn boot() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    let entity = app.world_mut().spawn((
        field_layout(),
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("hello"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    app.update();
    (app, entity)
}

fn focus(app: &mut App, entity: rndr_core::Entity) {
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
}

#[test]
fn type_and_backspace_edit_the_label() {
    let (mut app, entity) = boot();
    focus(&mut app, entity);
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 5);

    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push('x');
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hellox");
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 6);

    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Backspace);
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hello");
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 5);
}

#[test]
fn keyboard_selection_sets_the_primary() {
    let (mut app, entity) = boot();
    focus(&mut app, entity);
    app.world_mut().insert_resource(Platform::X11);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::ArrowLeft, shift_mod()));
    app.update();
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.primary, "o");
    assert!(clipboard.primary_dirty);

    app.world_mut()
        .resource_mut::<Clipboard>()
        .unwrap()
        .primary_dirty = false;
    let mut mods = Modifiers::new();
    mods.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('a', mods));
    app.update();
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert_eq!(clipboard.primary, "hello");
    assert!(clipboard.primary_dirty);

    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut()
        .resource_mut::<Clipboard>()
        .unwrap()
        .primary
        .clear();
    app.world_mut()
        .resource_mut::<Clipboard>()
        .unwrap()
        .primary_dirty = false;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('a', mods));
    app.update();
    let clipboard = app.world().resource::<Clipboard>().unwrap();
    assert!(clipboard.primary.is_empty());
    assert!(!clipboard.primary_dirty);
}

fn shift_mod() -> Modifiers {
    let mut modifiers = Modifiers::new();
    modifiers.shift = true;
    modifiers
}

#[test]
fn click_places_the_index() {
    let (mut app, entity) = boot();
    let rect = *app.world().get::<Rect>(entity).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [
        rect.x + TEXT_ORIGIN_X + CELL_W + 1.0,
        rect.y + TEXT_ORIGIN_Y + 1.0,
    ];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();

    let caret = app.world().get::<Caret>(entity).expect("caret");
    assert_eq!(caret.index, 1);
    assert_eq!(caret.anchor, 1);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(entity)
    );
}

struct Seen {
    text: String,
    changed: Vec<String>,
    submitted: Vec<String>,
}

fn watch_field(world: &mut World) {
    let entity = world.entities_with::<TextField>()[0];
    let text = world
        .get::<Label>(entity)
        .map(|label| label.text.clone())
        .unwrap_or_default();
    let changed = events::<TextChanged>(world)
        .iter()
        .map(|event| event.text.clone())
        .collect();
    let submitted = events::<Submitted>(world)
        .iter()
        .map(|event| event.text.clone())
        .collect();
    world.insert_resource(Seen {
        text,
        changed,
        submitted,
    });
}

#[test]
fn text_changed_is_visible_to_update_on_the_same_frame() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.add_system(Schedule::Update, watch_field);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    let entity = app.world_mut().spawn((
        field_layout(),
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("hello"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    assert_eq!(app.world().resource::<Seen>().unwrap().text, "hello");
    assert!(app.world().resource::<Seen>().unwrap().changed.is_empty());

    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push('x');
    app.update();
    let seen = app.world().resource::<Seen>().unwrap();
    assert_eq!(seen.text, "hellox");
    assert_eq!(seen.changed, vec!["hellox".to_string()]);
    assert!(seen.submitted.is_empty());
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hellox");
}

#[test]
fn enter_submits_without_inserting_a_newline() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.add_system(Schedule::Update, watch_field);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    let entity = app.world_mut().spawn((
        field_layout(),
        Color::rgb(0.15, 0.16, 0.18),
        Label::new("hello"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::Enter, Modifiers::new()));
    app.update();
    let seen = app.world().resource::<Seen>().unwrap();
    assert_eq!(seen.text, "hello");
    assert!(seen.changed.is_empty());
    assert_eq!(seen.submitted, vec!["hello".to_string()]);
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hello");
}
