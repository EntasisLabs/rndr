use rndr_core::{
    App, Axis, Buffer, CapturesKeys, Caret, Chrome, Editor, Focus, Input, Key, KeyEvent, Label,
    Layout, Modifiers, NamedKey, Size, Viewport,
};

fn boot(text: &str) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 100.0));
    app.world_mut().insert_resource(Focus::default());
    let entity = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 0.0,
            padding: 0.0,
            width: Size::Px(280.0),
            height: Size::Px(80.0),
            min_width: 0.0,
            min_height: 0.0,
        },
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    app.update();
    (app, entity)
}

#[test]
fn blooey_b001_legacy_up() {
    let (mut app, entity) = boot("hello\nworld");
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Up);
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().caret, 5);
}

#[test]
fn blooey_b001_queued_up() {
    let (mut app, entity) = boot("hello\nworld");
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::ArrowUp, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().caret, 5);
}

#[test]
fn blooey_b002_typing_reveals_caret() {
    let (mut app, entity) = boot(&"line\n".repeat(100));
    app.world_mut().resource_mut::<Input>().unwrap().chars = "x".into();
    app.update();
    assert!(app.world().get::<Caret>(entity).is_some());
    assert!(app.world().get::<Editor>(entity).unwrap().offset > 0.0);
    assert_visible(&app, entity);
}

fn press(app: &mut App, key: Key, shift: bool) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.key = Some(key);
    input.shift = shift;
    app.update();
}

fn position(app: &mut App, entity: rndr_core::Entity, index: usize, anchor: usize) {
    let buffer = app.world_mut().get_mut::<Buffer>(entity).unwrap();
    buffer.caret = index;
    buffer.anchor = anchor;
}

fn assert_visible(app: &App, entity: rndr_core::Entity) {
    let caret = app.world().get::<Caret>(entity).expect("published caret");
    let label = app.world().get::<Label>(entity).unwrap();
    let layout = app.world().get::<rndr_core::TextLayout>(entity).unwrap();
    let rect = app.world().get::<rndr_core::Rect>(entity).unwrap();
    assert!(label.text.is_char_boundary(caret.index));
    assert!(label.text.is_char_boundary(caret.anchor));
    let hint = app.world().get::<rndr_core::CaretRow>(entity);
    let (_, y) = hint.map_or_else(
        || layout.caret_xy(caret.index),
        |hint| layout.caret_xy_on_line(caret.index, hint.row),
    );
    let line = layout.lines.iter().find(|line| line.y == y).unwrap();
    assert!(y >= 0.0);
    assert!(
        y + line.h + 2.0 * rndr_core::TEXT_ORIGIN_Y <= rect.h,
        "caret at {y} not inside height {}",
        rect.h
    );
}

#[test]
fn vertical_goal_survives_short_lines_but_horizontal_motion_resets_it() {
    let (mut app, e) = boot("abcdef\nx\nabcdef");
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 8);
    app.update(); // idle does not lose the desired x
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 6);
    press(&mut app, Key::Left, false);
    press(&mut app, Key::Down, false);
    press(&mut app, Key::Down, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 14);
}

#[test]
fn shift_extends_selection_and_unshifted_arrows_collapse_directionally() {
    let (mut app, e) = boot("hello\nworld");
    press(&mut app, Key::Up, true);
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!((b.caret, b.anchor), (5, 11));
    press(&mut app, Key::Down, false);
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!((b.caret, b.anchor), (11, 11));
    position(&mut app, e, 11, 5);
    press(&mut app, Key::Up, false);
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!((b.caret, b.anchor), (5, 5));
}

#[test]
fn empty_blank_trailing_newline_and_document_edges_are_stable() {
    for text in ["", "\n", "hello\n", "\n\n", "x"] {
        let (mut app, e) = boot(text);
        press(&mut app, Key::Down, false);
        assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, text.len());
        for _ in 0..4 {
            press(&mut app, Key::Up, false);
        }
        let b = app.world().get::<Buffer>(e).unwrap();
        assert!(b.caret <= text.len());
        assert!(text.is_char_boundary(b.caret));
        assert_visible(&app, e);
    }
    let (mut app, e) = boot("ab\n\ncd\n");
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 4);
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 3);
}

#[test]
fn soft_wrapped_rows_navigate_and_reveal_without_newlines() {
    let text = "a".repeat(100);
    let (mut app, e) = boot(&text);
    let layout = rndr_core::layout_of(app.world(), e, &text);
    assert!(layout.lines.len() > 4);
    let (x, y) = layout.caret_xy(text.len());
    let row = layout.lines.iter().position(|line| line.y == y).unwrap();
    let expected = layout.index_on_line(row - 1, x);
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, expected);
    assert_visible(&app, e);
    press(&mut app, Key::Down, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, text.len());
    assert_visible(&app, e);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), text);
}

#[test]
fn up_at_hard_wrap_start_must_reach_the_previous_visual_row() {
    let (mut app, e) = boot(&"a".repeat(100));
    let text = app.world().get::<Buffer>(e).unwrap().text();
    let layout = rndr_core::layout_of(app.world(), e, &text);
    let start = layout.lines[2].start;
    position(&mut app, e, start, start);
    press(&mut app, Key::Up, false);
    assert_eq!(
        app.world().get::<Buffer>(e).unwrap().caret,
        layout.lines[1].start
    );
}

#[test]
fn unicode_indices_are_clamped_and_never_split_graphemes_during_navigation() {
    use unicode_segmentation::UnicodeSegmentation;
    let text = "a🇺🇸e\u{301}\n漢字\n🙂x";
    let boundaries: Vec<usize> = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect();
    let (mut app, e) = boot(text);
    for index in 0..=text.len() + 3 {
        position(&mut app, e, index, index.saturating_add(1));
        for key in [Key::Up, Key::Down] {
            press(&mut app, key, true);
            let b = app.world().get::<Buffer>(e).unwrap();
            assert!(boundaries.contains(&b.caret), "caret {}", b.caret);
            assert!(boundaries.contains(&b.anchor), "anchor {}", b.anchor);
            assert_eq!(b.text(), text);
        }
    }
}

#[test]
fn queued_native_events_preserve_order_shift_repeat_and_single_consumption() {
    let (mut app, e) = boot("hello\nworld");
    let mut shift = Modifiers::new();
    shift.shift = true;
    let mut repeated = KeyEvent::named(NamedKey::ArrowUp, shift);
    repeated.repeat = true;
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys = vec![
        repeated,
        KeyEvent::character('X', Modifiers::new()),
        KeyEvent::named(NamedKey::ArrowDown, Modifiers::new()),
    ];
    input.chars = "must not duplicate queued text".into();
    app.update();
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!(b.text(), "helloX");
    assert_eq!((b.caret, b.anchor), (6, 6));
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), "helloX");
}

#[test]
fn queued_release_synthetic_and_alt_arrows_do_not_navigate() {
    let (mut app, e) = boot("hello\nworld");
    let mut released = KeyEvent::named(NamedKey::ArrowUp, Modifiers::new());
    released.released = true;
    let mut synthetic = KeyEvent::named(NamedKey::ArrowUp, Modifiers::new());
    synthetic.synthetic = true;
    let mut alt = Modifiers::new();
    alt.alt = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![released, synthetic, KeyEvent::named(NamedKey::ArrowUp, alt)];
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 11);
}

#[test]
fn a_scalar_wrap_inside_a_grapheme_never_publishes_an_out_of_window_caret() {
    use rndr_core::{Rect, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
    let text = "e\u{301}x";
    let (mut app, e) = boot(text);
    app.world_mut().get_mut::<Layout>(e).unwrap().width = Size::Px(40.0);
    app.update();
    let full = rndr_core::layout_of(app.world(), e, text);
    assert_eq!(
        full.lines[1].start, 1,
        "exercise the bitmap scalar-wrap edge"
    );
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = full.lines[1].y;
    app.update();
    let rect = *app.world().get::<Rect>(e).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + TEXT_ORIGIN_X + 1.0, rect.y + TEXT_ORIGIN_Y + 1.0];
    input.pointer.pressed = true;
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 0);
    assert_visible(&app, e);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .pressed = false;
    press(&mut app, Key::Down, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 3);
    assert_visible(&app, e);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), text);
}

#[test]
fn queued_typing_and_ime_commit_reveal_eof() {
    for ime in [false, true] {
        let (mut app, e) = boot(&"line\n".repeat(100));
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.keys.push(KeyEvent::character('é', Modifiers::new()));
        if ime {
            input.ime_enabled = true;
            input.ime_commit = "漢".into();
        }
        app.update();
        let b = app.world().get::<Buffer>(e).unwrap();
        assert!(b.text().ends_with(if ime { "\n漢" } else { "\né" }));
        assert_visible(&app, e);
    }
}

#[test]
fn idle_scroll_is_not_overridden_and_top_navigation_reveals_again() {
    let (mut app, e) = boot(&"line\n".repeat(100));
    app.world_mut().resource_mut::<Input>().unwrap().chars = "x".into();
    app.update();
    assert_visible(&app, e);
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = 0.0;
    app.update();
    assert!(app.world().get::<Caret>(e).is_none());
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    position(&mut app, e, 0, 0);
    press(&mut app, Key::Down, false);
    assert_visible(&app, e);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 5);
}

#[test]
fn scrolled_wrapped_pointer_maps_to_document_bytes_and_decorations() {
    use rndr_core::{
        Decoration, DecorationKind, DecorationMarks, Decorations, Rect, TEXT_ORIGIN_X,
        TEXT_ORIGIN_Y,
    };
    let text = "word ".repeat(100);
    let (mut app, e) = boot(&text);
    let layout = rndr_core::layout_of(app.world(), e, &text);
    let row = &layout.lines[5];
    let base = row.start;
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = row.y;
    app.world_mut().insert(
        e,
        Decorations {
            list: vec![Decoration {
                start: base,
                end: base + 4,
                kind: DecorationKind::Find,
            }],
        },
    );
    app.update();
    assert_eq!(
        app.world().get::<Label>(e).unwrap().text.chars().next(),
        Some('w')
    );
    let rect = *app.world().get::<Rect>(e).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + TEXT_ORIGIN_X + 1.0, rect.y + TEXT_ORIGIN_Y + 1.0];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, base);
    let marks = app.world().get::<DecorationMarks>(e).unwrap();
    assert_eq!(marks.marks.len(), 1);
    assert_eq!(marks.marks[0].y, 0.0);
    assert_visible(&app, e);
}

#[test]
fn right_edge_alias_does_not_trap_up_on_the_same_visual_row() {
    let (mut app, e) = boot(&"a".repeat(32));
    let text = app.world().get::<Buffer>(e).unwrap().text();
    let layout = rndr_core::layout_of(app.world(), e, &text);
    assert_eq!(layout.lines.len(), 2);
    press(&mut app, Key::Up, false);
    let index = app.world().get::<Buffer>(e).unwrap().caret;
    assert_eq!(index, layout.lines[1].start);
    let hint = app.world().get::<rndr_core::CaretRow>(e).unwrap();
    assert_eq!(hint.row, 0);
    let drawn = app.world().get::<rndr_core::TextLayout>(e).unwrap();
    let (x, y) = drawn.caret_xy_on_line(hint.index, hint.row);
    assert_eq!(y, 0.0);
    assert_eq!(x, layout.lines[0].width);
    assert_visible(&app, e);
    press(&mut app, Key::Down, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 32);
}

#[test]
fn resizing_and_entity_line_height_keep_mapping_and_reveal_consistent() {
    let text = "a".repeat(100);
    let (mut app, e) = boot(&text);
    app.world_mut().insert(
        e,
        rndr_core::TextStyle {
            size: 12.0,
            line_height: Some(1.5),
            ..rndr_core::TextStyle::default()
        },
    );
    app.world_mut().get_mut::<Layout>(e).unwrap().width = Size::Px(100.0);
    app.update();
    let full = rndr_core::layout_of(app.world(), e, &text);
    let (x, y) = full.caret_xy(text.len());
    let row = full.lines.iter().position(|line| line.y == y).unwrap();
    let expected = full.index_on_line(row - 1, x);
    press(&mut app, Key::Up, false);
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, expected);
    assert_visible(&app, e);
    let drawn = app.world().get::<rndr_core::TextLayout>(e).unwrap();
    assert_eq!(drawn.lines[0].h, full.lines[0].h);
}

#[test]
fn legacy_typing_at_a_split_utf8_index_repairs_bounds_without_corrupting_text() {
    let (mut app, e) = boot("é🙂");
    position(&mut app, e, 1, 1);
    app.world_mut().resource_mut::<Input>().unwrap().chars = "X".into();
    app.update();
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!(b.text(), "Xé🙂");
    assert_eq!((b.caret, b.anchor), (1, 1));
    assert_visible(&app, e);
}

#[test]
fn unrelated_release_and_synthetic_input_do_not_cancel_explicit_scrolling() {
    let (mut app, e) = boot(&"line\n".repeat(100));
    let mut synthetic = KeyEvent::named(NamedKey::ArrowUp, Modifiers::new());
    synthetic.synthetic = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(synthetic);
    app.update();
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
    assert!(app.world().get::<Caret>(e).is_none());
    press(&mut app, Key::Escape, false);
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
}

#[test]
fn composition_owns_vertical_keys_until_commit() {
    let (mut app, e) = boot("hello\nworld");
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit = "漢".into();
    input
        .keys
        .push(KeyEvent::named(NamedKey::ArrowUp, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Buffer>(e).unwrap().caret, 11);
}
