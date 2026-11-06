//! Headless IME and complex-script matrix.
//!
//! These cases drive the same composition fold the desktop runner uses.
//! They are not an OS input-method session: no candidate window, no native
//! caret rectangle, and no physical DPI. See `docs/ime-complex-scripts.md`.

use rndr_core::editing::{attach_source, SourceDocuments};
use rndr_core::structured::StructuredDocument;
use rndr_core::{
    App, Axis, Buffer, CapturesKeys, Caret, Chrome, Color, Editor, Focus, ImeEvent, ImePreview,
    Input, KeyEvent, Label, Layout, Modifiers, NamedKey, Prompt, Size, TextField, Viewport,
};

const CJK: &str = "にほん";
const HAN: &str = "汉字";
const ARABIC: &str = "مرحبا";

fn column(width: f32, height: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(width),
        height: Size::Px(height),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn field(text: &str) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(320.0, 120.0));
    let entity = app.world_mut().spawn((
        column(220.0, 36.0),
        Color::rgb(0.15, 0.16, 0.18),
        Label::new(text),
        TextField,
        CapturesKeys,
        rndr_core::Focusable,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    (app, entity)
}

fn prompt(text: &str) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(320.0, 160.0));
    let entity = app.world_mut().spawn((
        column(220.0, 48.0),
        Color::rgb(0.15, 0.16, 0.18),
        Label::new(text),
        Prompt::new().submit_on_enter(false),
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    (app, entity)
}

fn editor(text: &str) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(320.0, 160.0));
    let entity = app.world_mut().spawn((
        column(220.0, 80.0),
        Color::rgb(0.14, 0.15, 0.16),
        Label::new(""),
        Buffer::from_str(text),
        Editor::default(),
        CapturesKeys,
    ));
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    app.update();
    app.update();
    (app, entity)
}

fn source_editor(text: &str) -> (App, rndr_core::Entity, rndr_core::editing::DocumentId) {
    let (mut app, entity) = editor(text);
    let mut docs = SourceDocuments::default();
    let id = docs.open(text);
    app.world_mut().insert_resource(docs);
    attach_source(app.world_mut(), entity, id).unwrap();
    app.update();
    (app, entity, id)
}

fn label_of(app: &App, entity: rndr_core::Entity) -> String {
    app.world().get::<Label>(entity).unwrap().text.clone()
}

fn caret_of(app: &App, entity: rndr_core::Entity) -> (usize, usize) {
    let caret = app.world().get::<Caret>(entity).unwrap();
    (caret.index, caret.anchor)
}

fn preedit(app: &mut App, text: &str) {
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = text.into();
    app.update();
}

fn cancel(app: &mut App) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.update();
}

fn commit(app: &mut App, text: &str) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.ime_preedit.clear();
    input.ime_commit = text.into();
    app.update();
}

fn push_nav(app: &mut App, key: NamedKey) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(key, Modifiers::new()));
    app.update();
}

#[test]
fn resolver_drops_caret_keys_during_cjk_and_arabic_preedit() {
    for sample in [CJK, HAN, ARABIC] {
        for key in [
            NamedKey::Home,
            NamedKey::End,
            NamedKey::PageUp,
            NamedKey::PageDown,
        ] {
            let resolved = rndr_core::resolve_ime(&[
                ImeEvent::Preedit(sample.into(), None),
                ImeEvent::Key(KeyEvent::named(key, Modifiers::new())),
            ]);
            assert!(resolved.keys.is_empty(), "{sample} {key:?}");
            assert_eq!(resolved.preedit, sample);
            assert!(resolved.commit.is_empty());
        }
    }
}

#[test]
fn field_prompt_and_editor_commit_cjk_and_arabic_once_and_cancel_cleanly() {
    for sample in [CJK, HAN, ARABIC] {
        for boot in [field, prompt] {
            let (mut app, entity) = boot("hi");
            let before = caret_of(&app, entity);
            preedit(&mut app, sample);
            assert_eq!(label_of(&app, entity), "hi");
            assert_eq!(app.world().get::<ImePreview>(entity).unwrap().text, sample);
            for key in [
                NamedKey::Home,
                NamedKey::End,
                NamedKey::PageUp,
                NamedKey::PageDown,
            ] {
                push_nav(&mut app, key);
                assert_eq!(caret_of(&app, entity), before, "{sample}");
                assert_eq!(label_of(&app, entity), "hi");
            }
            cancel(&mut app);
            assert_eq!(label_of(&app, entity), "hi");
            assert!(app.world().get::<ImePreview>(entity).is_none());

            preedit(&mut app, sample);
            commit(&mut app, sample);
            assert_eq!(label_of(&app, entity), format!("hi{sample}"));
            assert!(app.world().get::<ImePreview>(entity).is_none());
            commit(&mut app, "");
            assert_eq!(label_of(&app, entity), format!("hi{sample}"));
        }

        let (mut app, entity) = editor("hi");
        let before = {
            let buffer = app.world().get::<Buffer>(entity).unwrap();
            (buffer.caret, buffer.anchor)
        };
        preedit(&mut app, sample);
        assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hi");
        assert_eq!(app.world().get::<ImePreview>(entity).unwrap().text, sample);
        for key in [NamedKey::Home, NamedKey::End] {
            push_nav(&mut app, key);
            let buffer = app.world().get::<Buffer>(entity).unwrap();
            assert_eq!((buffer.caret, buffer.anchor), before);
            assert_eq!(buffer.text(), "hi");
        }
        cancel(&mut app);
        assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hi");
        preedit(&mut app, sample);
        commit(&mut app, sample);
        assert_eq!(
            app.world().get::<Buffer>(entity).unwrap().text(),
            format!("hi{sample}")
        );
    }
}

#[test]
fn source_session_keeps_preedit_out_of_the_document() {
    for sample in [CJK, ARABIC] {
        let (mut app, entity, id) = source_editor("hi");
        preedit(&mut app, sample);
        let source = app
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .text()
            .to_string();
        assert_eq!(source, "hi");
        assert_eq!(app.world().get::<Buffer>(entity).unwrap().text(), "hi");
        cancel(&mut app);
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(id)
                .unwrap()
                .text(),
            "hi"
        );
        preedit(&mut app, sample);
        commit(&mut app, sample);
        assert_eq!(
            app.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(id)
                .unwrap()
                .text(),
            format!("hi{sample}")
        );
    }
}

#[test]
fn structured_editor_cancels_without_a_revision_and_commits_once() {
    for sample in [HAN, ARABIC] {
        let mut doc = StructuredDocument::new("hi");
        let before = doc.plain_text();
        let revision = doc.revision();
        doc.begin_composition().unwrap();
        doc.cancel_composition();
        assert_eq!(doc.plain_text(), before);
        assert_eq!(doc.revision(), revision);
        assert!(!doc.is_composing());
        doc.begin_composition().unwrap();
        assert!(doc.commit_composition(sample).unwrap());
        // A new document caret starts at byte 0, so the commit is a prefix.
        assert_eq!(doc.plain_text(), format!("{sample}hi"));
        assert!(!doc.is_composing());
        let committed = doc.revision();
        assert!(doc.commit_composition(sample).is_err());
        assert_eq!(doc.revision(), committed);
        assert_eq!(doc.plain_text(), format!("{sample}hi"));
    }
}
