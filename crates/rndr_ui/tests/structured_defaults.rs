use rndr_core::structured::{AtomicPayload, StructuredDocument};
use rndr_core::structured_chrome::{chrome_palette, StructuredChromeStyle};
use rndr_core::structured_native::StructuredDocuments;
use rndr_core::*;
use rndr_ui::structured::{StructuredCommand, StructuredSurface, StructuredSurfaceProps};
use rndr_ui::structured_embeds::AtomicPanelHandle;
use rndr_ui::*;

#[test]
fn document_and_retained_child_defaults_follow_light_dark_and_high_contrast() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(640.0, 600.0));
    let mut docs = StructuredDocuments::default();
    let document = docs.insert(StructuredDocument::new("before\nafter"));
    app.world_mut().insert_resource(docs);
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let view = StructuredSurface::mount(
        app.world_mut(),
        root,
        UiId(1),
        StructuredSurfaceProps::new(document, "Document"),
    )
    .unwrap();
    view.command(
        app.world_mut(),
        StructuredCommand::Atomic(AtomicPayload {
            kind: "embedded".into(),
            fallback: "A retained draft".into(),
            attributes: Default::default(),
        }),
    )
    .unwrap();
    let block = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(document)
        .unwrap()
        .blocks()
        .iter()
        .find(|b| b.atomic_payload().is_some())
        .unwrap()
        .id;
    let panel =
        AtomicPanelHandle::mount(app.world_mut(), root, UiId(2), view.editor, block).unwrap();
    let revision = app
        .world()
        .resource::<StructuredDocuments>()
        .unwrap()
        .get(document)
        .unwrap()
        .revision();
    for (appearance, contrast) in [
        (Appearance::Light, Contrast::Standard),
        (Appearance::Dark, Contrast::Standard),
        (Appearance::Light, Contrast::High),
        (Appearance::Dark, Contrast::High),
    ] {
        let theme = UiTheme {
            appearance,
            contrast,
            ..Default::default()
        };
        apply_theme_bridge(app.world_mut(), theme.clone());
        app.update();
        app.update();
        let palette = chrome_palette(app.world(), view.editor);
        assert_eq!(
            palette.border,
            theme.color(if contrast == Contrast::High {
                Token::StrongStroke
            } else {
                Token::SubtleStroke
            })
        );
        assert_eq!(palette.surface, theme.color(Token::Canvas));
        assert_eq!(palette.header, theme.color(Token::ControlHover));
        assert_eq!(palette.accent, theme.color(Token::Focus));
        assert_eq!(
            *app.world().get::<Color>(panel.root).unwrap(),
            theme.color(Token::Raised)
        );
        assert_eq!(
            *app.world().get::<Color>(panel.editor).unwrap(),
            theme.color(Token::Content)
        );
        assert_eq!(
            app.world().get::<Label>(panel.editor).unwrap().color,
            Some(theme.color(Token::Text))
        );
        assert_eq!(
            app.world().get::<TextStyle>(panel.editor).unwrap(),
            &theme.typography(Typography::Reading)
        );
        assert_eq!(
            AtomicPanelHandle::mount(app.world_mut(), root, UiId(2), view.editor, block)
                .unwrap()
                .editor,
            panel.editor
        );
        assert_eq!(
            app.world().get::<Buffer>(panel.editor).unwrap().text(),
            "A retained draft"
        );
        assert_eq!(
            app.world()
                .resource::<StructuredDocuments>()
                .unwrap()
                .get(document)
                .unwrap()
                .revision(),
            revision
        );
    }
    let authored = Color::rgb(0.7, 0.2, 0.3);
    app.world_mut().insert(
        view.editor,
        StructuredChromeStyle {
            border: Some(authored),
            ..Default::default()
        },
    );
    apply_theme_bridge(app.world_mut(), UiTheme::default());
    app.update();
    assert_eq!(chrome_palette(app.world(), view.editor).border, authored);
}
