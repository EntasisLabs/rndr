use rndr_core::{
    App, Axis, CapturesKeys, Caret, Chrome, Color, Focus, Focusable, Input, Key, Label, Layout,
    Rect, Shell, Size, TextField, Viewport, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
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
