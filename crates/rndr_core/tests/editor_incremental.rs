//! Deterministic editor work/anchor tests, not native latency certification.
use rndr_core::editing::*;
use rndr_core::*;

fn boot(text: &str) -> (App, Entity, DocumentId) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let mut docs = SourceDocuments::default();
    let doc = docs.open(text);
    app.world_mut().insert_resource(docs);
    let e = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(100.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(""),
        Editor::default(),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), e, doc).unwrap();
    app.world_mut().insert_resource(Viewport::new(300.0, 120.0));
    for _ in 0..3 {
        app.update();
    }
    (app, e, doc)
}
fn stats(app: &App, e: Entity) -> EditorLayoutStats {
    *app.world().get::<EditorLayoutStats>(e).unwrap()
}
fn splice(app: &mut App, doc: DocumentId, range: std::ops::Range<usize>, replacement: &str) {
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session
        .commit(SourceBatch {
            document: doc,
            base: session.revision(),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range,
                replacement: replacement.into(),
            }],
        })
        .unwrap();
    app.update();
}
#[test]
fn idle_scroll_does_not_reassemble_or_shape_document_and_window_is_bounded() {
    let text: String = (0..1000)
        .map(|n| format!("line {n}: é 👩‍💻 data\n"))
        .collect();
    let (mut app, e, _) = boot(&text);
    let before = stats(&app, e);
    for offset in [100.0, 500.0, 1500.0, 0.0] {
        app.world_mut().get_mut::<Editor>(e).unwrap().offset = offset;
        app.update();
        let after = stats(&app, e);
        assert_eq!(after.assemblies, before.assemblies);
        assert_eq!(after.bytes_shaped, before.bytes_shaped);
        assert!(app.world().get::<Label>(e).unwrap().text.len() < 1000);
        assert!(app.world().get::<TextLayout>(e).unwrap().glyphs.len() < 500);
    }
}
#[test]
fn one_paragraph_edit_reuses_others_and_preserves_scrolled_content() {
    let text: String = (0..100)
        .map(|n| format!("row {n:03} alpha beta\n"))
        .collect();
    let (mut app, e, doc) = boot(&text);
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = 500.0;
    app.update();
    let top = app.world().get::<Label>(e).unwrap().text.clone();
    let before = stats(&app, e);
    splice(&mut app, doc, 0..0, "new row\n");
    let after = stats(&app, e);
    assert_eq!(after.paragraphs_shaped - before.paragraphs_shaped, 1);
    assert!(after.paragraphs_reused - before.paragraphs_reused >= 100);
    assert_eq!(app.world().get::<Label>(e).unwrap().text, top);
    assert!(after.paragraph_cache_bytes <= 8 * 1024 * 1024);
}
#[test]
fn exact_batch_anchors_compose_across_revisions_and_undo_in_two_views() {
    let text: String = (0..100)
        .map(|n| format!("row {n:03} alpha beta\n"))
        .collect();
    let (mut app, a, doc) = boot(&text);
    let b = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(100.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(""),
        Editor::default(),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), b, doc).unwrap();
    for _ in 0..3 {
        app.update();
    }
    app.world_mut().get_mut::<Editor>(a).unwrap().offset = 400.0;
    app.world_mut().get_mut::<Editor>(b).unwrap().offset = 800.0;
    app.update();
    let tops: Vec<_> = [a, b]
        .iter()
        .map(|&e| app.world().get::<Label>(e).unwrap().text.clone())
        .collect();
    assert_ne!(tops[0], tops[1]);
    // Two disjoint edits surround each viewport; a second transaction arrives
    // before either pane publishes, so retaining just the last map is insufficient.
    for n in 0..2 {
        let session = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(doc)
            .unwrap();
        let end = session.text().len();
        session
            .commit(SourceBatch {
                document: doc,
                base: session.revision(),
                origin: EditOrigin::Host(1),
                changes: vec![
                    SourceChange {
                        range: 0..0,
                        replacement: format!("prefix {n}\n"),
                    },
                    SourceChange {
                        range: end..end,
                        replacement: format!("suffix {n}\n"),
                    },
                ],
            })
            .unwrap();
    }
    app.update();
    for (e, top) in [a, b].into_iter().zip(&tops) {
        assert_eq!(&app.world().get::<Label>(e).unwrap().text, top);
    }
    for undo in [true, true, false, false] {
        let session = app
            .world_mut()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(doc)
            .unwrap();
        if undo {
            session.undo().unwrap();
        } else {
            session.redo().unwrap();
        }
        app.update();
        for (e, top) in [a, b].into_iter().zip(&tops) {
            assert_eq!(&app.world().get::<Label>(e).unwrap().text, top);
        }
    }
}

#[test]
fn repeated_newlines_keep_peer_source_anchor_through_native_enter_and_undo() {
    let (mut app, a, doc) = boot(&"\n".repeat(200));
    let b = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(100.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(""),
        Editor::default(),
        CapturesKeys,
    ));
    attach_source(app.world_mut(), b, doc).unwrap();
    for _ in 0..3 {
        app.update();
    }
    app.world_mut().get_mut::<Editor>(b).unwrap().offset = 400.0;
    app.update();
    let before = app.world().get::<Editor>(b).unwrap().offset;
    let row_h = app.world().get::<TextLayout>(b).unwrap().lines[0].h;
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session
        .set_view_selection(
            a,
            session.revision(),
            SourceSelection { anchor: 0, head: 0 },
        )
        .unwrap();
    let focus = Focus { entity: Some(a) };
    app.world_mut().insert_resource(focus);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::Enter, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Editor>(b).unwrap().offset, before + row_h);
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap()
        .undo()
        .unwrap();
    app.update();
    assert_eq!(app.world().get::<Editor>(b).unwrap().offset, before);
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap()
        .redo()
        .unwrap();
    app.update();
    assert_eq!(app.world().get::<Editor>(b).unwrap().offset, before + row_h);
}

#[test]
fn idle_source_projection_repairs_raw_buffer_mutation_and_document_rebinding() {
    let (mut app, e, original) = boot("original text");
    app.world_mut()
        .get_mut::<Buffer>(e)
        .unwrap()
        .insert_str(0, "tampered");
    app.update();
    assert_eq!(
        app.world().get::<Buffer>(e).unwrap().text(),
        "original text"
    );
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(original)
            .unwrap()
            .revision(),
        Revision(0)
    );
    detach_source(app.world_mut(), e).unwrap();
    let other = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open("different text");
    attach_source(app.world_mut(), e, other).unwrap();
    app.update();
    assert_eq!(
        app.world().get::<Buffer>(e).unwrap().text(),
        "different text"
    );
}

#[test]
fn width_and_style_reflow_retain_byte_anchor_and_invalidate_geometry() {
    let text: String = (0..80)
        .map(|n| format!("row {n:03} alpha beta gamma delta epsilon zeta eta\n"))
        .collect();
    let (mut app, e, _) = boot(&text);
    app.world_mut().get_mut::<Editor>(e).unwrap().offset = 600.0;
    app.update();
    let top = app.world().get::<Label>(e).unwrap().text.clone();
    let prefix = &top[..top.len().min(10)];
    let before = stats(&app, e);
    app.world_mut().get_mut::<Layout>(e).unwrap().width = Size::Px(180.0);
    for _ in 0..2 {
        app.update();
    }
    let label = &app.world().get::<Label>(e).unwrap().text;
    assert!(label[..label.len().min(100)].contains(prefix));
    assert!(stats(&app, e).assemblies > before.assemblies);
    let before = stats(&app, e);
    app.world_mut().insert(
        e,
        TextStyle {
            size: 20.0,
            ..TextStyle::default()
        },
    );
    for _ in 0..2 {
        app.update();
    }
    assert!(stats(&app, e).assemblies > before.assemblies);
    assert!(app.world().get::<Editor>(e).unwrap().offset > 0.0);
}
