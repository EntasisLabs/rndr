//! Scratch composer regressions through the native editor/input pipeline.
use rndr_core::*;

fn boot(text: &str) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(320.0, 120.0));
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(64.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    app.update();
    (app, entity)
}

fn assert_eof_visible(app: &App, entity: Entity) {
    let buffer = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!(buffer.caret, buffer.text().len());
    let caret = app.world().get::<Caret>(entity).expect("published caret");
    let label = app.world().get::<Label>(entity).unwrap();
    let layout = app.world().get::<TextLayout>(entity).unwrap();
    let rect = app.world().get::<Rect>(entity).unwrap();
    assert_eq!(
        caret.index,
        label.text.len(),
        "EOF must be in the visible window"
    );
    assert!(label.text.is_char_boundary(caret.index));
    let hint = app.world().get::<CaretRow>(entity);
    let (_, y) = hint.map_or_else(
        || layout.caret_xy(caret.index),
        |hint| layout.caret_xy_on_line(caret.index, hint.row),
    );
    let line = layout.lines.iter().find(|line| line.y == y).unwrap();
    assert!(line.start <= caret.index && caret.index <= line.end);
    assert!(y >= 0.0);
    assert!(
        y + line.h + 2.0 * TEXT_ORIGIN_Y <= rect.h,
        "caret row at {y} with height {} is clipped by {}",
        line.h,
        rect.h
    );
}

fn type_text(app: &mut App, text: &str) {
    app.world_mut().resource_mut::<Input>().unwrap().chars = text.into();
    app.update();
}

#[test]
fn repeated_typing_resize_and_long_eof_appends_keep_caret_in_window() {
    let (mut app, entity) = boot("");
    for ch in "alpha beta gamma delta epsilon zeta eta theta ".chars() {
        type_text(&mut app, &ch.to_string());
        assert_eof_visible(&app, entity);
    }
    app.world_mut().get_mut::<Layout>(entity).unwrap().width = Size::Px(130.0);
    app.update();
    app.update();
    let paragraph = "long paragraph with enough words to wrap across many visual rows ".repeat(4);
    for _ in 0..3 {
        type_text(&mut app, &paragraph);
        assert_eof_visible(&app, entity);
    }
    assert!(app.world().get::<Editor>(entity).unwrap().offset > 0.0);
    // Repeated Backspace must shrink/rebuild the tail and reveal the new EOF.
    for _ in 0..80 {
        app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Backspace);
        app.update();
        assert_eof_visible(&app, entity);
    }
}

#[test]
fn newline_appends_and_grapheme_backspaces_keep_geometry_current() {
    let (mut app, entity) = boot("first paragraph\n");
    for text in [
        "é ",
        "e\u{301} ",
        "👩\u{200d}💻 ",
        "🇺🇸",
        "\n",
        "another long line of words that wraps over the viewport",
    ] {
        type_text(&mut app, text);
        assert_eof_visible(&app, entity);
    }
    while !app.world().get::<Buffer>(entity).unwrap().text().is_empty() {
        app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Backspace);
        app.update();
        assert_eof_visible(&app, entity);
    }
}
