use rndr_core::editing::{
    attach_source, detach_source, DocumentId, EditError, EditOrigin, SourceBatch, SourceChange,
    SourceDocuments, SourceSelection,
};
use rndr_core::source_extensions::*;
use rndr_core::{Buffer, Color, Editor, Entity, Focus, Input, World};

fn boot(text: &str) -> (World, Entity, DocumentId, SourceExtensions, ProviderId) {
    let mut world = World::new();
    let mut docs = SourceDocuments::default();
    let id = docs.open(text);
    world.insert_resource(docs);
    let view = world.spawn((Buffer::from_str(text), Editor::default()));
    attach_source(&mut world, view, id).unwrap();
    world.insert_resource(Focus { entity: Some(view) });
    world.insert_resource(Input::default());
    let mut extensions = SourceExtensions::default();
    let provider = extensions.register(0);
    (world, view, id, extensions, provider)
}
fn item(range: std::ops::Range<usize>, text: &str) -> CompletionItem {
    CompletionItem {
        label: text.into(),
        changes: vec![SourceChange {
            range,
            replacement: text.into(),
        }],
    }
}
fn content(world: &World, id: DocumentId) -> &str {
    world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .text()
}

#[test]
fn strict_utf16_crlf_non_bmp_and_final_empty_line() {
    let text = "a😀é\r\n中\n";
    for byte in [0, 1, 5, 7, 9, 12, 13] {
        let p = byte_to_utf16(text, byte).unwrap();
        assert_eq!(utf16_to_byte(text, p), Ok(byte));
    }
    assert_eq!(
        utf16_to_byte(
            text,
            Utf16Position {
                line: 0,
                character: 3
            }
        ),
        Ok(5)
    );
    for p in [
        Utf16Position {
            line: 0,
            character: 2,
        },
        Utf16Position {
            line: 0,
            character: 5,
        },
        Utf16Position {
            line: 3,
            character: 0,
        },
    ] {
        assert_eq!(utf16_to_byte(text, p), Err(ExtensionError::InvalidPosition));
    }
    for byte in [2, 3, 4, 6, 8, usize::MAX] {
        assert_eq!(
            byte_to_utf16(text, byte),
            Err(ExtensionError::InvalidPosition)
        );
    }
    assert_eq!(utf16_to_byte("", Utf16Position::default()), Ok(0));
    assert_eq!(
        byte_to_utf16("x\r", 2),
        Ok(Utf16Position {
            line: 0,
            character: 2
        })
    );
    assert!(utf16_change(
        text,
        Utf16Position {
            line: 1,
            character: 0
        },
        Utf16Position {
            line: 0,
            character: 0
        },
        "x"
    )
    .is_err());
}

#[test]
fn delayed_out_of_order_superseded_cancelled_and_changed_revision() {
    let (mut world, view, id, mut ex, p) = boot("x");
    let (old, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    let (new, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    assert!(old.is_cancelled());
    let mut fake = FakeDelayedProvider::default();
    fake.enqueue(5, old, vec![item(1..1, "old")]);
    fake.enqueue(2, new.clone(), vec![item(1..1, "new")]);
    assert_eq!(fake.deliver(2, &mut ex, &world), vec![Ok(())]);
    assert_eq!(ex.completions(&world, view).unwrap().1[0].label, "new");
    assert_eq!(
        fake.deliver(5, &mut ex, &world),
        vec![Err(ExtensionError::Stale)]
    );
    let base = new.revision();
    world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .commit(SourceBatch {
            document: id,
            base,
            origin: EditOrigin::Host(99),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "z".into(),
            }],
        })
        .unwrap();
    assert!(ex.completions(&world, view).is_none());
    assert_eq!(
        ex.publish_completions(&world, &new, vec![]),
        Err(ExtensionError::Stale)
    );
    ex.refresh(&world);
    assert!(new.is_cancelled());
    assert_eq!(content(&world, id), "zx");
}

#[test]
fn completion_one_atomic_history_step_and_overlap_refusal() {
    let (mut world, view, id, mut ex, p) = boot("abc");
    let (ticket, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    let invalid = CompletionItem {
        label: "bad".into(),
        changes: vec![
            SourceChange {
                range: 0..2,
                replacement: "A".into(),
            },
            SourceChange {
                range: 1..3,
                replacement: "B".into(),
            },
        ],
    };
    assert_eq!(
        ex.publish_completions(&world, &ticket, vec![invalid]),
        Err(ExtensionError::Edit(EditError::Overlap))
    );
    assert_eq!(content(&world, id), "abc");
    let valid = CompletionItem {
        label: "good".into(),
        changes: vec![
            SourceChange {
                range: 0..1,
                replacement: "A".into(),
            },
            SourceChange {
                range: 2..3,
                replacement: "C".into(),
            },
        ],
    };
    ex.publish_completions(&world, &ticket, vec![valid])
        .unwrap();
    let event = ex.accept_completion(&mut world, view, 0).unwrap().unwrap();
    assert_eq!(event.changes.len(), 2);
    assert_eq!(content(&world, id), "AbC");
    assert!(ex.completions(&world, view).is_none());
    let doc = world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    doc.undo().unwrap().unwrap();
    assert_eq!(doc.text(), "abc");
    assert!(doc.undo().unwrap().is_none());
    doc.redo().unwrap().unwrap();
    assert_eq!(doc.text(), "AbC");
}

#[test]
fn readonly_focus_ime_detach_and_identity_guards() {
    let (mut world, view, id, mut ex, p) = boot("x");
    let (t, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    ex.publish_completions(&world, &t, vec![item(1..1, "y")])
        .unwrap();
    world.resource_mut::<Focus>().unwrap().entity = None;
    assert_eq!(
        ex.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::Unfocused)
    );
    ex.refresh(&world);
    world.resource_mut::<Focus>().unwrap().entity = Some(view);
    assert_eq!(
        ex.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::Stale)
    );
    let (t, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    world.resource_mut::<Input>().unwrap().ime_preedit = "仮".into();
    assert_eq!(
        ex.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::Composing)
    );
    ex.refresh(&world);
    world.resource_mut::<Input>().unwrap().ime_preedit.clear();
    let (t, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true)
        .unwrap();
    assert_eq!(
        ex.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::Readonly)
    );
    ex.refresh(&world);
    assert!(t.is_cancelled());
    // Diagnostics remain valid on readonly source, but quick-fix does not.
    let (d, _) = ex
        .request(&world, p, view, RequestKind::Diagnostics)
        .unwrap();
    ex.publish_diagnostics(
        &world,
        &d,
        vec![Diagnostic {
            range: 0..1,
            message: "hint".into(),
            severity: DiagnosticSeverity::Hint,
            fix: vec![SourceChange {
                range: 0..1,
                replacement: "Y".into(),
            }],
        }],
    )
    .unwrap();
    assert_eq!(
        ex.apply_diagnostic_fix(&mut world, p, view, 0),
        Err(ExtensionError::Readonly)
    );
    ex.cancel_view(view);
    detach_source(&mut world, view).unwrap();
    let other = world.resource_mut::<SourceDocuments>().unwrap().open("x");
    attach_source(&mut world, view, other).unwrap();
    assert!(ex.publish_diagnostics(&world, &d, vec![]).is_err());
    assert_eq!(content(&world, id), "x");
    assert_eq!(content(&world, other), "x");
}

#[test]
fn explicit_cancellation_also_retires_same_frame_detach_rebind() {
    let (mut world, view, id, mut ex, p) = boot("x");
    let (t, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    ex.cancel_view(view);
    detach_source(&mut world, view).unwrap();
    attach_source(&mut world, view, id).unwrap();
    world.resource_mut::<Focus>().unwrap().entity = Some(view);
    assert!(ex
        .publish_completions(&world, &t, vec![item(1..1, "late")])
        .is_err());
    assert_eq!(content(&world, id), "x");
}

#[test]
fn priority_ownership_and_provider_teardown_are_deterministic() {
    let (world, view, _, mut ex, first) = boot("x");
    let second = ex.register(0);
    let high = ex.register(10);
    let mut tickets = vec![];
    for (p, label) in [(second, "second"), (high, "high"), (first, "first")] {
        let (t, _) = ex
            .request(&world, p, view, RequestKind::Completion)
            .unwrap();
        ex.publish_completions(&world, &t, vec![item(1..1, label)])
            .unwrap();
        tickets.push(t);
        let (d, _) = ex
            .request(&world, p, view, RequestKind::Decoration)
            .unwrap();
        ex.publish_decorations(
            &world,
            &d,
            vec![SourceDecoration {
                range: 0..1,
                class: label.into(),
                color: Color::rgb(1., 0., 0.),
            }],
            vec![GutterMarker {
                at: 0,
                label: label.into(),
            }],
        )
        .unwrap();
    }
    assert_eq!(ex.completions(&world, view).unwrap().0, high);
    assert_eq!(
        ex.decorations(&world, view)
            .iter()
            .map(|(_, d)| d.class.as_str())
            .collect::<Vec<_>>(),
        vec!["second", "first", "high"]
    );
    assert_eq!(ex.gutter_markers(&world, view).len(), 3);
    ex.remove_provider(high);
    assert!(tickets[1].is_cancelled());
    assert_eq!(ex.completions(&world, view).unwrap().0, first);
    assert_eq!(ex.decorations(&world, view).len(), 2);
    ex.remove_provider(first);
    assert_eq!(ex.completions(&world, view).unwrap().0, second);
    assert_eq!(
        ex.publish_completions(&world, &tickets[1], vec![]),
        Err(ExtensionError::MissingProvider)
    );
}

#[test]
fn navigation_changes_only_invoking_selection_without_history_or_focus() {
    let (mut world, view, id, mut ex, p) = boot("abc");
    let (ticket, _) = ex
        .request(&world, p, view, RequestKind::Navigation)
        .unwrap();
    let selection = SourceSelection { anchor: 2, head: 1 };
    ex.navigate(&mut world, &ticket, selection).unwrap();
    let doc = world
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.view_selection(view), Some(selection));
    assert_eq!(doc.revision(), ticket.revision());
    assert_eq!(world.resource::<Focus>().unwrap().entity, Some(view));
    assert_eq!(content(&world, id), "abc");
    assert!(ticket.is_cancelled());
    assert!(world
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .undo()
        .unwrap()
        .is_none());
}

#[test]
fn foreign_controller_ticket_and_manual_cancel_are_refused() {
    let (world, view, _, mut ex, p) = boot("x");
    let (t, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    let mut foreign = SourceExtensions::default();
    assert_eq!(
        foreign.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::MissingProvider)
    );
    t.cancel();
    assert_eq!(
        ex.publish_completions(&world, &t, vec![]),
        Err(ExtensionError::Stale)
    );
}

#[test]
fn same_frame_lifecycle_aba_never_resurrects_tickets() {
    for boundary in 0..5 {
        let (mut world, view, id, mut ex, p) = boot("x");
        let (ticket, _) = ex
            .request(&world, p, view, RequestKind::Completion)
            .unwrap();
        match boundary {
            0 => {
                detach_source(&mut world, view).unwrap();
                attach_source(&mut world, view, id).unwrap();
                world.resource_mut::<Focus>().unwrap().entity = Some(view);
            }
            1 => {
                let focus = world.resource_mut::<Focus>().unwrap();
                focus.entity = None;
                focus.entity = Some(view);
            }
            2 => {
                let doc = world
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(id)
                    .unwrap();
                doc.set_readonly(true).unwrap();
                doc.set_readonly(false).unwrap();
            }
            3 => {
                world.insert(view, rndr_core::Semantics::default());
                let policy = world.get_mut::<rndr_core::Semantics>(view).unwrap();
                policy.readonly = true;
                policy.readonly = false;
            }
            _ => {
                world.insert(
                    view,
                    rndr_core::ImePreview {
                        text: "x".into(),
                        cursor: None,
                    },
                );
                world.remove::<rndr_core::ImePreview>(view);
            }
        }
        assert_eq!(
            ex.publish_completions(&world, &ticket, vec![item(0..1, "late")]),
            Err(ExtensionError::Stale)
        );
        assert_eq!(content(&world, id), "x");
    }
}

fn completion_app() -> (rndr_core::App, Entity, DocumentId) {
    use rndr_core::{App, CapturesKeys, GenericInteraction, Label, Platform};
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Platform::Windows);
    let mut docs = SourceDocuments::default();
    let id = docs.open("x");
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Buffer::from_str("x"),
        Editor::default(),
        Label::new(""),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view);
    app.update();
    // The fixture has no layout focus policy. Restore its explicit focus after
    // the initial shell/layout publication, before capturing the request.
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view);
    let mut ex = SourceExtensions::default();
    let p = ex.register(0);
    let (ticket, _) = ex
        .request(app.world(), p, view, RequestKind::Completion)
        .unwrap();
    ex.publish_completions(
        app.world(),
        &ticket,
        vec![item(0..1, "first"), item(0..1, "second")],
    )
    .unwrap();
    app.world_mut().insert_resource(ex);
    (app, view, id)
}
#[test]
fn delayed_ticket_survives_idle_updates_and_unrelated_policy_mutation() {
    let (mut app, view, _) = completion_app();
    // Keep shell focus normalization out of this fixture's idle path.
    app.world_mut().insert(view, rndr_core::Focusable);
    let unrelated = app.world_mut().spawn(rndr_core::Semantics::default());
    let mut ex = SourceExtensions::default();
    let p = ex.register(0);
    let (ticket, _) = ex
        .request(app.world(), p, view, RequestKind::Completion)
        .unwrap();
    let mut fake = FakeDelayedProvider::default();
    fake.enqueue(5, ticket.clone(), vec![item(0..1, "delayed")]);
    for _ in 0..5 {
        app.world_mut()
            .get_mut::<rndr_core::Semantics>(unrelated)
            .unwrap()
            .readonly = true;
        app.world_mut().insert(
            unrelated,
            rndr_core::ImePreview {
                text: "other".into(),
                cursor: None,
            },
        );
        app.world_mut().remove::<rndr_core::ImePreview>(unrelated);
        // Native same-target focus requests must not look like focus ABA.
        assert!(rndr_core::request_focus(app.world_mut(), view));
        app.update();
        ex.refresh(app.world());
        assert!(!ticket.is_cancelled());
    }
    assert_eq!(fake.deliver(5, &mut ex, app.world()), vec![Ok(())]);
    assert_eq!(
        ex.completions(app.world(), view).unwrap().1[0].label,
        "delayed"
    );
}

#[test]
fn ancestor_policy_and_reparent_aba_cancel_but_other_view_detach_does_not() {
    use rndr_core::{Parent, Semantics};
    let (mut world, view, id, mut ex, p) = boot("x");
    let ancestor = world.spawn(Semantics::default());
    world.insert(view, Parent { entity: ancestor });
    let other = world.spawn((Buffer::from_str("x"), Editor::default()));
    attach_source(&mut world, other, id).unwrap();
    let (ticket, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    detach_source(&mut world, other).unwrap();
    ex.publish_completions(&world, &ticket, vec![]).unwrap();
    let policy = world.get_mut::<Semantics>(ancestor).unwrap();
    policy.readonly = true;
    policy.readonly = false;
    assert_eq!(
        ex.publish_completions(&world, &ticket, vec![]),
        Err(ExtensionError::Stale)
    );
    let (ticket, _) = ex
        .request(&world, p, view, RequestKind::Completion)
        .unwrap();
    world.remove::<Parent>(view);
    world.insert(view, Parent { entity: ancestor });
    assert_eq!(
        ex.publish_completions(&world, &ticket, vec![]),
        Err(ExtensionError::Stale)
    );
}

#[test]
fn attachment_epoch_is_unique_but_idempotent_attach_is_stable() {
    use rndr_core::editing::source_attachment_epoch;
    let (mut world, view, id, mut ex, p) = boot("x");
    let epoch = source_attachment_epoch(&world, view).unwrap();
    let (ticket, _) = ex
        .request(&world, p, view, RequestKind::Diagnostics)
        .unwrap();
    attach_source(&mut world, view, id).unwrap();
    assert_eq!(source_attachment_epoch(&world, view), Some(epoch));
    ex.publish_diagnostics(&world, &ticket, vec![]).unwrap();
    detach_source(&mut world, view).unwrap();
    assert_eq!(source_attachment_epoch(&world, view), None);
    attach_source(&mut world, view, id).unwrap();
    assert_ne!(source_attachment_epoch(&world, view), Some(epoch));
    assert_eq!(
        ex.publish_diagnostics(&world, &ticket, vec![]),
        Err(ExtensionError::Stale)
    );
}

fn named(key: rndr_core::NamedKey) -> rndr_core::KeyEvent {
    rndr_core::KeyEvent::named(key, rndr_core::Modifiers::new())
}
#[test]
fn chronological_down_then_tab_accepts_selected_completion_once() {
    use rndr_core::{KeyBinding, Keymap, KeymapFired, Keystroke, NamedKey};
    let (mut app, view, id) = completion_app();
    app.world_mut()
        .resource_mut::<Keymap>()
        .unwrap()
        .bind(KeyBinding::new(
            "leaked-tab",
            [Keystroke::named(NamedKey::Tab)],
        ));
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![named(NamedKey::ArrowDown), named(NamedKey::Tab)];
    app.update();
    assert_eq!(content(app.world(), id), "second");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(view));
    assert!(app
        .world()
        .resource::<KeymapFired>()
        .unwrap()
        .actions
        .is_empty());
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    doc.undo().unwrap();
    assert_eq!(doc.text(), "x");
}
#[test]
fn earlier_text_invalidates_later_enter_and_escape_does_not_fire_keymap() {
    use rndr_core::{KeyBinding, KeyEvent, Keymap, KeymapFired, Keystroke, Modifiers, NamedKey};
    let (mut app, _, id) = completion_app();
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::character('z', Modifiers::new()),
        named(NamedKey::Enter),
    ];
    app.update();
    // The source fixture starts at EOF. Earlier text stays there; stale completion
    // must not replace x before the subsequent ordinary Enter is applied.
    assert_eq!(content(app.world(), id), "xz\n");
    let (mut app, view, id) = completion_app();
    app.world_mut()
        .resource_mut::<Keymap>()
        .unwrap()
        .bind(KeyBinding::new(
            "quit",
            [Keystroke::named(NamedKey::Escape)],
        ));
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![named(NamedKey::Escape)];
    app.update();
    assert_eq!(content(app.world(), id), "x");
    assert!(app
        .world()
        .resource::<SourceExtensions>()
        .unwrap()
        .completions(app.world(), view)
        .is_none());
    assert!(app
        .world()
        .resource::<KeymapFired>()
        .unwrap()
        .actions
        .is_empty());
}
#[test]
fn completion_yields_entire_coalesced_control_queue_to_ime() {
    use rndr_core::NamedKey;
    let (mut app, view, id) = completion_app();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit = "候".into();
    input.keys = vec![
        named(NamedKey::ArrowDown),
        named(NamedKey::Enter),
        named(NamedKey::Tab),
        named(NamedKey::Escape),
    ];
    app.update();
    assert_eq!(content(app.world(), id), "x");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(view));
    assert!(app
        .world()
        .resource::<SourceExtensions>()
        .unwrap()
        .completions(app.world(), view)
        .is_none());
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = "候".into();
    input.keys = vec![named(NamedKey::Enter)];
    app.update();
    assert_eq!(content(app.world(), id), "x候");
}
