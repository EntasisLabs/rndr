use rndr_core::editing::{attach_source, detach_source, DocumentId, SourceDocuments};
use rndr_core::source_extensions::*;
use rndr_core::source_presentation::provider_marks;
use rndr_core::{
    App, Buffer, Chrome, Color, Decoration, DecorationKind, Decorations, Editor, Entity, Label,
    Layout, Size, Viewport,
};

fn one_fold(range: std::ops::Range<usize>) -> Vec<std::ops::Range<usize>> {
    vec![range]
}

fn boot() -> (App, Entity, DocumentId, SourceExtensions) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let mut docs = SourceDocuments::default();
    let id = docs.open("alpha\nbeta\ngamma\n");
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(180.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str("alpha\nbeta\ngamma\n"),
        Editor::default(),
        Decorations {
            list: vec![Decoration {
                start: 0,
                end: 2,
                kind: DecorationKind::Find,
            }],
        },
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    app.update();
    app.update();
    (app, view, id, SourceExtensions::default())
}
fn publish(
    app: &App,
    view: Entity,
    extensions: &mut SourceExtensions,
    priority: i32,
    color: Color,
) -> ProviderId {
    let p = extensions.register(priority);
    let (ticket, _) = extensions
        .request(app.world(), p, view, RequestKind::Decoration)
        .unwrap();
    extensions
        .publish_decorations(
            app.world(),
            &ticket,
            vec![SourceDecoration {
                range: 0..5,
                color,
                class: "annotation".into(),
            }],
            vec![GutterMarker {
                at: 6,
                label: "Breakpoint beta".into(),
            }],
        )
        .unwrap();
    p
}
#[test]
fn additive_colors_priority_and_gutter_labels_survive_without_host_mutation() {
    let (mut app, view, _, mut extensions) = boot();
    let red = Color::rgb(0.8, 0.1, 0.2);
    let blue = Color::rgb(0.1, 0.2, 0.8);
    let low = publish(&app, view, &mut extensions, -1, red);
    let high = publish(&app, view, &mut extensions, 10, blue);
    app.world_mut().insert_resource(extensions);
    let marks = provider_marks(app.world(), view);
    let text: Vec<_> = marks.iter().filter(|m| !m.gutter).collect();
    assert_eq!(text.len(), 2);
    assert_eq!((text[0].provider, text[0].color), (low, red));
    assert_eq!((text[1].provider, text[1].color), (high, blue));
    assert!(marks
        .iter()
        .any(|m| m.gutter && m.label == "Breakpoint beta"));
    assert_eq!(app.world().get::<Decorations>(view).unwrap().list.len(), 1);
    app.world_mut()
        .resource_mut::<SourceExtensions>()
        .unwrap()
        .remove_provider(high);
    assert!(provider_marks(app.world(), view)
        .iter()
        .all(|m| m.provider == low));
}
#[test]
fn cancellation_ime_and_detach_are_checked_without_cached_update_pass() {
    let (mut app, view, _, mut extensions) = boot();
    let p = publish(&app, view, &mut extensions, 0, Color::rgb(0.3, 0.5, 0.7));
    app.world_mut().insert_resource(extensions);
    assert!(!provider_marks(app.world(), view).is_empty());
    app.world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .ime_preedit = "x".into();
    assert!(provider_marks(app.world(), view).is_empty());
    app.world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.world_mut()
        .resource_mut::<SourceExtensions>()
        .unwrap()
        .remove_provider(p);
    assert!(provider_marks(app.world(), view).is_empty());
    detach_source(app.world_mut(), view).unwrap();
    assert!(provider_marks(app.world(), view).is_empty());
}
fn secondary_set(
    app: &mut App,
    view: Entity,
    id: DocumentId,
    ranges: Vec<rndr_core::editing::SourceSelection>,
) {
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let set = rndr_core::source_selection::SourceSelections::new(doc.text(), ranges, 0).unwrap();
    doc.set_view_selections(view, doc.revision(), set).unwrap();
    rndr_core::editing::project_source(app.world_mut(), view);
}
#[test]
fn secondary_marks_are_live_additive_and_refuse_unprojected_or_ime_state() {
    use rndr_core::editing::SourceSelection as S;
    use rndr_core::source_presentation::secondary_selection_marks as marks;
    let (mut app, view, id, _) = boot();
    secondary_set(
        &mut app,
        view,
        id,
        vec![
            S { anchor: 0, head: 0 },
            S { anchor: 2, head: 4 },
            S { anchor: 8, head: 8 },
        ],
    );
    app.update();
    app.update();
    let got = marks(app.world(), view);
    assert!(got.iter().any(|m| m.caret && m.w == 1.0));
    assert!(got.iter().any(|m| !m.caret && m.w > 0.0));
    assert_eq!(app.world().get::<Decorations>(view).unwrap().list.len(), 1);
    app.world_mut().get_mut::<Buffer>(view).unwrap().caret = 1;
    assert!(marks(app.world(), view).is_empty());
    app.world_mut().get_mut::<Buffer>(view).unwrap().caret = 0;
    app.world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .ime_preedit = "x".into();
    assert!(marks(app.world(), view).is_empty());
    app.world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .ime_preedit
        .clear();
    app.world_mut().insert(view, Buffer::from_str("wrong"));
    assert!(marks(app.world(), view).is_empty());
    detach_source(app.world_mut(), view).unwrap();
    assert!(marks(app.world(), view).is_empty());
}
#[test]
fn secondary_marks_refuse_stale_revision_without_an_update_pass() {
    use rndr_core::editing::SourceSelection as S;
    use rndr_core::source_presentation::secondary_selection_marks as marks;
    let (mut app, view, id, _) = boot();
    secondary_set(
        &mut app,
        view,
        id,
        vec![S { anchor: 0, head: 0 }, S { anchor: 2, head: 4 }],
    );
    app.update();
    app.update();
    assert!(!marks(app.world(), view).is_empty());
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    doc.reload(doc.revision(), "changed source").unwrap();
    assert!(marks(app.world(), view).is_empty());
}
#[test]
fn secondary_offscreen_fragments_do_not_attach_to_window_end() {
    use rndr_core::editing::SourceSelection as S;
    use rndr_core::source_presentation::secondary_selection_marks as marks;
    let (mut app, view, _, _) = boot();
    detach_source(app.world_mut(), view).unwrap();
    let text = "line\n".repeat(2000);
    let end = text.len() - 1;
    let id = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open(text);
    attach_source(app.world_mut(), view, id).unwrap();
    secondary_set(&mut app, view, id, vec![S { anchor: 0, head: 0 }]);
    app.update();
    app.update();
    secondary_set(
        &mut app,
        view,
        id,
        vec![
            S { anchor: 0, head: 0 },
            S {
                anchor: end,
                head: end,
            },
        ],
    );
    assert!(marks(app.world(), view).is_empty());
    secondary_set(
        &mut app,
        view,
        id,
        vec![
            S { anchor: 0, head: 0 },
            S {
                anchor: end - 2,
                head: end,
            },
        ],
    );
    assert!(marks(app.world(), view).is_empty());
}
#[test]
fn secondary_hidden_geometry_is_not_reattached_to_seam() {
    use rndr_core::editing::SourceSelection as S;
    use rndr_core::source_presentation::secondary_selection_marks as marks;
    let (mut app, view, id, _) = boot();
    let revision = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    rndr_core::source_folding::set_source_folds(
        app.world_mut(),
        view,
        id,
        revision,
        one_fold(6..11),
    )
    .unwrap();
    app.update();
    app.update();
    // Publish after layout: preparing folds on the next pass deliberately expands
    // hidden endpoints, but paint must not temporarily attach them to the seam.
    secondary_set(
        &mut app,
        view,
        id,
        vec![S { anchor: 0, head: 0 }, S { anchor: 7, head: 9 }],
    );
    assert!(marks(app.world(), view).is_empty());
    secondary_set(
        &mut app,
        view,
        id,
        vec![S { anchor: 0, head: 0 }, S { anchor: 8, head: 8 }],
    );
    assert!(marks(app.world(), view).is_empty());
}
#[test]
fn hidden_annotations_do_not_reattach_to_fold_seam() {
    let (mut app, view, id, mut extensions) = boot();
    let revision = app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    rndr_core::source_folding::set_source_folds(
        app.world_mut(),
        view,
        id,
        revision,
        one_fold(6..11),
    )
    .unwrap();
    app.update();
    app.update();
    let p = extensions.register(0);
    let (ticket, _) = extensions
        .request(app.world(), p, view, RequestKind::Decoration)
        .unwrap();
    extensions
        .publish_decorations(
            app.world(),
            &ticket,
            vec![SourceDecoration {
                range: 6..10,
                color: Color::rgb(0.5, 0.5, 0.8),
                class: "hidden".into(),
            }],
            vec![GutterMarker {
                at: 6,
                label: "hidden".into(),
            }],
        )
        .unwrap();
    app.world_mut().insert_resource(extensions);
    assert!(provider_marks(app.world(), view).is_empty());
}
