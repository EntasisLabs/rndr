use super::*;
use crate::structured::{Marks, StructuredError};
fn setup(text: &str) -> (World, Entity, StructuredHandle) {
    let mut world = World::new();
    world.insert_resource(Input::default());
    world.insert_resource(Platform::Mac);
    world.insert_resource(Clipboard::default());
    let mut docs = StructuredDocuments::default();
    let id = docs.insert(StructuredDocument::new(text));
    world.insert_resource(docs);
    let entity = world.spawn((
        Layout {
            width: Size::Px(240.0),
            height: Size::Px(220.0),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        Focusable,
        CapturesKeys,
    ));
    world.insert(
        entity,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 240.0,
            h: 220.0,
        },
    );
    world.insert_resource(Focus {
        entity: Some(entity),
    });
    attach_structured(&mut world, entity, id).unwrap();
    (world, entity, id)
}
fn key(world: &mut World, entity: Entity, named: NamedKey) {
    world.resource_mut::<Input>().unwrap().keys = vec![KeyEvent::named(named, Modifiers::new())];
    crate::editor::apply_editor(world);
    assert!(!world.has::<crate::typing::EditHistory>(entity));
}
fn type_text(world: &mut World, text: &str) {
    world.resource_mut::<Input>().unwrap().chars = text.into();
    crate::editor::apply_editor(world);
}
fn select(world: &mut World, entity: Entity, a: usize, h: usize) {
    let r = world.get::<StructuredBinding>(entity).unwrap().revision;
    set_display_selection(world, entity, r, a, h).unwrap();
}
#[test]
fn native_structured_typing_split_join_and_document_undo() {
    let (mut w, e, id) = setup("ab");
    select(&mut w, e, 1, 1);
    key(&mut w, e, NamedKey::Enter);
    assert_eq!(document(&w, e).unwrap().plain_text(), "a\nb");
    assert_eq!(document(&w, e).unwrap().blocks().len(), 2);
    key(&mut w, e, NamedKey::Backspace);
    assert_eq!(document(&w, e).unwrap().plain_text(), "ab");
    w.resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .undo()
        .unwrap();
    project(&mut w, e);
    assert_eq!(w.get::<Buffer>(e).unwrap().text(), "a\nb");
    detach_structured(&mut w, e).unwrap();
    assert_eq!(
        w.resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .blocks()
            .len(),
        2
    );
}
#[test]
fn native_structured_paste_across_mark_boundary_and_one_undo() {
    let (mut w, e, _) = setup("abcdef");
    select(&mut w, e, 0, 3);
    document_mut(&mut w, e)
        .unwrap()
        .set_marks(
            Marks {
                strong: true,
                ..Default::default()
            },
            EditOrigin::Host(1),
        )
        .unwrap();
    project(&mut w, e);
    select(&mut w, e, 2, 5);
    w.resource_mut::<Clipboard>().unwrap().text = "é\nمرحبا".into();
    w.resource_mut::<Input>().unwrap().key = Some(Key::Paste);
    crate::editor::apply_editor(&mut w);
    assert_eq!(document(&w, e).unwrap().plain_text(), "abé\nمرحباf");
    assert!(document(&w, e).unwrap().blocks()[1].runs[0].marks.strong);
    document_mut(&mut w, e).unwrap().undo().unwrap();
    assert_eq!(document(&w, e).unwrap().plain_text(), "abcdef");
}
#[test]
fn native_structured_marked_ime_is_transient_and_commits_once() {
    let (mut w, e, _) = setup("abcdef");
    select(&mut w, e, 0, 3);
    document_mut(&mut w, e)
        .unwrap()
        .set_marks(
            Marks {
                strong: true,
                ..Default::default()
            },
            EditOrigin::Host(1),
        )
        .unwrap();
    project(&mut w, e);
    select(&mut w, e, 2, 5);
    let before = document(&w, e).unwrap().revision();
    w.resource_mut::<Input>().unwrap().ime_preedit = "仮".into();
    crate::editor::apply_editor(&mut w);
    assert!(is_composing(&w, e));
    assert_eq!(document(&w, e).unwrap().plain_text(), "abcdef");
    assert_eq!(document(&w, e).unwrap().revision(), before);
    assert!(matches!(
        command_ready(&w, e),
        Err(StructuredViewError::Composing)
    ));
    assert_eq!(
        document_mut(&mut w, e)
            .unwrap()
            .replace_selection("no", EditOrigin::Host(2)),
        Err(StructuredError::CompositionActive)
    );
    let input = w.resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "漢".into();
    input.chars = "漢".into();
    input.ime_enabled = true;
    crate::editor::apply_editor(&mut w);
    assert_eq!(document(&w, e).unwrap().plain_text(), "ab漢f");
    assert!(!is_composing(&w, e));
    assert!(document(&w, e).unwrap().blocks()[0].runs[0].marks.strong);
    document_mut(&mut w, e).unwrap().undo().unwrap();
    assert_eq!(document(&w, e).unwrap().plain_text(), "abcdef");
}
#[test]
fn native_structured_readonly_preserves_copy_and_navigation_all_origins() {
    let (mut w, e, _) = setup("abc");
    document_mut(&mut w, e).unwrap().set_readonly(true);
    select(&mut w, e, 0, 3);
    type_text(&mut w, "x");
    assert_eq!(document(&w, e).unwrap().plain_text(), "abc");
    w.resource_mut::<Input>().unwrap().key = Some(Key::Copy);
    crate::editor::apply_editor(&mut w);
    assert_eq!(w.resource::<Clipboard>().unwrap().text, "abc");
    key(&mut w, e, NamedKey::ArrowLeft);
    assert_eq!(w.get::<Buffer>(e).unwrap().caret, 0);
    assert!(crate::access_details(&w, e).readonly);
    assert_eq!(
        document_mut(&mut w, e)
            .unwrap()
            .replace_selection("x", EditOrigin::Ime),
        Err(StructuredError::Readonly)
    );
}
#[test]
fn native_structured_view_readonly_is_not_overwritten_by_projection() {
    let (mut w, e, _) = setup("abc");
    w.insert(
        e,
        Semantics {
            readonly: true,
            ..Default::default()
        },
    );
    project(&mut w, e);
    type_text(&mut w, "x");
    assert_eq!(document(&w, e).unwrap().plain_text(), "abc");
    assert!(w.get::<Semantics>(e).unwrap().readonly);
}
#[test]
fn native_structured_access_state_refuses_edit_undo_aba() {
    let (mut w, e, _) = setup("abc");
    crate::editor::apply_editor(&mut w);
    let snapshot = crate::access_text(&mut w, e).unwrap();
    type_text(&mut w, "x");
    document_mut(&mut w, e).unwrap().undo().unwrap();
    project(&mut w, e);
    assert_eq!(document(&w, e).unwrap().plain_text(), "abc");
    assert_eq!(
        crate::access_text::set_selection(
            &mut w,
            e,
            snapshot.state,
            crate::editing::SourceSelection { anchor: 0, head: 2 }
        ),
        crate::AccessCommandResult::Rejected
    );
}
#[test]
fn native_structured_geometry_and_block_mapping_share_exact_projection() {
    let (mut w, e, _) = setup("é مرحبا 👩‍💻\nnext");
    crate::editor::apply_editor(&mut w);
    let display = structured_display(&w, e).unwrap();
    assert_eq!(display.blocks.len(), 2);
    let snapshot = crate::access_text(&mut w, e).unwrap();
    assert!(snapshot.caret_bounds().h > 0.0);
    select(&mut w, e, 0, 3);
    crate::editor::apply_editor(&mut w);
    let snapshot = crate::access_text(&mut w, e).unwrap();
    assert!(!snapshot.selection_bounds().is_empty());
    let base = display.revision;
    document_mut(&mut w, e)
        .unwrap()
        .set_block_kind(BlockKind::Heading(2), EditOrigin::Host(2))
        .unwrap();
    assert!(structured_display(&w, e).is_none());
    assert!(set_display_selection(&mut w, e, base, 0, 0).is_err());
}
#[test]
fn native_structured_interactive_grapheme_snap_and_raw_buffer_guard() {
    let (mut w, e, _) = setup("👩‍💻x");
    select(&mut w, e, 4, 4);
    assert_eq!(w.get::<Buffer>(e).unwrap().caret, 0);
    assert_eq!(
        crate::edit_buffer(&mut w, e, 0..0, "x"),
        Err(crate::BufferEditError::ManagedDocument)
    );
    w.get_mut::<Buffer>(e).unwrap().insert_str(0, "BAD");
    project(&mut w, e);
    assert_eq!(w.get::<Buffer>(e).unwrap().text(), "👩‍💻x");
}
#[test]
fn native_structured_node_delete_and_undo_restore_node_identity() {
    let (mut w, e, _) = setup("one\ntwo");
    let id = document(&w, e).unwrap().blocks()[0].id;
    document_mut(&mut w, e)
        .unwrap()
        .set_selection(StructuredSelection::Node(id))
        .unwrap();
    project(&mut w, e);
    key(&mut w, e, NamedKey::Delete);
    assert_eq!(document(&w, e).unwrap().plain_text(), "two");
    document_mut(&mut w, e).unwrap().undo().unwrap();
    assert_eq!(
        document(&w, e).unwrap().selection(),
        StructuredSelection::Node(id)
    );
}
#[test]
fn native_structured_composition_survives_toolbar_focus_until_native_cancel() {
    let (mut w, e, _) = setup("abc");
    w.resource_mut::<Input>().unwrap().ime_preedit = "x".into();
    crate::editor::apply_editor(&mut w);
    w.insert_resource(Focus { entity: None });
    crate::editor::apply_editor(&mut w);
    assert!(is_composing(&w, e));
    assert_eq!(command_ready(&w, e), Err(StructuredViewError::Composing));
    w.resource_mut::<Input>().unwrap().ime_preedit.clear();
    crate::editor::apply_editor(&mut w);
    assert!(!is_composing(&w, e));
}
#[test]
fn native_structured_plain_crlf_clipboard_normalizes_before_offset_mapping() {
    let (mut w, e, _) = setup("a\r\nb");
    assert_eq!(document(&w, e).unwrap().plain_text(), "a\nb");
    select(&mut w, e, 0, 1);
    w.resource_mut::<Clipboard>().unwrap().text = "é\r\nx\ry".into();
    w.resource_mut::<Input>().unwrap().key = Some(Key::Paste);
    crate::editor::apply_editor(&mut w);
    assert_eq!(document(&w, e).unwrap().plain_text(), "é\nx\ny\nb");
    assert!(!w.has::<StructuredViewError>(e));
    assert_eq!(w.get::<Buffer>(e).unwrap().caret, 6);
}
#[test]
fn native_structured_accessible_block_navigation_maps_to_typed_node() {
    let (mut w, e, _) = setup("one\ntwo");
    crate::editor::apply_editor(&mut w);
    let display = structured_display(&w, e).unwrap();
    let snapshot = crate::access_text(&mut w, e).unwrap();
    assert_eq!(
        crate::access_text::set_selection(
            &mut w,
            e,
            snapshot.state,
            crate::editing::SourceSelection {
                anchor: display.blocks[1].range.start,
                head: display.blocks[1].range.end
            }
        ),
        crate::AccessCommandResult::Applied
    );
    let StructuredSelection::Text(selection) = document(&w, e).unwrap().selection() else {
        panic!("text selection")
    };
    assert_eq!(selection.head.block, display.blocks[1].block);
    assert_eq!(selection.anchor.offset, 0);
    assert_eq!(selection.head.offset, 3);
}
#[test]
fn native_structured_canceled_model_composition_cannot_retarget_late_commit() {
    let (mut w, e, _) = setup("abc");
    select(&mut w, e, 0, 1);
    w.resource_mut::<Input>().unwrap().ime_preedit = "仮".into();
    crate::editor::apply_editor(&mut w);
    let doc = document_mut(&mut w, e).unwrap();
    doc.set_readonly(true);
    doc.set_readonly(false);
    let p = doc.position_at(3, Affinity::After).unwrap();
    doc.set_selection(StructuredSelection::Text(TextSelection {
        anchor: p,
        head: p,
    }))
    .unwrap();
    crate::editor::apply_editor(&mut w);
    let input = w.resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "漢".into();
    crate::editor::apply_editor(&mut w);
    assert_eq!(document(&w, e).unwrap().plain_text(), "abc");
    assert_eq!(
        w.get::<StructuredViewError>(e),
        Some(&StructuredViewError::Model(StructuredError::StaleRevision))
    );
}
#[test]
fn native_structured_whole_document_replacement_is_not_identity_aba() {
    let (mut w, e, id) = setup("abc");
    crate::editor::apply_editor(&mut w);
    let snapshot = crate::access_text(&mut w, e).unwrap();
    *w.resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap() = StructuredDocument::new("abc");
    assert!(structured_display(&w, e).is_none());
    assert!(attached_structured(&w, e).is_none());
    assert_eq!(
        crate::access_text::set_selection(
            &mut w,
            e,
            snapshot.state,
            crate::editing::SourceSelection { anchor: 0, head: 2 }
        ),
        crate::AccessCommandResult::Rejected
    );
}
#[test]
fn native_structured_orphan_cleanup_releases_history_for_remount() {
    let (mut w, e, id) = setup("abc");
    w.despawn_entity(e);
    crate::editor::apply_editor(&mut w);
    assert!(w
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .remove(id)
        .is_some());
}
