use rndr_core::editing::{attach_source, SourceDocuments};
use rndr_core::source_extensions::{RequestKind, SourceDecoration, SourceExtensions};
use rndr_core::{App, Buffer, Chrome, Color, Editor, Label, Layout, Size, Viewport};

#[test]
fn secondary_backgrounds_precede_glyphs_and_collapsed_carets_reach_paint() {
    use rndr_core::editing::{project_source, SourceSelection as S};
    use rndr_core::source_selection::SourceSelections;
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let mut docs = SourceDocuments::default();
    let id = docs.open("alpha\nbeta\ngamma");
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(180.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str(""),
        Editor::default(),
    ));
    attach_source(app.world_mut(), view, id).unwrap();
    let doc = app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .get_mut(id)
        .unwrap();
    let selections = SourceSelections::new(
        doc.text(),
        vec![
            S { anchor: 0, head: 0 },
            S { anchor: 2, head: 4 },
            S { anchor: 8, head: 8 },
        ],
        0,
    )
    .unwrap();
    doc.set_view_selections(view, doc.revision(), selections)
        .unwrap();
    project_source(app.world_mut(), view);
    app.update();
    app.update();
    let theme = rndr_core::theme_of(app.world());
    let quads = rndr_paint::paint(app.world());
    let background = quads
        .iter()
        .position(|q| q.color == theme.selection && q.rect.w > 1.0)
        .expect("secondary background");
    let glyph = quads
        .iter()
        .position(|q| q.color == theme.text && q.rect.w > 1.0)
        .expect("text glyph");
    assert!(background < glyph, "selection must never cover glyphs");
    assert!(quads
        .iter()
        .any(|q| q.color == theme.caret && q.rect.w == 1.0));
    rndr_core::editing::detach_source(app.world_mut(), view).unwrap();
    assert!(!rndr_paint::paint(app.world())
        .iter()
        .any(|q| q.color == theme.selection));
}

#[test]
fn generic_provider_color_reaches_standard_paint_and_retirement_removes_it() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(300.0, 200.0));
    let mut docs = SourceDocuments::default();
    let document = docs.open("hello provider");
    app.world_mut().insert_resource(docs);
    let view = app.world_mut().spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(180.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str("hello provider"),
        Editor::default(),
    ));
    attach_source(app.world_mut(), view, document).unwrap();
    app.update();
    app.update();
    let mut extensions = SourceExtensions::default();
    let provider = extensions.register(1);
    let (ticket, _) = extensions
        .request(app.world(), provider, view, RequestKind::Decoration)
        .unwrap();
    let color = Color::rgba(0.123, 0.456, 0.789, 0.7);
    extensions
        .publish_decorations(
            app.world(),
            &ticket,
            vec![SourceDecoration {
                range: 0..5,
                color,
                class: "custom".into(),
            }],
            vec![],
        )
        .unwrap();
    app.world_mut().insert_resource(extensions);
    assert!(rndr_paint::paint(app.world())
        .iter()
        .any(|q| q.color == color && q.rect.w > 0.0));
    app.world_mut()
        .resource_mut::<SourceExtensions>()
        .unwrap()
        .remove_provider(provider);
    assert!(!rndr_paint::paint(app.world())
        .iter()
        .any(|q| q.color == color));
}
