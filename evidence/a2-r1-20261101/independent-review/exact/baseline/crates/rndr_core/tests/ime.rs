use rndr_core::{
    App, Axis, Buffer, CapturesKeys, Chrome, Color, Editor, Focus, ImePreview, Input, Label,
    Layout, Shell, Size, TextField, TextLayout, Viewport,
};

fn field() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
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
        Label::new("hi"),
        TextField,
        CapturesKeys,
        rndr_core::Focusable,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn editor() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(240.0, 80.0));
    app.world_mut().insert_resource(Focus::default());
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(200.0),
            height: Size::Px(80.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.14, 0.15, 0.16),
        Label::new(""),
        Buffer::from_str("hi"),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    app.update();
    (app, entity)
}

#[test]
fn preedit_cursor_places_the_ime_caret() {
    let (mut app, entity) = field();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit = "abc".into();
    input.ime_cursor = Some((1, 1));
    app.update();
    let layout = app.world().get::<TextLayout>(entity).unwrap();
    // "hi" is 2 bytes; the preedit is inserted at the field caret.
    assert_eq!(layout.ime_range, Some((2, 5)));
    assert_eq!(layout.ime_cursor, Some(3));
}

#[test]
fn preedit_does_not_change_a_field_until_commit() {
    let (mut app, entity) = field();
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "xy".into();
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hi");
    assert_eq!(app.world().get::<ImePreview>(entity).unwrap().text, "xy");
    assert!(app
        .world()
        .get::<TextLayout>(entity)
        .unwrap()
        .glyphs
        .iter()
        .any(|glyph| glyph.ch == 'x'));

    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "z".into();
    app.update();
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hiz");
    assert!(app.world().get::<ImePreview>(entity).is_none());
}

#[test]
fn preedit_does_not_change_a_buffer_until_commit() {
    let (mut app, entity) = editor();
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "xy".into();
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hi");
    assert_eq!(app.world().get::<ImePreview>(entity).unwrap().text, "xy");
    assert!(app
        .world()
        .get::<TextLayout>(entity)
        .unwrap()
        .glyphs
        .iter()
        .any(|glyph| glyph.ch == 'y'));

    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "z".into();
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hiz");
    assert!(app.world().get::<ImePreview>(entity).is_none());
}
