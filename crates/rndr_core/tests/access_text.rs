use rndr_core::editing::{
    attach_source, detach_source, DocumentId, EditOrigin, Revision, SourceBatch, SourceChange,
    SourceDocuments, SourceSelection,
};
use rndr_core::{
    access_text, access_tree, try_apply_access, AccessCommand, AccessCommandResult, App, Buffer,
    Chrome, ControlDisabled, Editor, Focus, Hidden, Input, Label, Rect, Semantics,
};

fn boot(text: &str, managed: bool) -> (App, rndr_core::Entity, Option<DocumentId>) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    let e = app.world_mut().spawn((
        Buffer::from_str(text),
        Editor::default(),
        Label::new(""),
        Rect {
            x: 20.0,
            y: 30.0,
            w: 180.0,
            h: 50.0,
        },
    ));
    let id = managed.then(|| {
        let mut docs = SourceDocuments::default();
        let id = docs.open(text);
        app.world_mut().insert_resource(docs);
        attach_source(app.world_mut(), e, id).unwrap();
        id
    });
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
    app.update();
    (app, e, id)
}
fn select(
    app: &mut App,
    e: rndr_core::Entity,
    state: rndr_core::AccessTextState,
    anchor: usize,
    head: usize,
) -> AccessCommandResult {
    try_apply_access(
        app.world_mut(),
        AccessCommand::SetTextSelection {
            entity: e,
            state,
            selection: SourceSelection { anchor, head },
        },
    )
}
#[test]
fn full_source_not_paint_window_and_directed_readonly_selection() {
    for managed in [false, true] {
        let text = "combining e\u{301} 👩‍👩‍👧‍👦 🇺🇳\r\nمرحبا Latin\n".repeat(20);
        let (mut app, e, id) = boot(&text, managed);
        assert_ne!(app.world().get::<Label>(e).unwrap().text, text);
        let snapshot = access_text(app.world(), e).unwrap();
        assert_eq!(snapshot.state.text(), text);
        assert_eq!(
            access_tree(app.world())
                .nodes
                .iter()
                .find(|n| n.id == rndr_core::access_id(e))
                .unwrap()
                .value,
            text
        );
        if let Some(id) = id {
            app.world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap()
                .set_readonly(true)
                .unwrap();
        } else {
            app.world_mut().insert(
                e,
                Semantics {
                    readonly: true,
                    ..Default::default()
                },
            );
        }
        assert!(access_text(app.world(), e).unwrap().readonly);
        assert_eq!(
            select(&mut app, e, snapshot.state, text.len(), 0),
            AccessCommandResult::Applied
        );
        let b = app.world().get::<Buffer>(e).unwrap();
        assert_eq!((b.anchor, b.caret), (text.len(), 0));
        assert_eq!(b.text(), text);
        assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
        if let Some(id) = id {
            let doc = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap();
            assert_eq!(doc.revision(), Revision(0));
            assert_eq!(
                doc.selection(),
                SourceSelection {
                    anchor: text.len(),
                    head: 0
                }
            );
            assert_eq!(doc.drain_events().count(), 0);
        }
    }
}
#[test]
fn scalar_validation_grapheme_snap_and_unchanged_history() {
    for managed in [false, true] {
        let text = "e\u{301}👩‍👩‍👧‍👦🇺🇳\r\n";
        let (mut app, e, id) = boot(text, managed);
        let state = access_text(app.world(), e).unwrap().state;
        assert_eq!(
            select(&mut app, e, state.clone(), 2, 0),
            AccessCommandResult::Rejected
        );
        assert_eq!(
            select(&mut app, e, state.clone(), usize::MAX, 0),
            AccessCommandResult::Rejected
        );
        // A scalar boundary inside combining/ZWJ clusters floors to a grapheme.
        assert_eq!(
            select(&mut app, e, state, 1, 7),
            AccessCommandResult::Applied
        );
        let b = app.world().get::<Buffer>(e).unwrap();
        assert_eq!((b.anchor, b.caret), (0, 3));
        if let Some(id) = id {
            let doc = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap();
            assert_eq!(doc.undo(), Ok(None));
            assert_eq!(doc.revision(), Revision(0));
        }
    }
}
#[test]
fn host_selection_requires_projection_before_announcing_ranges() {
    let (mut app, e, id) = boot("abcdef", true);
    let state = access_text(app.world(), e).unwrap().state;
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id.unwrap())
        .unwrap()
        .set_selection(SourceSelection { anchor: 2, head: 4 })
        .unwrap();
    assert!(access_text(app.world(), e).is_none());
    assert_eq!(
        select(&mut app, e, state, 0, 0),
        AccessCommandResult::Rejected
    );
    app.update();
    let snapshot = access_text(app.world(), e).unwrap();
    assert_eq!(snapshot.selection, SourceSelection { anchor: 2, head: 4 });
    assert!(!snapshot.selection_bounds().is_empty());
}

#[test]
fn stale_revision_source_reattach_and_cross_view_tokens_refuse() {
    let (mut app, e, id) = boot("abc", true);
    let id = id.unwrap();
    let state = access_text(app.world(), e).unwrap().state;
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    doc.commit(SourceBatch {
        document: id,
        base: Revision(0),
        origin: EditOrigin::Host(1),
        changes: vec![SourceChange {
            range: 0..1,
            replacement: "x".into(),
        }],
    })
    .unwrap();
    assert!(
        access_text(app.world(), e).is_none(),
        "unprojected host update is not exposed"
    );
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .undo()
        .unwrap();
    app.update();
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected,
        "revision fences ABA"
    );
    let fresh = access_text(app.world(), e).unwrap().state;
    detach_source(app.world_mut(), e).unwrap();
    let other = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open("abc");
    attach_source(app.world_mut(), e, other).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
    assert_eq!(
        select(&mut app, e, fresh, 0, 0),
        AccessCommandResult::Rejected
    );
    let (mut legacy, a, _) = boot("abc", false);
    let token = access_text(legacy.world(), a).unwrap().state;
    let b = legacy.world_mut().spawn((
        Buffer::from_str("abc"),
        Editor::default(),
        Rect {
            x: 0.0,
            y: 0.0,
            w: 100.0,
            h: 50.0,
        },
    ));
    legacy.world_mut().resource_mut::<Focus>().unwrap().entity = Some(b);
    assert_eq!(
        select(&mut legacy, b, token, 0, 0),
        AccessCommandResult::Rejected
    );
}
#[test]
fn unavailable_or_composing_target_never_moves_or_steals_focus() {
    let (mut app, e, _) = boot("hello", true);
    let state = access_text(app.world(), e).unwrap().state;
    app.world_mut().insert(e, Hidden);
    assert!(access_text(app.world(), e).is_none());
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    app.world_mut().remove::<Hidden>(e);
    app.world_mut().insert(e, ControlDisabled);
    assert!(access_text(app.world(), e).is_none());
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    app.world_mut().remove::<ControlDisabled>(e);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
    assert!(!access_text(app.world(), e).unwrap().selectable);
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, None);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "未".into();
    assert!(!access_text(app.world(), e).unwrap().selectable);
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.world_mut().despawn_entity(e);
    assert_eq!(
        select(&mut app, e, state, 0, 0),
        AccessCommandResult::Rejected
    );
}
#[test]
fn accessible_selection_replacement_uses_the_existing_undo_owner() {
    for managed in [false, true] {
        let (mut app, e, id) = boot("hello world", managed);
        let state = access_text(app.world(), e).unwrap().state;
        assert_eq!(
            select(&mut app, e, state, 11, 6),
            AccessCommandResult::Applied
        );
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .keys
            .push(rndr_core::KeyEvent::character(
                'X',
                rndr_core::Modifiers::new(),
            ));
        app.update();
        assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), "hello X");
        if let Some(id) = id {
            let doc = app
                .world_mut()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(id)
                .unwrap();
            assert_eq!(doc.revision(), Revision(1));
            assert_eq!(doc.drain_events().count(), 1);
            assert!(doc.undo().unwrap().is_some());
            assert_eq!(doc.text(), "hello world");
            assert_eq!(
                doc.selection(),
                SourceSelection {
                    anchor: 11,
                    head: 6
                }
            );
        } else {
            app.world_mut()
                .insert_resource(rndr_core::Platform::Windows);
            let mut modifiers = rndr_core::Modifiers::new();
            modifiers.ctrl = true;
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .keys
                .push(rndr_core::KeyEvent::character('z', modifiers));
            app.update();
            assert_eq!(app.world().get::<Buffer>(e).unwrap().text(), "hello world");
        }
    }
}

#[test]
fn modal_and_inherited_availability_reject_selection() {
    let (mut app, e, _) = boot("abc", true);
    let state = access_text(app.world(), e).unwrap().state;
    let modal = app
        .world_mut()
        .spawn((rndr_core::Layer(5), rndr_core::Modal));
    assert!(access_text(app.world(), e).is_none());
    assert_eq!(
        select(&mut app, e, state.clone(), 0, 0),
        AccessCommandResult::Rejected
    );
    app.world_mut().despawn_entity(modal);
    assert_eq!(
        select(&mut app, e, state, 0, 0),
        AccessCommandResult::Applied
    );
    let snapshot = access_text(app.world(), e).unwrap();
    assert_eq!(snapshot.caret_bounds().x, snapshot.origin.0);
    assert_eq!(snapshot.caret_bounds().y, snapshot.origin.1);
    assert!(snapshot.selection_bounds().is_empty());
}

#[test]
fn geometry_tracks_shaped_wrap_width_and_row_snapped_scroll() {
    let (mut app, e, _) = boot(&"abcdefghi\n".repeat(30), true);
    let initial = access_text(app.world(), e).unwrap();
    let row = &initial.layout.lines[4];
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = row.y + row.h / 2.0;
    let scrolled = access_text(app.world(), e).unwrap();
    assert_eq!(
        scrolled.origin.1,
        scrolled.clip.y + rndr_core::TEXT_ORIGIN_Y - row.y
    );
    assert_eq!(
        scrolled.state, initial.state,
        "scroll does not stale source ranges"
    );
    app.world_mut().get_mut::<Rect>(e).unwrap().w = 45.0;
    let wrapped = access_text(app.world(), e).unwrap();
    assert!(wrapped.layout.lines.len() > scrolled.layout.lines.len());
    assert_eq!(
        wrapped.state, initial.state,
        "resize does not stale source ranges"
    );
    assert_eq!(
        select(&mut app, e, wrapped.state, 0, 0),
        AccessCommandResult::Applied
    );
    assert_eq!(app.world().get::<Editor>(e).unwrap().offset, 0.0);
}
