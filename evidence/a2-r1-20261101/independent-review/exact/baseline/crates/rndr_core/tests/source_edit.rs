use rndr_core::{
    App, Buffer, CapturesKeys, Chrome, Editor, Focus, Input, KeyEvent, Label, Modifiers, Platform,
};

fn boot() -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Platform::Windows);
    let entity = app.world_mut().spawn((
        Buffer::from_str("- [ ] task"),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity)
}

fn press(app: &mut App, key: char) {
    let mut modifiers = Modifiers::new();
    modifiers.ctrl = true;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(key, modifiers));
    app.update();
}

fn text(app: &App, entity: rndr_core::Entity) -> String {
    app.world().get::<Buffer>(entity).unwrap().text()
}

fn type_char(app: &mut App, ch: char) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(ch, Modifiers::new()));
    app.update();
}

#[test]
fn rejected_and_identical_edits_preserve_selection_and_redo() {
    use rndr_core::{edit_buffer, BufferEditError};
    let (mut app, entity) = boot();
    type_char(&mut app, '!');
    press(&mut app, 'z');
    let b = app.world_mut().get_mut::<Buffer>(entity).unwrap();
    b.caret = 6;
    b.anchor = 2;
    let reversed = std::ops::Range { start: 4, end: 3 };
    for range in [reversed, 0..100, usize::MAX..usize::MAX] {
        assert_eq!(
            edit_buffer(app.world_mut(), entity, range, "x"),
            Err(BufferEditError::InvalidRange)
        );
    }
    assert_eq!(edit_buffer(app.world_mut(), entity, 3..4, " "), Ok(false));
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor), (6, 2));
    assert_eq!(b.text(), "- [ ] task");
    press(&mut app, 'y');
    assert_eq!(text(&app, entity), "- [ ] task!");
    let missing = app.world_mut().spawn(Label::new("not a buffer"));
    assert_eq!(
        edit_buffer(app.world_mut(), missing, 0..0, "x"),
        Err(BufferEditError::MissingBuffer)
    );
}

#[test]
fn unicode_bytes_and_reversed_selection_round_trip_exactly() {
    use rndr_core::edit_buffer;
    let (mut app, entity) = boot();
    let source = "\u{feff}---\r\nkey: untouched\r\n---\r\n- [ ] é مرحبا 👩‍💻\r\n```unknown\r\nopaque\r\n```\r\n";
    let mut b = Buffer::from_str(source);
    let start = source.find('é').unwrap();
    b.caret = start;
    b.anchor = source.len();
    app.world_mut().insert(entity, b);
    assert_eq!(
        edit_buffer(app.world_mut(), entity, start + 1..start + 2, "x"),
        Err(rndr_core::BufferEditError::InvalidRange)
    );
    let task = source.find("[ ]").unwrap() + 1;
    assert_eq!(
        edit_buffer(app.world_mut(), entity, task..task + 1, "✓"),
        Ok(true)
    );
    let next = source.replacen("[ ]", "[✓]", 1);
    assert_eq!(text(&app, entity), next);
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor), (start + 2, source.len() + 2));
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), source);
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor), (start, source.len()));
    press(&mut app, 'y');
    assert_eq!(text(&app, entity), next);
}

#[test]
fn selection_mapping_insertion_deletion_and_grapheme_join() {
    use rndr_core::edit_buffer;
    let (mut app, entity) = boot();
    let mut b = Buffer::from_str("abcdef");
    b.caret = 1;
    b.anchor = 5;
    app.world_mut().insert(entity, b);
    edit_buffer(app.world_mut(), entity, 2..4, "Z").unwrap();
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor, b.text()), (1, 4, "abZef".into()));
    edit_buffer(app.world_mut(), entity, 1..4, "").unwrap();
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor, b.text()), (1, 1, "af".into()));
    edit_buffer(app.world_mut(), entity, 1..1, "é").unwrap();
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.caret, b.anchor, b.text()), (3, 3, "aéf".into()));
    // Inserting before a combining mark can create a new grapheme around the
    // replacement boundary. Endpoints must not split that resulting cluster.
    app.world_mut().insert(entity, Buffer::from_str("\u{301}"));
    app.world_mut().get_mut::<Buffer>(entity).unwrap().caret = 0;
    app.world_mut().get_mut::<Buffer>(entity).unwrap().anchor = 0;
    edit_buffer(app.world_mut(), entity, 0..0, "e").unwrap();
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!(b.text(), "e\u{301}");
    assert_eq!((b.caret, b.anchor), (0, 0));
}

#[test]
fn source_operations_break_typing_runs_and_invalidate_redo() {
    use rndr_core::edit_buffer;
    let (mut app, entity) = boot();
    type_char(&mut app, 'a');
    edit_buffer(app.world_mut(), entity, 3..4, "x").unwrap();
    type_char(&mut app, 'b');
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), "- [x] taska");
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), "- [ ] taska");
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), "- [ ] task");
    edit_buffer(app.world_mut(), entity, 3..4, "y").unwrap();
    press(&mut app, 'y');
    assert_eq!(text(&app, entity), "- [y] task");
}

#[test]
fn composition_refuses_edit_without_consuming_commit_or_clipboard() {
    use rndr_core::{edit_buffer, BufferEditError, Clipboard};
    let (mut app, entity) = boot();
    app.world_mut().insert_resource(Clipboard {
        text: "clipboard".into(),
        ..Default::default()
    });
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    assert_eq!(
        edit_buffer(app.world_mut(), entity, 3..4, "x"),
        Err(BufferEditError::Composing)
    );
    app.update();
    assert_eq!(
        edit_buffer(app.world_mut(), entity, 3..4, "x"),
        Err(BufferEditError::Composing)
    );
    assert_eq!(text(&app, entity), "- [ ] task");
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "日本".into();
    app.update();
    assert_eq!(text(&app, entity), "- [ ] task日本");
    edit_buffer(app.world_mut(), entity, 3..4, "x").unwrap();
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), "- [ ] task日本");
    press(&mut app, 'z');
    assert_eq!(text(&app, entity), "- [ ] task");
    assert_eq!(
        app.world().resource::<Clipboard>().unwrap().text,
        "clipboard"
    );
}

#[test]
fn edits_to_other_note_do_not_steal_focus_or_consume_input() {
    use rndr_core::edit_buffer;
    let (mut app, active) = boot();
    let other =
        app.world_mut()
            .spawn((Buffer::from_str("other"), Editor::default(), Label::new("")));
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('!', Modifiers::new()));
    edit_buffer(app.world_mut(), other, 0..5, "changed").unwrap();
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(active)
    );
    assert_eq!(app.world().resource::<Input>().unwrap().keys.len(), 1);
    app.update();
    assert_eq!(text(&app, active), "- [ ] task!");
    assert_eq!(text(&app, other), "changed");
    press(&mut app, 'z');
    assert_eq!(text(&app, active), "- [ ] task");
    assert_eq!(text(&app, other), "changed");
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(other);
    press(&mut app, 'z');
    assert_eq!(text(&app, other), "other");
}

#[test]
fn ime_commit_replaces_selection_and_is_a_separate_step_with_or_without_keys() {
    for queued in [false, true] {
        let (mut app, entity) = boot();
        type_char(&mut app, 'a');
        let buffer = app.world_mut().get_mut::<Buffer>(entity).unwrap();
        buffer.caret = 6;
        buffer.anchor = 10;
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_enabled = true;
        input.ime_commit = "日本".into();
        if queued {
            input.keys.push(KeyEvent::character('本', Modifiers::new()));
        }
        app.update();
        assert_eq!(text(&app, entity), "- [ ] 日本a", "queued={queued}");
        press(&mut app, 'z');
        assert_eq!(text(&app, entity), "- [ ] taska");
        let buffer = app.world().get::<Buffer>(entity).unwrap();
        assert_eq!((buffer.caret, buffer.anchor), (6, 10));
        press(&mut app, 'z');
        assert_eq!(text(&app, entity), "- [ ] task");
        press(&mut app, 'y');
        press(&mut app, 'y');
        assert_eq!(text(&app, entity), "- [ ] 日本a");
    }
}

#[test]
fn source_range_edit_shares_keyboard_undo() {
    let (mut app, entity) = boot();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character('!', Modifiers::new()));
    app.update();
    assert_eq!(
        app.world().get::<Buffer>(entity).unwrap().text(),
        "- [ ] task!"
    );
    assert_eq!(
        rndr_core::edit_buffer(app.world_mut(), entity, 3..4, "x"),
        Ok(true)
    );
    assert_eq!(
        app.world().get::<Buffer>(entity).unwrap().text(),
        "- [x] task!"
    );
    press(&mut app, 'z');
    assert_eq!(
        app.world().get::<Buffer>(entity).unwrap().text(),
        "- [ ] task!",
        "undo the source operation before unrelated typing"
    );
    press(&mut app, 'z');
    assert_eq!(
        app.world().get::<Buffer>(entity).unwrap().text(),
        "- [ ] task"
    );
    press(&mut app, 'y');
    press(&mut app, 'y');
    assert_eq!(
        app.world().get::<Buffer>(entity).unwrap().text(),
        "- [x] task!"
    );
}
