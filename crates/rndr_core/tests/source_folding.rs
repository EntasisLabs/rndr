use rndr_core::editing::{attach_source, detach_source, SourceDocuments, SourceSelection};
use rndr_core::source_folding::*;
use rndr_core::{App, Buffer, Chrome, Editor, Entity, Label, Layout, Size, Viewport};

const TEXT: &str = "header\nhidden β\ntail\n";
fn boot() -> (App, Entity, rndr_core::editing::DocumentId) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let mut docs = SourceDocuments::default();
    let id = docs.open(TEXT);
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(180.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(TEXT),
        Editor::default(),
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    app.update();
    app.update();
    (app, view, id)
}
fn collapse(app: &mut App, view: Entity, id: rndr_core::editing::DocumentId) {
    let rev = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    set_source_folds(app.world_mut(), view, id, rev, vec![7..17]).unwrap();
    app.update();
    app.update();
}
#[test]
fn hidden_rows_keep_exact_source_but_project_paint_hit_testing_and_accessibility() {
    let (mut app, view, id) = boot();
    collapse(&mut app, view, id);
    assert_eq!(
        app.world().get::<Label>(view).unwrap().text,
        "header\ntail\n"
    );
    assert_eq!(app.world().get::<Buffer>(view).unwrap().text(), TEXT);
    let access = rndr_core::access_text(app.world(), view).unwrap();
    assert_eq!(access.state.text(), TEXT);
    assert_eq!(access.layout.lines[1].start, 17);
    assert_eq!(access.layout.index_on_line(1, 0.0), 17);
    assert!(access
        .layout
        .lines
        .iter()
        .all(|l| !(7..17).contains(&l.start)));
    assert!(source_range_boxes(app.world(), view, 7, 16).is_empty());
    assert!(source_position_box(app.world(), view, 8).is_none());
    let tail = source_position_box(app.world(), view, 17).unwrap();
    assert!(tail.y > 0.0);
}
#[test]
fn cross_fold_copy_includes_hidden_bytes_and_selection_into_fold_expands() {
    let (mut app, view, id) = boot();
    collapse(&mut app, view, id);
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let revision = doc.revision();
    doc.set_view_selection(
        view,
        revision,
        SourceSelection {
            anchor: 0,
            head: TEXT.len(),
        },
    )
    .unwrap();
    app.update();
    app.update();
    assert_eq!(
        app.world().get::<Label>(view).unwrap().text,
        "header\ntail\n"
    );
    assert_eq!(
        app.world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .copy_selections(),
        TEXT
    );
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    doc.set_view_selection(view, revision, SourceSelection { anchor: 8, head: 8 })
        .unwrap();
    app.update();
    app.update();
    assert_eq!(app.world().get::<Label>(view).unwrap().text, TEXT);
}
#[test]
fn detach_reattach_same_document_does_not_resurrect_folds() {
    let (mut app, view, id) = boot();
    collapse(&mut app, view, id);
    detach_source(app.world_mut(), view).unwrap();
    attach_source(app.world_mut(), view, id).unwrap();
    app.update();
    app.update();
    assert_eq!(app.world().get::<Label>(view).unwrap().text, TEXT);
}
#[test]
fn malformed_publications_are_atomic_and_clear_restores_geometry() {
    let (mut app, view, id) = boot();
    collapse(&mut app, view, id);
    let rev = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    for ranges in [
        vec![0..7],
        vec![8..17],
        vec![7..16],
        vec![7..17, 7..17],
        vec![7..100],
    ] {
        assert_eq!(
            set_source_folds(app.world_mut(), view, id, rev, ranges),
            Err(FoldError::InvalidRange)
        );
        assert_eq!(
            app.world().get::<SourceFolds>(view).unwrap().ranges(),
            &[7..17]
        );
    }
    clear_source_folds(app.world_mut(), view);
    app.update();
    app.update();
    assert_eq!(app.world().get::<Label>(view).unwrap().text, TEXT);
}
#[test]
fn gutter_uses_source_line_numbers_and_edits_invalidate_fold_geometry() {
    use rndr_core::editing::{EditOrigin, SourceBatch, SourceChange};
    use rndr_core::{set_parent, Gutter};
    let (mut app, view, id) = boot();
    let parent = app.world_mut().spawn(Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::row()
    });
    set_parent(app.world_mut(), view, parent);
    let gutter = app.world_mut().spawn((Gutter, Label::new("")));
    set_parent(app.world_mut(), gutter, parent);
    collapse(&mut app, view, id);
    assert!(
        app.world()
            .get::<Label>(gutter)
            .unwrap()
            .text
            .starts_with("1\n3"),
        "gutter={:?}, label={:?}, rect={:?}",
        app.world().get::<Label>(gutter),
        app.world().get::<Label>(view),
        app.world().get::<rndr_core::Rect>(view)
    );
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let old = doc.revision();
    doc.commit(SourceBatch {
        document: id,
        base: old,
        origin: EditOrigin::Host(99),
        changes: vec![SourceChange {
            range: 0..0,
            replacement: "!".into(),
        }],
    })
    .unwrap();
    assert_eq!(
        set_source_folds(app.world_mut(), view, id, old, vec![7..17]),
        Err(FoldError::Stale)
    );
    app.update();
    app.update();
    assert_eq!(
        app.world().get::<Label>(view).unwrap().text,
        format!("!{TEXT}")
    );
    assert!(app.world().get::<SourceFolds>(view).is_none());
}
#[test]
fn representative_hooks_exclude_brackets_inside_strings_comments_and_preserve_utf8_spans() {
    let text = "{\n  \"β\\\"}\" // }\n  42\n}\n";
    let analysis = BraceSourceHooks.analyze(text);
    assert_eq!(analysis.brackets.len(), 1);
    assert_eq!(analysis.folds, vec![2..text.rfind('}').unwrap()]);
    assert!(analysis.indents.iter().any(|i| i.columns == 4));
    assert_eq!(analysis.syntax.len(), 3);
    assert!(analysis
        .syntax
        .iter()
        .all(|span| text.is_char_boundary(span.start) && text.is_char_boundary(span.end)));
}
