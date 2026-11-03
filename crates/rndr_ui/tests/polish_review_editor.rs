use rndr_core::*;
use rndr_ui::UiPlugin;

#[test]
fn wheel_over_focused_editor_moves_only_scroll_ancestry_and_preserves_selection() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(200., 150.));
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.),
            height: Size::Px(150.),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let mut buffer = Buffer::from_str("one two three");
    buffer.caret = 7;
    buffer.anchor = 4;
    let editor = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(100.),
            ..Layout::column()
        },
        Editor::default(),
        buffer,
        Caret {
            index: 7,
            anchor: 4,
        },
        Focusable,
    ));
    set_parent(app.world_mut(), editor, root);
    let tail = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(300.),
        ..Layout::column()
    },));
    set_parent(app.world_mut(), tail, root);
    rndr_paint::frame(&mut app);
    assert!(request_focus(app.world_mut(), editor));
    rndr_paint::frame(&mut app);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [50., 50.];
    input.scroll = [0., -0.25];
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Scroll>(root).unwrap().offset, [0., -0.25]);
    assert_eq!(app.world().get::<Editor>(editor).unwrap().offset, 0.);
    let caret = app.world().get::<Caret>(editor).unwrap();
    assert_eq!((caret.index, caret.anchor), (7, 4));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(editor)
    );
    // An idle frame must not undo explicit wheel movement via focus reveal.
    rndr_paint::frame(&mut app);
    assert_eq!(app.world().get::<Scroll>(root).unwrap().offset[1], -0.25);
}
