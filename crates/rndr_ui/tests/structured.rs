use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    BlockKind, Marks, StructuredDocument, StructuredSelection, TextSelection,
};
use rndr_core::structured_native::*;
use rndr_core::*;
use rndr_ui::structured::*;
use rndr_ui::{UiId, UiPlugin};
fn setup() -> (App, Entity, StructuredSurfaceHandle, StructuredHandle) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 480.0));
    let mut docs = StructuredDocuments::default();
    let id = docs.insert(StructuredDocument::new("abcdef\nnext"));
    app.world_mut().insert_resource(docs);
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let view = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(10),
        StructuredSurfaceProps::new(id, "Tree editor"),
    )
    .unwrap();
    app.world_mut().insert_resource(Focus {
        entity: Some(view.editor),
    });
    app.update();
    (app, parent, view, id)
}
fn select(app: &mut App, view: StructuredSurfaceHandle, a: usize, h: usize) {
    let r = app
        .world()
        .get::<StructuredBinding>(view.editor)
        .unwrap()
        .revision;
    set_display_selection(app.world_mut(), view.editor, r, a, h).unwrap();
}
#[test]
fn structured_toolbar_keeps_directed_saved_selection_and_additive_marks() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 5, 1);
    let before = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .selection();
    app.world_mut().insert_resource(Focus { entity: None });
    for command in [
        StructuredCommand::Strong,
        StructuredCommand::Emphasis,
        StructuredCommand::Link(Some("https://example.com".into())),
    ] {
        view.command(app.world_mut(), command).unwrap();
    }
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.selection(), before);
    let marked = &doc.blocks()[0].runs[1].marks;
    assert!(marked.strong && marked.emphasis && marked.link.is_some());
    assert!(app
        .world()
        .get::<Spans>(view.editor)
        .unwrap()
        .runs
        .iter()
        .any(|s| s.italic && s.underline && s.weight == Some(700)));
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    let marked = &app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .blocks()[0]
        .runs[1]
        .marks;
    assert!(!marked.strong && marked.emphasis && marked.link.is_some());
}
#[test]
fn structured_invalid_link_and_empty_selection_refuse_without_history() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 2);
    let r = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap()
        .revision();
    assert!(view
        .command(
            app.world_mut(),
            StructuredCommand::Link(Some("javascript:alert(1)".into()))
        )
        .is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .revision(),
        r
    );
    select(&mut app, view, 1, 1);
    assert_eq!(
        view.command(app.world_mut(), StructuredCommand::Strong),
        Err(StructuredSurfaceError::SelectText)
    );
}
#[test]
fn structured_toolbar_refuses_active_and_new_native_preedit() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 1, 4);
    app.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "仮".into();
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    app.update();
    assert!(is_composing(app.world(), view.editor));
    app.world_mut().insert_resource(Focus { entity: None });
    app.update();
    assert!(view
        .command(
            app.world_mut(),
            StructuredCommand::Block(BlockKind::Heading(2))
        )
        .is_err());
    assert!(view.destroy(app.world_mut()).is_err());
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .plain_text(),
        "abcdef\nnext"
    );
}
#[test]
fn structured_retained_identity_teardown_and_document_history_survive() {
    let (mut app, parent, view, id) = setup();
    app.world_mut()
        .get_mut::<Semantics>(view.editor)
        .unwrap()
        .readonly = true;
    let same = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(10),
        StructuredSurfaceProps::new(id, "Renamed tree"),
    )
    .unwrap();
    assert_eq!(view, same);
    assert!(app.world().get::<Semantics>(view.editor).unwrap().readonly);
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    app.world_mut()
        .get_mut::<Semantics>(view.editor)
        .unwrap()
        .readonly = false;
    select(&mut app, view, 0, 3);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    let borrowed = app.world_mut().spawn((Layout::column(),));
    set_parent(app.world_mut(), borrowed, view.root);
    view.destroy(app.world_mut()).unwrap();
    assert!(app.world().is_alive(borrowed));
    assert!(app.world().get::<Parent>(borrowed).is_none());
    assert!(!app.world().is_alive(view.editor));
    assert!(view.validate(app.world()).is_err());
    let next = StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(11),
        StructuredSurfaceProps::new(id, "Remount"),
    )
    .unwrap();
    assert_ne!(next.editor, view.editor);
    next.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .blocks()[0]
            .runs[0]
            .marks,
        Marks::default()
    );
}
#[test]
fn structured_second_view_refusal_does_not_rebind_existing_history() {
    let (mut app, parent, view, id) = setup();
    assert!(StructuredSurface::mount(
        app.world_mut(),
        parent,
        UiId(20),
        StructuredSurfaceProps::new(id, "Second")
    )
    .is_err());
    view.validate(app.world()).unwrap();
    assert_eq!(attached_structured(app.world(), view.editor), Some(id));
}
#[test]
fn structured_readonly_toolbar_and_native_advertising_agree() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 3);
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_readonly(true);
    app.update();
    assert!(access_details(app.world(), view.editor).readonly);
    assert!(view
        .command(app.world_mut(), StructuredCommand::Strong)
        .is_err());
    assert!(view
        .command(app.world_mut(), StructuredCommand::Undo)
        .is_err());
    view.command(app.world_mut(), StructuredCommand::SelectBlock)
        .unwrap();
    assert!(matches!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .selection(),
        StructuredSelection::Node(_)
    ));
}
#[test]
fn structured_first_frame_spans_match_resolved_foreground_and_theme_changes() {
    let (mut app, _, view, _) = setup();
    for appearance in [rndr_ui::Appearance::Light, rndr_ui::Appearance::Dark] {
        rndr_ui::apply_theme_bridge(
            app.world_mut(),
            rndr_ui::UiTheme {
                appearance,
                ..Default::default()
            },
        );
        app.update();
        let foreground = app
            .world()
            .get::<Label>(view.editor)
            .unwrap()
            .color
            .unwrap();
        let background = *app.world().get::<Color>(view.editor).unwrap();
        assert_ne!(
            foreground, background,
            "glyph foreground is not an entity fill"
        );
        assert_eq!(
            app.world().get::<Spans>(view.editor).unwrap().runs[0].color,
            foreground
        );
    }
}
#[test]
fn structured_generic_tree_snapshot_is_not_markdown_codec() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 6);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    view.command(
        app.world_mut(),
        StructuredCommand::Block(BlockKind::Heading(2)),
    )
    .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    let generic = format!("{:?}", doc.blocks());
    assert!(generic.contains("Heading(2)") && generic.contains("strong: true"));
    assert_eq!(doc.plain_text(), "abcdef\nnext");
    assert!(!doc.plain_text().contains("**"));
}
#[test]
fn structured_formatting_and_keyboard_share_exactly_one_history() {
    let (mut app, _, view, id) = setup();
    select(&mut app, view, 0, 3);
    view.command(app.world_mut(), StructuredCommand::Strong)
        .unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().chars = "é".into();
    app.update();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .plain_text(),
        "édef\nnext"
    );
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    let doc = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(id)
        .unwrap();
    assert_eq!(doc.plain_text(), "abcdef\nnext");
    assert!(doc.blocks()[0].runs[0].marks.strong);
    view.command(app.world_mut(), StructuredCommand::Undo)
        .unwrap();
    assert_eq!(
        app.world()
            .resource::<StructuredDocuments>()
            .unwrap()
            .get(id)
            .unwrap()
            .blocks()[0]
            .runs[0]
            .marks,
        Marks::default()
    );
}
#[test]
fn structured_external_typed_selection_and_block_navigation_contract() {
    let (mut app, _, view, id) = setup();
    let doc = app
        .world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let a = doc.position_at(7, Affinity::After).unwrap();
    let h = doc.position_at(11, Affinity::After).unwrap();
    doc.set_selection(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: h,
    }))
    .unwrap();
    project(app.world_mut(), view.editor);
    let display = structured_display(app.world(), view.editor).unwrap();
    assert_eq!(display.blocks[1].range, 7..11);
    assert_eq!(
        display.selection,
        StructuredSelection::Text(TextSelection { anchor: a, head: h })
    );
    app.world_mut()
        .resource_mut::<StructuredDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap()
        .set_block_kind(BlockKind::Heading(3), EditOrigin::Host(1))
        .unwrap();
    assert!(structured_display(app.world(), view.editor).is_none());
}
