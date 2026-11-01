use rndr_core::{
    App, Axis, CapturesKeys, Caret, Chrome, Clipboard, Color, Focus, Focusable, Input, Key, Label,
    Layout, Shell, Size, TextField, Viewport,
};

fn boot() -> (App, rndr_core::Entity) {
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
        Label::new("hello"),
        TextField,
        Focusable,
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn select(app: &mut App, entity: rndr_core::Entity, anchor: usize, index: usize) {
    app.world_mut().insert(entity, Caret { index, anchor });
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
}

#[test]
fn copy_cut_and_paste_the_selection() {
    let (mut app, entity) = boot();
    select(&mut app, entity, 1, 4);
    press(&mut app, Key::Copy);
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "ell");
    assert!(app.world().resource::<Clipboard>().unwrap().dirty);
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hello");

    select(&mut app, entity, 1, 4);
    press(&mut app, Key::Cut);
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "ho");
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 1);
    assert_eq!(app.world().resource::<Clipboard>().unwrap().text, "ell");

    app.world_mut().resource_mut::<Clipboard>().unwrap().text = "Z".into();
    press(&mut app, Key::Paste);
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hZo");
    assert_eq!(app.world().get::<Caret>(entity).unwrap().index, 2);
}
