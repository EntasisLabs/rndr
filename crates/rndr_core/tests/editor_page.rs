use rndr_core::editing::{attach_source, DocumentId, Revision, SourceDocuments};
use rndr_core::{
    App, Axis, Buffer, CapturesKeys, Chrome, Editor, Focus, Input, KeyEvent, Label, Layout,
    Modifiers, NamedKey, Platform, Size, Viewport,
};

fn boot(text: &str, managed: bool) -> (App, rndr_core::Entity, Option<DocumentId>) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 120.0));
    app.world_mut().insert_resource(Platform::Windows);
    app.world_mut().insert_resource(Focus::default());
    let e = app.world_mut().spawn((
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
    let id = managed.then(|| {
        let mut docs = SourceDocuments::default();
        let id = docs.open(text);
        app.world_mut().insert_resource(docs);
        attach_source(app.world_mut(), e, id).unwrap();
        id
    });
    let b = app.world_mut().get_mut::<Buffer>(e).unwrap();
    b.caret = 3;
    b.anchor = 3;
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
    app.update();
    app.update();
    (app, e, id)
}
fn send(app: &mut App, event: KeyEvent) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(event);
    app.update();
}
fn pos(app: &App, e: rndr_core::Entity) -> (usize, usize) {
    let b = app.world().get::<Buffer>(e).unwrap();
    (b.caret, b.anchor)
}
#[test]
fn page_motion_uses_viewport_preserves_x_and_extends_directed_selection() {
    for managed in [false, true] {
        let text = "abcdefghij\n".repeat(30);
        let (mut app, e, id) = boot(&text, managed);
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
        );
        let (head, anchor) = pos(&app, e);
        assert!(head > 14, "must advance more than one row: {head}");
        assert_eq!(head % 11, 3);
        assert_eq!(anchor, head);
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageUp, Modifiers::new()),
        );
        assert_eq!(pos(&app, e), (3, 3));
        let mut shift = Modifiers::new();
        shift.shift = true;
        send(&mut app, KeyEvent::named(NamedKey::PageDown, shift));
        assert_eq!(pos(&app, e), (head, 3));
        send(&mut app, KeyEvent::named(NamedKey::PageUp, shift));
        assert_eq!(pos(&app, e), (3, 3));
        send(&mut app, KeyEvent::named(NamedKey::PageUp, shift));
        assert_eq!(pos(&app, e), (3, 3)); // retains x at the first row
        if let Some(id) = id {
            let d = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap();
            assert_eq!(d.revision(), Revision(0));
            assert_eq!(d.drain_events().count(), 0);
            assert!(d.undo().unwrap().is_none());
        }
    }
}
#[test]
fn page_motion_uses_soft_rows_and_recomputes_after_resize() {
    for managed in [false, true] {
        let text = "abcdefghij ".repeat(200);
        let (mut app, e, _) = boot(&text, managed);
        let start = pos(&app, e).0;
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
        );
        let first_page = pos(&app, e).0;
        assert!(first_page > start);
        assert!(!text[..first_page].contains('\n'));
        assert!(app.world().get::<rndr_core::Caret>(e).is_some());
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageUp, Modifiers::new()),
        );
        assert_eq!(pos(&app, e), (start, start));
        app.world_mut().get_mut::<Layout>(e).unwrap().height = Size::Px(40.0);
        app.update();
        app.update();
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
        );
        assert!(pos(&app, e).0 > start);
        assert!(pos(&app, e).0 < first_page);
        assert!(app.world().get::<rndr_core::Caret>(e).is_some());
    }
}

#[test]
fn page_release_synthetic_and_modified_keys_do_not_move_or_edit() {
    for managed in [false, true] {
        let (mut app, e, _) = boot(&"abcdefghij\n".repeat(30), managed);
        let key = KeyEvent::named(NamedKey::PageDown, Modifiers::new());
        send(&mut app, key.clone().released());
        send(&mut app, key.synthetic());
        let mut ctrl = Modifiers::new();
        ctrl.ctrl = true;
        send(&mut app, KeyEvent::named(NamedKey::PageDown, ctrl));
        assert_eq!(pos(&app, e), (3, 3));
        send(
            &mut app,
            KeyEvent::named(NamedKey::PageDown, Modifiers::new()).repeating(),
        );
        assert!(pos(&app, e).0 > 14);
    }
}
#[test]
fn page_navigation_readonly_unicode_and_composition_admission() {
    let text = "a\u{301}👩‍💻🇺🇸 العربية\r\n".repeat(30);
    let (mut app, e, id) = boot(&text, true);
    let id = id.unwrap();
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true)
        .unwrap();
    send(
        &mut app,
        KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
    );
    let (head, _) = pos(&app, e);
    assert!(head > 3);
    use unicode_segmentation::UnicodeSegmentation;
    assert!(text.grapheme_indices(true).any(|(i, _)| i == head));
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(false)
        .unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    let before = pos(&app, e);
    send(
        &mut app,
        KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
    );
    assert_eq!(pos(&app, e), before);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.update();
    send(
        &mut app,
        KeyEvent::named(NamedKey::PageDown, Modifiers::new()),
    );
    assert!(pos(&app, e).0 > before.0);
    let d = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(d.text(), text);
    assert_eq!(d.revision(), Revision(0));
}
