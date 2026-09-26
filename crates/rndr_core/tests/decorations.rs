use rndr_core::{
    App, Axis, Buffer, Chrome, Color, Decoration, DecorationKind, DecorationMarks, Decorations,
    Editor, Label, Layout, Size, Viewport, CELL_H,
};

fn boot(offset: f32) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let body = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(280.0),
            height: Size::Px(200.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Color::rgb(0.14, 0.15, 0.16),
        Label::new(""),
        Buffer::from_str(&"a\n".repeat(80)),
        Editor { offset },
        Decorations {
            list: vec![Decoration {
                start: 49 * 2,
                end: 49 * 2 + 1,
                kind: DecorationKind::Find,
            }],
        },
    ));
    app.update();
    app.update();
    (app, body)
}

#[test]
fn offscreen_match_has_no_mark() {
    let (app, body) = boot(0.0);
    assert!(app
        .world()
        .get::<DecorationMarks>(body)
        .unwrap()
        .marks
        .is_empty());
}

#[test]
fn visible_line_50_is_marked() {
    let (app, body) = boot(49.0 * CELL_H);
    let marks = &app.world().get::<DecorationMarks>(body).unwrap().marks;
    assert_eq!(marks.len(), 1);
    assert_eq!(marks[0].kind, DecorationKind::Find);
}
