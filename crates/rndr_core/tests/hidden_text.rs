//! Hidden presentation is disposable; updates made while hidden survive unhide.
use rndr_core::*;
use std::sync::Arc;

fn fixture() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(300.0, 100.0));
    let parent = app.world_mut().spawn(Layout::column());
    let label = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Label::new("before"),
    ));
    set_parent(app.world_mut(), label, parent);
    (app, parent, label)
}

#[test]
fn hidden_ancestor_defers_layout_then_rebuilds_updated_text_and_ime() {
    let (mut app, parent, label) = fixture();
    app.update();
    let before = app.world().get::<TextLayout>(label).unwrap().clone();
    app.world_mut().insert(parent, Hidden);
    app.world_mut().get_mut::<Label>(label).unwrap().text = "after".into();
    app.world_mut().insert(
        label,
        ImePreview {
            text: "!".into(),
            cursor: Some((1, 1)),
        },
    );
    app.update();
    assert!(app.world().get::<TextLayout>(label).is_none());
    assert_eq!(app.world().get::<Label>(label).unwrap().text, "after");
    app.world_mut().remove::<Hidden>(parent);
    app.update();
    let after = app.world().get::<TextLayout>(label).unwrap();
    assert_ne!(after, &before);
    assert_eq!(after.ime_range, Some((5, 6)));
    assert_eq!(after.ime_cursor, Some(6));
}

#[test]
fn hidden_label_is_not_shaped_and_unhide_uses_replacement_engine() {
    let (mut app, _, label) = fixture();
    let engine = Arc::new(BuiltinText::new());
    let erased: Arc<dyn TextEngine> = engine.clone();
    app.world_mut().insert_resource(erased);
    app.world_mut().insert(label, Hidden);
    app.update();
    assert_eq!(engine.shape_count(), 0);
    assert!(app.world().get::<TextLayout>(label).is_none());
    let replacement = Arc::new(BuiltinText::new());
    let erased: Arc<dyn TextEngine> = replacement.clone();
    app.world_mut().insert_resource(erased);
    app.world_mut().remove::<Hidden>(label);
    app.update();
    assert!(replacement.shape_count() > 0);
    assert!(app.world().get::<TextLayout>(label).is_some());
}

#[test]
fn hidden_live_text_updates_source_without_presentation() {
    let (mut app, parent, label) = fixture();
    let text = LiveText::new("before");
    app.world_mut().insert(label, LiveLabel::new(&text));
    app.update();
    app.world_mut().insert(parent, Hidden);
    app.update();
    text.set("while hidden");
    app.update_live();
    assert_eq!(
        app.world().get::<Label>(label).unwrap().text,
        "while hidden"
    );
    assert!(app.world().get::<TextLayout>(label).is_none());
    app.world_mut().remove::<Hidden>(parent);
    app.update();
    assert!(app.world().get::<TextLayout>(label).is_some());
}

#[test]
fn unhide_matches_fresh_layout_after_width_style_scale_and_eviction() {
    let (mut app, parent, label) = fixture();
    let engine = Arc::new(BuiltinText::new());
    let erased: Arc<dyn TextEngine> = engine.clone();
    app.world_mut().insert_resource(erased);
    app.world_mut().get_mut::<Label>(label).unwrap().overflow = TextOverflow::Wrap;
    app.update();
    app.world_mut().insert(parent, Hidden);
    for _ in 0..4 {
        app.update();
    }
    app.world_mut().get_mut::<Layout>(label).unwrap().width = Size::Px(80.0);
    app.world_mut().get_mut::<Label>(label).unwrap().text =
        "changed text wraps across lines".into();
    app.world_mut().insert(
        label,
        TextStyle {
            size: 22.0,
            ..TextStyle::default()
        },
    );
    app.world_mut().insert_resource(Viewport {
        scale: 2.0,
        ..Viewport::new(300.0, 100.0)
    });
    let shaped = engine.shape_count();
    app.world_mut().remove::<Hidden>(parent);
    app.update();
    assert!(
        engine.shape_count() > shaped,
        "unhide rebuilds evicted presentation"
    );
    let restored = app.world().get::<TextLayout>(label).unwrap().clone();
    app.world_mut().remove::<TextLayout>(label);
    app.update();
    assert_eq!(&restored, app.world().get::<TextLayout>(label).unwrap());
}

#[test]
fn hidden_source_session_still_synchronizes_and_keeps_undo() {
    use rndr_core::editing::*;
    let (mut app, parent, label) = fixture();
    app.add_plugin(Chrome);
    let mut docs = SourceDocuments::default();
    let doc = docs.open("before");
    app.world_mut().insert_resource(docs);
    app.world_mut().insert(label, Buffer::from_str(""));
    app.world_mut().insert(label, Editor::default());
    attach_source(app.world_mut(), label, doc).unwrap();
    app.update();
    app.world_mut().insert(parent, Hidden);
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
                range: 0..6,
                replacement: "updated".into(),
            }],
        })
        .unwrap();
    app.update();
    assert!(app.world().get::<TextLayout>(label).is_none());
    assert_eq!(app.world().get::<Buffer>(label).unwrap().text(), "updated");
    app.world_mut().remove::<Hidden>(parent);
    app.update();
    assert!(app.world().get::<TextLayout>(label).is_some());
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    session.undo().unwrap();
    assert_eq!(session.text(), "before");
}
