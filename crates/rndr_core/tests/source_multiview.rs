use rndr_core::editing::*;
use rndr_core::{
    App, Buffer, CapturesKeys, Chrome, Editor, Entity, Focus, Input, KeyEvent, Label, Modifiers,
    Platform,
};

fn view(app: &mut App, id: DocumentId) -> Entity {
    let e = app.world_mut().spawn((
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), e, id).unwrap();
    e
}
fn boot(text: &str) -> (App, DocumentId, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Focus::default());
    app.world_mut().insert_resource(Platform::Windows);
    let mut docs = SourceDocuments::default();
    let id = docs.open(text);
    app.world_mut().insert_resource(docs);
    let a = view(&mut app, id);
    let b = view(&mut app, id);
    focus(&mut app, a);
    app.update();
    (app, id, a, b)
}
fn focus(app: &mut App, e: Entity) {
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(e);
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
fn select(app: &mut App, id: DocumentId, e: Entity, anchor: usize, head: usize) {
    let d = doc_mut(app, id);
    d.set_view_selection(e, d.revision(), SourceSelection { anchor, head })
        .unwrap();
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
fn assert_projection(app: &App, id: DocumentId, e: Entity) {
    let d = doc(app, id);
    let b = app.world().get::<Buffer>(e).unwrap();
    assert_eq!(b.text(), d.text());
    assert_eq!(
        SourceSelection {
            anchor: b.anchor,
            head: b.caret
        },
        d.view_selection(e).unwrap()
    );
}

fn named(app: &mut App, key: rndr_core::NamedKey) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(key, Modifiers::new()));
    app.update();
}

#[test]
fn repeated_native_insertions_use_actual_range_and_grouped_history_from_either_pane() {
    for unit in ["a", "é", "👩‍💻"] {
        for invoking_second in [false, true] {
            let text = unit.repeat(4);
            let (mut app, id, a, b) = boot(&text);
            let (actor, peer) = if invoking_second { (b, a) } else { (a, b) };
            let n = unit.len();
            select(&mut app, id, actor, 0, 0);
            select(&mut app, id, peer, 3 * n, 2 * n);
            focus(&mut app, actor);
            for (i, _) in (0..2).enumerate() {
                // A complete grapheme may arrive as one native text event.
                let mut event = KeyEvent::character('a', Modifiers::new());
                event.text = Some(unit.into());
                app.world_mut()
                    .resource_mut::<Input>()
                    .unwrap()
                    .keys
                    .push(event);
                app.update();
                let event = doc_mut(&mut app, id).drain_events().next().unwrap();
                assert_eq!(
                    event.changes,
                    vec![SourceChange {
                        range: i * n..i * n,
                        replacement: unit.into()
                    }]
                );
                assert_eq!(
                    doc(&app, id).view_selection(peer),
                    Some(SourceSelection {
                        anchor: (4 + i) * n,
                        head: (3 + i) * n
                    })
                );
            }
            assert_eq!(doc(&app, id).text(), unit.repeat(6));
            // Invoke history from the OTHER pane. Only that pane restores
            // snapshot selection; the original actor follows the exact map.
            focus(&mut app, peer);
            key(&mut app, 'z', true);
            assert_eq!(doc(&app, id).text(), text);
            let undo = doc_mut(&mut app, id).drain_events().next().unwrap();
            assert_eq!(
                undo.changes,
                vec![SourceChange {
                    range: 0..2 * n,
                    replacement: String::new()
                }]
            );
            assert_eq!(
                doc(&app, id).view_selection(actor),
                Some(SourceSelection { anchor: 0, head: 0 })
            );
            key(&mut app, 'y', true);
            assert_eq!(doc(&app, id).text(), unit.repeat(6));
            let redo = doc_mut(&mut app, id).drain_events().next().unwrap();
            assert_eq!(
                redo.changes,
                vec![SourceChange {
                    range: 0..0,
                    replacement: unit.repeat(2)
                }]
            );
            assert_eq!(
                doc(&app, id).view_selection(actor),
                Some(SourceSelection {
                    anchor: 2 * n,
                    head: 2 * n
                })
            );
            assert_projection(&app, id, actor);
            assert_projection(&app, id, peer);
        }
    }
}

#[test]
fn repeated_native_deletions_preserve_actual_grapheme_and_selection_ranges() {
    use rndr_core::NamedKey;
    for unit in ["a", "é", "👩‍💻"] {
        let n = unit.len();
        for (key_name, anchor, head, range) in [
            (NamedKey::Backspace, n, n, 0..n),
            (NamedKey::Delete, 0, 0, 0..n),
            (NamedKey::Backspace, 2 * n, 0, 0..2 * n),
            (NamedKey::Delete, 0, 2 * n, 0..2 * n),
        ] {
            let text = unit.repeat(5);
            let (mut app, id, a, b) = boot(&text);
            select(&mut app, id, a, anchor, head);
            select(&mut app, id, b, 4 * n, 3 * n);
            named(&mut app, key_name);
            let event = doc_mut(&mut app, id).drain_events().next().unwrap();
            assert_eq!(
                event.changes,
                vec![SourceChange {
                    range: range.clone(),
                    replacement: String::new()
                }]
            );
            let removed = range.len();
            assert_eq!(
                doc(&app, id).view_selection(b),
                Some(SourceSelection {
                    anchor: 4 * n - removed,
                    head: 3 * n - removed
                })
            );
            key(&mut app, 'z', true);
            assert_eq!(doc(&app, id).text(), text);
            assert_eq!(
                doc(&app, id).view_selection(b),
                Some(SourceSelection {
                    anchor: 4 * n,
                    head: 3 * n
                })
            );
            key(&mut app, 'y', true);
            assert_eq!(
                doc(&app, id).view_selection(b),
                Some(SourceSelection {
                    anchor: 4 * n - removed,
                    head: 3 * n - removed
                })
            );
            assert_projection(&app, id, a);
            assert_projection(&app, id, b);
        }
    }
}

#[test]
fn repeated_selection_replacement_keeps_original_range_across_grouped_typing_history() {
    for unit in ["a", "é", "👩‍💻"] {
        let n = unit.len();
        let (mut app, id, a, b) = boot(&unit.repeat(5));
        select(&mut app, id, a, 2 * n, n);
        select(&mut app, id, b, 4 * n, 3 * n);
        for replacement in [unit.repeat(2), unit.into()] {
            let mut event = KeyEvent::character('a', Modifiers::new());
            event.text = Some(replacement);
            app.world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .keys
                .push(event);
            app.update();
        }
        let events: Vec<_> = doc_mut(&mut app, id).drain_events().collect();
        assert_eq!(
            events[0].changes,
            vec![SourceChange {
                range: n..2 * n,
                replacement: unit.repeat(2)
            }]
        );
        assert_eq!(
            events[1].changes,
            vec![SourceChange {
                range: 3 * n..3 * n,
                replacement: unit.into()
            }]
        );
        assert_eq!(
            doc(&app, id).view_selection(b),
            Some(SourceSelection {
                anchor: 6 * n,
                head: 5 * n
            })
        );
        key(&mut app, 'z', true);
        assert_eq!(doc(&app, id).text(), unit.repeat(5));
        let undo = doc_mut(&mut app, id).drain_events().next().unwrap();
        assert_eq!(
            undo.changes,
            vec![SourceChange {
                range: n..4 * n,
                replacement: unit.into()
            }]
        );
        assert_eq!(
            doc(&app, id).view_selection(a),
            Some(SourceSelection {
                anchor: 2 * n,
                head: n
            })
        );
        assert_eq!(
            doc(&app, id).view_selection(b),
            Some(SourceSelection {
                anchor: 4 * n,
                head: 3 * n
            })
        );
        key(&mut app, 'y', true);
        assert_eq!(doc(&app, id).text(), unit.repeat(7));
        let redo = doc_mut(&mut app, id).drain_events().next().unwrap();
        assert_eq!(
            redo.changes,
            vec![SourceChange {
                range: n..2 * n,
                replacement: unit.repeat(3)
            }]
        );
        assert_projection(&app, id, a);
        assert_projection(&app, id, b);
    }
}

#[test]
fn disjoint_batch_undo_redo_preserves_interior_peer_selections_and_maps() {
    for (anchor, head) in [(2, 2), (3, 1)] {
        let (mut app, id, a, b) = boot("abcd");
        select(&mut app, id, b, anchor, head);
        let event = doc_mut(&mut app, id)
            .commit(SourceBatch {
                document: id,
                base: Revision(0),
                origin: EditOrigin::Host(99),
                changes: vec![
                    SourceChange {
                        range: 0..0,
                        replacement: "X".into(),
                    },
                    SourceChange {
                        range: 4..4,
                        replacement: "Y".into(),
                    },
                ],
            })
            .unwrap()
            .unwrap();
        let shifted = SourceSelection {
            anchor: anchor + 1,
            head: head + 1,
        };
        assert_eq!(doc(&app, id).view_selection(b), Some(shifted));
        app.update();
        for _ in 0..3 {
            let undo = doc_mut(&mut app, id).undo().unwrap().unwrap();
            assert_eq!(undo.changes, event.inverse);
            assert_eq!(undo.forward_map.map(3, Affinity::After), 2);
            assert_eq!(doc(&app, id).text(), "abcd");
            assert_eq!(
                doc(&app, id).view_selection(b),
                Some(SourceSelection { anchor, head })
            );
            app.update();
            assert_projection(&app, id, a);
            assert_projection(&app, id, b);
            let redo = doc_mut(&mut app, id).redo().unwrap().unwrap();
            assert_eq!(redo.changes, event.changes);
            assert_eq!(redo.forward_map.map(2, Affinity::After), 3);
            assert_eq!(doc(&app, id).text(), "XabcdY");
            assert_eq!(doc(&app, id).view_selection(b), Some(shifted));
            app.update();
            assert_projection(&app, id, a);
            assert_projection(&app, id, b);
        }
    }
}

#[test]
fn grouped_native_history_and_disjoint_batches_keep_independent_exact_maps() {
    let (mut app, id, a, b) = boot("abcd");
    select(&mut app, id, a, 0, 0);
    select(&mut app, id, b, 3, 1);
    key(&mut app, '界', false);
    key(&mut app, 'é', false);
    assert_eq!(doc(&app, id).revision(), Revision(2));
    assert_eq!(doc(&app, id).text(), "界éabcd");
    let base = doc(&app, id).revision();
    doc_mut(&mut app, id)
        .commit(SourceBatch {
            document: id,
            base,
            origin: EditOrigin::Host(7),
            changes: vec![
                SourceChange {
                    range: 0..0,
                    replacement: "X".into(),
                },
                SourceChange {
                    range: 9..9,
                    replacement: "Y".into(),
                },
            ],
        })
        .unwrap();
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "界éabcd");
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 8, head: 6 })
    );
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abcd");
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 3, head: 1 })
    );
    key(&mut app, 'y', true);
    key(&mut app, 'y', true);
    assert_eq!(doc(&app, id).text(), "X界éabcdY");
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 9, head: 7 })
    );
    assert_projection(&app, id, a);
    assert_projection(&app, id, b);
}

#[test]
fn native_transactions_map_peer_directed_selections_in_same_frame_in_both_orders() {
    for invoking_second in [false, true] {
        let (mut app, id, a, b) = boot("aébcdef");
        let (actor, peer) = if invoking_second { (b, a) } else { (a, b) };
        select(&mut app, id, actor, 1, 3);
        select(&mut app, id, peer, 8, 5);
        focus(&mut app, actor);
        key(&mut app, '界', false);
        assert_eq!(doc(&app, id).text(), "a界bcdef");
        assert_eq!(
            doc(&app, id).view_selection(actor),
            Some(SourceSelection { anchor: 4, head: 4 })
        );
        assert_eq!(
            doc(&app, id).view_selection(peer),
            Some(SourceSelection { anchor: 9, head: 6 })
        );
        assert_eq!(doc(&app, id).revision(), Revision(1));
        assert_projection(&app, id, a);
        assert_projection(&app, id, b);
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(actor));
        let events: Vec<_> = doc_mut(&mut app, id).drain_events().collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].origin, EditOrigin::Keyboard);
    }
}

#[test]
fn undo_and_redo_restore_invoking_pane_only_and_break_cross_pane_typing_run() {
    let (mut app, id, a, b) = boot("abcd");
    select(&mut app, id, a, 1, 1);
    select(&mut app, id, b, 4, 4);
    key(&mut app, 'X', false);
    focus(&mut app, b);
    key(&mut app, 'Y', false);
    assert_eq!(doc(&app, id).text(), "aXbcdY");
    // Undo B's typing from A: A restores history selection, B maps normally.
    focus(&mut app, a);
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "aXbcd");
    assert_eq!(
        doc(&app, id).view_selection(a),
        Some(SourceSelection { anchor: 5, head: 5 })
    );
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 5, head: 5 })
    );
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abcd");
    assert_eq!(
        doc(&app, id).view_selection(a),
        Some(SourceSelection { anchor: 1, head: 1 })
    );
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 4, head: 4 })
    );
    key(&mut app, 'y', true);
    assert_eq!(doc(&app, id).text(), "aXbcd");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(a));
    assert_projection(&app, id, a);
    assert_projection(&app, id, b);
    assert_eq!(doc(&app, id).revision(), Revision(5));
}

#[test]
fn host_batch_maps_all_views_and_stale_view_selection_refuses_atomically() {
    let (mut app, id, a, b) = boot("aébcdef");
    select(&mut app, id, a, 8, 1);
    select(&mut app, id, b, 4, 4);
    let event = doc_mut(&mut app, id)
        .commit(SourceBatch {
            document: id,
            base: Revision(0),
            origin: EditOrigin::Host(88),
            changes: vec![SourceChange {
                range: 1..4,
                replacement: "👩‍💻".into(),
            }],
        })
        .unwrap()
        .unwrap();
    let snap = doc(&app, id).snapshot();
    let selection = doc(&app, id).view_selection(b);
    assert_eq!(
        doc_mut(&mut app, id).set_view_selection(
            b,
            Revision(0),
            SourceSelection { anchor: 0, head: 0 }
        ),
        Err(EditError::StaleRevision)
    );
    assert_eq!(doc(&app, id).snapshot(), snap);
    assert_eq!(doc(&app, id).view_selection(b), selection);
    app.update();
    for e in [a, b] {
        assert_projection(&app, id, e);
    }
    assert_eq!(
        doc(&app, id).view_selection(a),
        Some(SourceSelection {
            anchor: event.forward_map.map(8, Affinity::After),
            head: 12
        })
    );
    // Split-codepoint and stale detached handles never move a view.
    assert_eq!(
        doc_mut(&mut app, id).set_view_selection(
            b,
            Revision(1),
            SourceSelection { anchor: 2, head: 2 }
        ),
        Err(EditError::InvalidRange)
    );
    detach_source(app.world_mut(), b).unwrap();
    assert_eq!(
        doc_mut(&mut app, id).set_view_selection(b, Revision(1), SourceSelection::default()),
        Err(EditError::InvalidView)
    );
}

#[test]
fn detach_reopen_preserves_document_history_other_selection_and_scroll() {
    let text = "line\n".repeat(100);
    let (mut app, id, a, b) = boot(&text);
    select(&mut app, id, a, 0, 0);
    select(&mut app, id, b, 250, 250);
    app.world_mut().get_mut::<Editor>(b).unwrap().offset = 80.0;
    key(&mut app, '!', false);
    assert_eq!(app.world().get::<Editor>(b).unwrap().offset, 80.0);
    detach_source(app.world_mut(), a).unwrap();
    assert_eq!(doc(&app, id).view_selection(a), None);
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection {
            anchor: 251,
            head: 251
        })
    );
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, None);
    let c = view(&mut app, id);
    focus(&mut app, c);
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), text);
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection {
            anchor: 250,
            head: 250
        })
    );
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(c));
    assert_projection(&app, id, b);
}

#[test]
fn composition_is_document_exclusive_and_never_transfers_between_same_doc_panes() {
    let (mut app, id, a, b) = boot("abcd");
    select(&mut app, id, a, 1, 3);
    select(&mut app, id, b, 4, 4);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    assert!(doc(&app, id).is_composing());
    assert_eq!(
        doc_mut(&mut app, id).set_view_selection(b, Revision(0), SourceSelection::default()),
        Err(EditError::Composing)
    );
    assert_eq!(detach_source(app.world_mut(), b), Err(EditError::Composing));
    let c = app
        .world_mut()
        .spawn((Buffer::from_str(""), Editor::default()));
    assert_eq!(
        attach_source(app.world_mut(), c, id),
        Err(EditError::Composing)
    );
    focus(&mut app, b);
    let i = app.world_mut().resource_mut::<Input>().unwrap();
    i.ime_preedit.clear();
    i.ime_commit = "日本".into();
    app.update();
    assert_eq!(doc(&app, id).text(), "abcd");
    assert_eq!(doc(&app, id).revision(), Revision(0));
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 4, head: 4 })
    );
    assert!(!doc(&app, id).is_composing());
    focus(&mut app, a);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
    app.update();
    let i = app.world_mut().resource_mut::<Input>().unwrap();
    i.ime_preedit.clear();
    i.ime_commit = "日本".into();
    app.update();
    assert_eq!(doc(&app, id).text(), "a日本d");
    assert_eq!(doc(&app, id).revision(), Revision(1));
    assert_projection(&app, id, a);
    assert_projection(&app, id, b);
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abcd");
    assert_eq!(
        doc(&app, id).view_selection(a),
        Some(SourceSelection { anchor: 1, head: 3 })
    );
}

#[test]
fn accessible_snapshots_and_selection_actions_are_per_view_and_revision_fenced() {
    use rndr_core::{access_text, try_apply_access, AccessCommand, AccessCommandResult, Rect};
    let (mut app, id, a, b) = boot("aébcdef");
    for (i, e) in [a, b].into_iter().enumerate() {
        app.world_mut().insert(
            e,
            Rect {
                x: i as f32 * 220.0,
                y: 0.0,
                w: 200.0,
                h: 80.0,
            },
        );
    }
    select(&mut app, id, a, 1, 3);
    select(&mut app, id, b, 8, 4);
    app.update();
    let first = access_text(app.world(), a).unwrap();
    let second = access_text(app.world(), b).unwrap();
    assert_eq!(first.selection, SourceSelection { anchor: 1, head: 3 });
    assert_eq!(second.selection, SourceSelection { anchor: 8, head: 4 });
    assert!(first.selectable);
    assert!(!second.selectable);
    // Focus can move between publication and the accessibility action.
    focus(&mut app, b);
    assert_eq!(
        try_apply_access(
            app.world_mut(),
            AccessCommand::SetTextSelection {
                entity: b,
                state: second.state.clone(),
                selection: SourceSelection { anchor: 3, head: 1 },
            }
        ),
        AccessCommandResult::Applied
    );
    assert_eq!(doc(&app, id).view_selection(a), Some(first.selection));
    assert_eq!(
        doc(&app, id).view_selection(b),
        Some(SourceSelection { anchor: 3, head: 1 })
    );
    key(&mut app, '!', false);
    assert_eq!(doc(&app, id).text(), "a!bcdef");
    assert_eq!(
        try_apply_access(
            app.world_mut(),
            AccessCommand::SetTextSelection {
                entity: b,
                state: second.state,
                selection: SourceSelection::default(),
            }
        ),
        AccessCommandResult::Rejected
    );
    assert_eq!(
        access_text(app.world(), a).unwrap().selection,
        doc(&app, id).view_selection(a).unwrap()
    );
    assert_eq!(
        access_text(app.world(), b).unwrap().selection,
        doc(&app, id).view_selection(b).unwrap()
    );
}

#[test]
fn destroyed_or_malformed_views_retire_without_losing_document_or_peer_state() {
    let (mut app, id, a, b) = boot("abc");
    key(&mut app, '!', false);
    app.world_mut().despawn_entity(a);
    focus(&mut app, b);
    app.update();
    assert_eq!(doc(&app, id).view_selection(a), None);
    key(&mut app, 'z', true);
    assert_eq!(doc(&app, id).text(), "abc");
    assert_projection(&app, id, b);
    app.world_mut().remove::<Editor>(b);
    app.update();
    assert_eq!(doc(&app, id).view_selection(b), None);
    assert_eq!(
        doc_mut(&mut app, id).set_view_selection(b, Revision(2), SourceSelection::default()),
        Err(EditError::InvalidView)
    );
    app.world_mut().insert(b, Editor::default());
    attach_source(app.world_mut(), b, id).unwrap();
    focus(&mut app, b);
    key(&mut app, 'y', true);
    assert_eq!(doc(&app, id).text(), "abc!");
    assert_projection(&app, id, b);
}
