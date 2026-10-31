#![cfg(feature = "parley")]

use rndr_core::{
    edit_buffer, App, Buffer, CapturesKeys, Chrome, Color, Editor, Focus, Input, KeyEvent, Label,
    Layout, Modifiers, Platform, Rect, Size, Spans, TextEngine, TextLayout, TextSpan, TextStyle,
    Viewport, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_render::ParleyText;
use std::sync::Arc;

fn refresh_spans(app: &mut App, entity: rndr_core::Entity) {
    let len = app.world().get::<Buffer>(entity).unwrap().len();
    app.world_mut().insert(
        entity,
        Spans {
            runs: vec![TextSpan::new(6, len, Color::rgb(0.8, 0.7, 0.6))
                .weight(700)
                .italic()],
        },
    );
}

fn frame(app: &mut App) {
    let _ = rndr_paint::frame(app);
}

fn chord(app: &mut App, ch: char) {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(ch, modifiers));
    frame(app);
}

#[test]
fn styled_unicode_source_edit_undo_resize_and_hit_testing_use_shaped_rows() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Viewport::new(500.0, 600.0));
    let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    let source = "- [ ] AV fi café مرحبا 👩‍💻 alpha beta gamma delta\nnext block";
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(400.0),
            height: Size::Px(560.0),
            ..Layout::column()
        },
        Buffer::from_str(source),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
        TextStyle {
            family: "Inter".into(),
            size: 18.0,
            ..Default::default()
        },
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    refresh_spans(&mut app, entity);
    frame(&mut app);
    frame(&mut app);
    let wide_rows = app.world().get::<TextLayout>(entity).unwrap().lines.len();
    assert!(
        app.world()
            .get::<TextLayout>(entity)
            .unwrap()
            .glyphs
            .iter()
            .any(|g| g.atlas == 1),
        "real emoji shaping"
    );
    edit_buffer(app.world_mut(), entity, 3..4, "done").unwrap();
    refresh_spans(&mut app, entity);
    frame(&mut app);
    let changed = source.replacen("[ ]", "[done]", 1);
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, changed);
    chord(&mut app, 'z');
    refresh_spans(&mut app, entity);
    frame(&mut app);
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), source);
    chord(&mut app, 'y');
    refresh_spans(&mut app, entity);
    frame(&mut app);
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), changed);
    app.world_mut().get_mut::<Layout>(entity).unwrap().width = Size::Px(150.0);
    frame(&mut app);
    frame(&mut app);
    let layout = app.world().get::<TextLayout>(entity).unwrap();
    assert!(
        layout.lines.len() > wide_rows,
        "narrow resize must reflow styled Unicode source"
    );
    assert_eq!(app.world().get::<Label>(entity).unwrap().text, changed);
    let index = changed.find('A').unwrap();
    let (x, y) = layout.caret_xy(index);
    let line = layout.lines.iter().find(|line| line.y == y).unwrap();
    let expected = layout.index_at(x, y + line.h / 2.0);
    assert_eq!(expected, index);
    let rect = *app.world().get::<Rect>(entity).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + TEXT_ORIGIN_X + x, rect.y + TEXT_ORIGIN_Y + y + 2.0];
    input.pointer.pressed = true;
    input.pointer.down = true;
    frame(&mut app);
    let buffer = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((buffer.caret, buffer.anchor), (expected, expected));
    assert_eq!(buffer.text(), changed);
}
