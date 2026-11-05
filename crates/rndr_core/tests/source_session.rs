use rndr_core::editing::*;
use rndr_core::{
    App, Buffer, CapturesKeys, Chrome, Editor, Focus, Input, KeyEvent, Label, Modifiers, Platform,
};

fn batch(id: DocumentId, base: Revision, changes: Vec<SourceChange>) -> SourceBatch {
    SourceBatch {
        document: id,
        base,
        origin: EditOrigin::Host(7),
        changes,
    }
}
fn change(range: std::ops::Range<usize>, replacement: &str) -> SourceChange {
    SourceChange {
        range,
        replacement: replacement.into(),
    }
}

#[test]
fn atomic_validation_noop_and_identity_preserve_redo_and_selection() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("aébc");
    let wrong = docs.open("aébc");
    let d = docs.get_mut(id).unwrap();
    d.set_selection(SourceSelection { anchor: 5, head: 1 })
        .unwrap();
    let first = d
        .commit(batch(id, Revision(0), vec![change(3..4, "X")]))
        .unwrap()
        .unwrap();
    assert_eq!(first.before, Revision(0));
    assert_eq!(first.after, Revision(1));
    d.undo().unwrap();
    let selection = d.selection();
    let base = d.revision();
    for (b, expected) in [
        (batch(wrong, base, vec![]), EditError::MissingDocument),
        (batch(id, Revision(0), vec![]), EditError::StaleRevision),
        (
            batch(id, base, vec![change(0..1, "!"), change(2..3, "bad")]),
            EditError::InvalidRange,
        ),
        (
            batch(id, base, vec![change(0..3, "!"), change(1..4, "bad")]),
            EditError::Overlap,
        ),
        (
            batch(id, base, vec![change(0..0, "!"), change(0..1, "bad")]),
            EditError::Overlap,
        ),
        (
            batch(id, base, vec![change(4..4, "!"), change(1..1, "bad")]),
            EditError::Overlap,
        ),
        (
            batch(id, base, vec![change(usize::MAX..usize::MAX, "!")]),
            EditError::InvalidRange,
        ),
    ] {
        assert_eq!(d.commit(b), Err(expected));
        assert_eq!(d.text(), "aébc");
        assert_eq!(d.selection(), selection);
        assert_eq!(d.revision(), base);
    }
    assert!(d
        .commit(batch(id, base, vec![change(1..3, "é")]))
        .unwrap()
        .is_none());
    assert!(d.commit(batch(id, base, vec![])).unwrap().is_none());
    assert_eq!(d.drain_events().count(), 2);
    d.redo().unwrap().unwrap();
    assert_eq!(d.text(), "aéXc");
    assert_eq!(d.revision(), Revision(3));
}

#[test]
fn maps_inverse_and_directed_selection() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("0123456789");
    let d = docs.get_mut(id).unwrap();
    d.set_selection(SourceSelection { anchor: 9, head: 2 })
        .unwrap();
    let event = d
        .commit(batch(
            id,
            d.revision(),
            vec![change(1..3, "α"), change(5..7, ""), change(8..8, "!")],
        ))
        .unwrap()
        .unwrap();
    assert_eq!(d.text(), "0α347!89");
    assert_eq!(event.forward_map.map(2, Affinity::Before), 1);
    assert_eq!(event.forward_map.map(2, Affinity::After), 3);
    assert_eq!(event.forward_map.map(8, Affinity::Before), 6);
    assert_eq!(event.forward_map.map(8, Affinity::After), 7);
    assert_eq!(d.selection(), SourceSelection { anchor: 8, head: 3 });
    d.commit(batch(id, d.revision(), event.inverse)).unwrap();
    assert_eq!(d.text(), "0123456789");
}

#[test]
fn adjacent_deletions_produce_replayable_inverse() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("abcd");
    let d = docs.get_mut(id).unwrap();
    let e = d
        .commit(batch(
            id,
            d.revision(),
            vec![change(0..1, ""), change(1..2, ""), change(2..3, "!")],
        ))
        .unwrap()
        .unwrap();
    assert_eq!(d.text(), "!d");
    d.commit(batch(id, d.revision(), e.inverse)).unwrap();
    assert_eq!(d.text(), "abcd");
}

#[test]
fn deterministic_random_batches_match_string_and_restore_exact_bytes() {
    let mut docs = SourceDocuments::default();
    let original = "\u{feff}aé\r\n👩‍💻 العربية xyz\n";
    let id = docs.open(original);
    let d = docs.get_mut(id).unwrap();
    let mut seed = 0x7123984u64;
    for _ in 0..800 {
        let before = d.text().to_string();
        let boundaries: Vec<_> = before
            .char_indices()
            .map(|(i, _)| i)
            .chain([before.len()])
            .collect();
        let mut random = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            seed as usize
        };
        let mut points: Vec<_> = (0..4)
            .map(|_| boundaries[random() % boundaries.len()])
            .collect();
        points.sort_unstable();
        points.dedup();
        let mut changes = Vec::new();
        for pair in points.chunks(2) {
            let end = *pair.last().unwrap();
            changes.push(change(
                pair[0]..end,
                ["", "é", "\r\n", "👩‍💻", "X"][random() % 5],
            ));
        }
        let mut expected = before.clone();
        for c in changes.iter().rev() {
            expected.replace_range(c.range.clone(), &c.replacement);
        }
        let event = d.commit(batch(id, d.revision(), changes)).unwrap();
        assert_eq!(d.text(), expected);
        if let Some(event) = event {
            let rev = d.revision();
            let inverse = event.inverse.clone();
            d.commit(batch(id, rev, inverse)).unwrap();
            assert_eq!(d.text(), before);
            d.undo().unwrap();
            assert_eq!(d.text(), expected);
            d.redo().unwrap();
            assert_eq!(d.text(), before);
        }
        d.drain_events().for_each(drop);
    }
}

fn boot(text: &str) -> (App, rndr_core::Entity, DocumentId) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Platform::Windows);
    let mut docs = SourceDocuments::default();
    let id = docs.open(text);
    app.world_mut().insert_resource(docs);
    let entity = app.world_mut().spawn((
        Buffer::from_str("discarded projection"),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), entity, id).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(entity);
    app.update();
    (app, entity, id)
}
fn doc(app: &App, id: DocumentId) -> &SourceSession {
    app.world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
}
fn doc_mut(app: &mut App, id: DocumentId) -> &mut SourceSession {
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
}
fn key(app: &mut App, ch: char, command: bool) {
    let mut mods = Modifiers::new();
    mods.ctrl = command;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::character(ch, mods));
    app.update();
}

#[test]
fn native_typing_host_paste_and_undo_use_one_document_history() {
    let (mut app, entity, id) = boot("ab");
    key(&mut app, 'x', false);
    key(&mut app, 'y', false);
    let rev = doc(&app, id).revision();
    doc_mut(&mut app, id)
        .commit(batch(id, rev, vec![change(0..1, "Z")]))
        .unwrap();
    app.update();
    assert_eq!(doc(&app, id).text(), "Zbxy");
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abxy");
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "ab");
    key(&mut app, 'y', true);
    assert_eq!(doc(&app, id).text(), "abxy");
    app.world_mut()
        .resource_mut::<rndr_core::Clipboard>()
        .unwrap()
        .text = "\r\né".into();
    key(&mut app, 'v', true);
    assert_eq!(doc(&app, id).text(), "abxy\r\né");
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abxy");
    assert_eq!(
        rndr_core::edit_buffer(app.world_mut(), entity, 0..0, "bad"),
        Err(rndr_core::BufferEditError::ManagedDocument)
    );
    let events: Vec<_> = doc_mut(&mut app, id).drain_events().collect();
    assert_eq!(events.len(), 8);
    for (i, e) in events.iter().enumerate() {
        assert_eq!(e.before, Revision(i as u64));
        assert_eq!(e.after, Revision(i as u64 + 1));
    }
    assert_eq!(events[2].origin, EditOrigin::Host(7));
    assert_eq!(events[3].origin, EditOrigin::Undo);
}

#[test]
fn standalone_and_queued_ime_capture_selection_and_commit_once() {
    for queued in [false, true] {
        let (mut app, entity, id) = boot("abcd");
        let b = app.world_mut().get_mut::<Buffer>(entity).unwrap();
        b.caret = 3;
        b.anchor = 1;
        app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
        app.update();
        assert!(doc(&app, id).is_composing());
        assert_eq!(doc(&app, id).text(), "abcd");
        let base = doc(&app, id).revision();
        assert_eq!(
            doc_mut(&mut app, id).commit(batch(id, base, vec![change(0..0, "!")])),
            Err(EditError::Composing)
        );
        assert_eq!(
            doc_mut(&mut app, id).set_readonly(true),
            Err(EditError::Composing)
        );
        assert_eq!(
            detach_source(app.world_mut(), entity),
            Err(EditError::Composing)
        );
        let i = app.world_mut().resource_mut::<Input>().unwrap();
        i.ime_preedit.clear();
        i.ime_commit = "日本".into();
        i.ime_enabled = true;
        if queued {
            i.keys.push(KeyEvent::character('本', Modifiers::new()));
        }
        app.update();
        assert_eq!(doc(&app, id).text(), "a日本d");
        assert_eq!(doc(&app, id).revision(), Revision(1));
        key(&mut app, 'z', true);
        assert_eq!(doc(&app, id).text(), "abcd");
        assert_eq!(
            doc(&app, id).selection(),
            SourceSelection { head: 3, anchor: 1 }
        );
        let events: Vec<_> = doc_mut(&mut app, id).drain_events().collect();
        assert_eq!(events[0].origin, EditOrigin::Ime);
    }
}

#[test]
fn readonly_all_origins_and_reload_admission() {
    let (mut app, _, id) = boot("abc");
    key(&mut app, '!', false);
    doc_mut(&mut app, id).set_readonly(true).unwrap();
    let base = doc(&app, id).revision();
    key(&mut app, 'x', false);
    key(&mut app, 'z', true);
    key(&mut app, 'v', true);
    assert_eq!(doc(&app, id).text(), "abc!");
    assert_eq!(
        doc_mut(&mut app, id).commit(batch(id, base, vec![change(0..0, "!")])),
        Err(EditError::Readonly)
    );
    assert_eq!(
        doc_mut(&mut app, id).reload(base, "external"),
        Err(EditError::Readonly)
    );
    assert_eq!(doc_mut(&mut app, id).undo(), Err(EditError::Readonly));
    doc_mut(&mut app, id).set_readonly(false).unwrap();
    assert_eq!(
        doc_mut(&mut app, id).reload(Revision(0), "external"),
        Err(EditError::StaleRevision)
    );
    doc_mut(&mut app, id).reload(base, "external").unwrap();
    assert!(doc_mut(&mut app, id).undo().unwrap().is_none());
    assert_eq!(doc(&app, id).text(), "external");
}

#[test]
fn document_survives_detach_projection_is_not_authority_and_multiple_views() {
    let (mut app, entity, id) = boot("a");
    key(&mut app, 'b', false);
    let other = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), other, id).unwrap();
    app.world_mut()
        .get_mut::<Buffer>(entity)
        .unwrap()
        .insert_str(0, "UNAUTHORIZED");
    app.update();
    assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "ab");
    detach_source(app.world_mut(), entity).unwrap();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, None);
    attach_source(app.world_mut(), other, id).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(other);
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "a");
}

#[test]
fn event_backpressure_refuses_without_losing_content_or_history() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("");
    let d = docs.get_mut(id).unwrap();
    for _ in 0..EVENT_LIMIT {
        d.commit(batch(
            id,
            d.revision(),
            vec![change(d.text().len()..d.text().len(), "a")],
        ))
        .unwrap();
    }
    let snap = d.snapshot();
    assert_eq!(d.undo(), Err(EditError::EventBackpressure));
    assert_eq!(d.snapshot(), snap);
    assert_eq!(d.drain_events().count(), EVENT_LIMIT);
    d.undo().unwrap();
    assert_eq!(d.text().len(), EVENT_LIMIT - 1);
}

#[test]
fn navigation_and_copy_remain_available_at_event_backpressure() {
    let (mut app, entity, id) = boot("");
    for _ in 0..EVENT_LIMIT {
        let d = doc_mut(&mut app, id);
        let end = d.text().len();
        d.commit(batch(id, d.revision(), vec![change(end..end, "a")]))
            .unwrap();
    }
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(
            rndr_core::NamedKey::ArrowLeft,
            Modifiers::new(),
        ));
    app.update();
    assert_eq!(doc(&app, id).selection().head, EVENT_LIMIT - 1);
    key(&mut app, 'a', true);
    key(&mut app, 'c', true);
    assert_eq!(
        app.world().resource::<rndr_core::Clipboard>().unwrap().text,
        "a".repeat(EVENT_LIMIT)
    );
    assert!(!app.world().has::<SourceInputError>(entity));
    key(&mut app, '!', false);
    assert_eq!(
        app.world().get::<SourceInputError>(entity),
        Some(&SourceInputError(EditError::EventBackpressure))
    );
    assert_eq!(doc(&app, id).text(), "a".repeat(EVENT_LIMIT));
    assert_eq!(doc_mut(&mut app, id).drain_events().count(), EVENT_LIMIT);
}

#[test]
fn net_noop_batch_and_identical_reload_preserve_redo() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("ab");
    let d = docs.get_mut(id).unwrap();
    d.commit(batch(id, Revision(0), vec![change(2..2, "!")]))
        .unwrap();
    d.undo().unwrap();
    let base = d.revision();
    assert!(d
        .commit(batch(id, base, vec![change(0..1, ""), change(1..2, "ab")]))
        .unwrap()
        .is_none());
    assert!(d.reload(base, "ab").unwrap().is_none());
    d.redo().unwrap();
    assert_eq!(d.text(), "ab!");
    assert_eq!(d.revision(), Revision(3));
}

#[test]
fn explicit_host_selection_survives_projection_without_content_revision() {
    let (mut app, entity, id) = boot("abcdef");
    doc_mut(&mut app, id)
        .set_selection(SourceSelection { anchor: 1, head: 4 })
        .unwrap();
    app.update();
    let b = app.world().get::<Buffer>(entity).unwrap();
    assert_eq!((b.anchor, b.caret), (1, 4));
    assert_eq!(doc(&app, id).revision(), Revision(0));
    key(&mut app, '!', false);
    assert_eq!(doc(&app, id).text(), "a!ef");
}

#[test]
fn composition_does_not_transfer_to_another_focused_document() {
    let (mut app, first, id) = boot("first");
    let second_id = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open("second");
    let second = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), second, second_id).unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(second);
    app.update();
    assert!(doc(&app, id).is_composing());
    assert!(!doc(&app, second_id).is_composing());
    assert!(!app.world().has::<rndr_core::ImePreview>(second));
    let i = app.world_mut().resource_mut::<Input>().unwrap();
    i.ime_preedit.clear();
    i.ime_commit = "日本".into();
    app.update();
    assert_eq!(doc(&app, id).text(), "first");
    assert_eq!(doc(&app, second_id).text(), "second");
    assert!(!doc(&app, id).is_composing());
    detach_source(app.world_mut(), first).unwrap();
}

#[test]
fn readonly_policy_tracks_session_changes_and_never_displays_preedit() {
    let (mut app, entity, id) = boot("a");
    app.world_mut().insert(
        entity,
        rndr_core::Semantics {
            name: Some("Source".into()),
            description: String::new(),
            readonly: false,
        },
    );
    doc_mut(&mut app, id).set_readonly(true).unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    assert!(
        app.world()
            .get::<rndr_core::Semantics>(entity)
            .unwrap()
            .readonly
    );
    assert!(!app.world().has::<rndr_core::ImePreview>(entity));
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    doc_mut(&mut app, id).set_readonly(false).unwrap();
    key(&mut app, 'b', false);
    assert!(
        !app.world()
            .get::<rndr_core::Semantics>(entity)
            .unwrap()
            .readonly
    );
    assert_eq!(doc(&app, id).text(), "ab");
}

#[test]
fn legacy_chars_also_join_document_history() {
    let (mut app, _, id) = boot("");
    app.world_mut().resource_mut::<Input>().unwrap().chars = "é".into();
    app.update();
    assert_eq!(doc(&app, id).text(), "é");
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "");
}
