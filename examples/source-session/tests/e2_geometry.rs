//! Headless Parley geometry, not native keyboard/IME certification.
use rndr_core::editing::*;
use rndr_core::*;
use rndr_ui::editing::*;
use rndr_ui::{UiId, UiPlugin};
use std::sync::Arc;

fn boot(text: &str) -> (App, Entity, DocumentId) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let engine: Arc<dyn TextEngine> = Arc::new(rndr_render::ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.world_mut().insert_resource(Viewport::new(260.0, 150.0));
    let mut docs = SourceDocuments::default();
    let doc = docs.open(text);
    app.world_mut().insert_resource(docs);
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let mut props = EditorSurfaceProps::new(doc, "Unicode source");
    props.gutter = false;
    let view = EditorSurface::mount(app.world_mut(), root, UiId(1), props).unwrap();
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(view.editor);
    for _ in 0..3 {
        app.update();
    }
    (app, view.editor, doc)
}
fn key(app: &mut App, key: NamedKey, shift: bool) {
    let mut modifiers = Modifiers::new();
    modifiers.shift = shift;
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(key, modifiers));
    app.update();
}
#[test]
fn mixed_bidi_left_right_remain_logical_grapheme_commands() {
    let clusters = [
        "A",
        "e\u{301}",
        "👩‍💻",
        "🇺🇸",
        " ",
        "م",
        "ر",
        "ح",
        "ب",
        "ا",
        " ",
        "Z",
        "\r\n",
    ];
    let text = clusters.concat();
    let mut stops = vec![0];
    for cluster in clusters {
        stops.push(stops.last().unwrap() + cluster.len());
    }
    let (mut app, editor, doc) = boot(&text);
    for &expected in stops.iter().rev().skip(1) {
        key(&mut app, NamedKey::ArrowLeft, false);
        let buffer = app.world().get::<Buffer>(editor).unwrap();
        assert_eq!((buffer.caret, buffer.anchor), (expected, expected));
    }
    for &expected in stops.iter().skip(1) {
        key(&mut app, NamedKey::ArrowRight, true);
        let buffer = app.world().get::<Buffer>(editor).unwrap();
        assert_eq!((buffer.caret, buffer.anchor), (expected, 0));
        let snapshot = access_text(app.world(), editor).unwrap();
        assert_eq!(snapshot.selection.head, expected);
        assert_eq!(snapshot.selection.anchor, 0);
        let caret = snapshot.caret_bounds();
        assert!([caret.x, caret.y, caret.h].iter().all(|v| v.is_finite()));
        assert!(snapshot
            .selection_bounds()
            .iter()
            .all(|r| r.w >= 0.0 && r.h >= 0.0));
    }
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    assert_eq!(session.revision(), Revision(0));
    assert_eq!(session.text(), text);
    assert!(session.undo().unwrap().is_none());
}
#[test]
fn shaped_page_selection_geometry_remains_coherent_after_resize_and_readonly() {
    let text = "Latin e\u{301} 👩‍💻 🇺🇸 العربية\r\n".repeat(30);
    let (mut app, editor, doc) = boot(&text);
    app.world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap()
        .set_readonly(true)
        .unwrap();
    let anchor = app.world().get::<Buffer>(editor).unwrap().anchor;
    for width in [260.0, 180.0, 330.0] {
        app.world_mut().insert_resource(Viewport::new(width, 150.0));
        app.update();
        app.update();
        for direction in [NamedKey::PageUp, NamedKey::PageDown] {
            key(&mut app, direction, true);
            let snapshot = access_text(app.world(), editor).unwrap();
            let buffer = app.world().get::<Buffer>(editor).unwrap();
            assert!(snapshot.readonly);
            assert_eq!(
                snapshot.selection,
                SourceSelection {
                    anchor,
                    head: buffer.caret
                }
            );
            assert!(text.is_char_boundary(buffer.caret));
            assert!(snapshot.caret_row < snapshot.layout.lines.len());
            let caret = snapshot.caret_bounds();
            assert!(caret.x.is_finite() && caret.y.is_finite() && caret.h > 0.0);
            assert!(caret.y + caret.h > snapshot.clip.y);
            assert!(caret.y < snapshot.clip.y + snapshot.clip.h);
        }
    }
    let session = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(doc)
        .unwrap();
    assert_eq!(session.revision(), Revision(0));
    assert_eq!(session.drain_events().count(), 0);
    assert_eq!(session.text(), text);
}
