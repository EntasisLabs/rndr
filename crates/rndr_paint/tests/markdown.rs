//! Headless paint of a short markdown document.

use rndr_core::{markdown_view, App, MarkdownPlugin, MarkdownView, Viewport};
use rndr_paint::{frame, quads_snapshot};

#[test]
fn markdown_document_paints_heading_list_quote_rule_and_code() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(420.0, 560.0));
    let view = markdown_view(
        app.world_mut(),
        "# Title\n\nHello **world**.\n\n- one\n  - two\n\n> quoted\n\n---\n\n```\nlet x = 1;\n```\n",
    );
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .set_follow(false);
    let painted = frame(&mut app);
    // `highlight` recolors `let` and `1`. The default build keeps one code color.
    #[cfg(not(feature = "highlight"))]
    insta::assert_snapshot!(quads_snapshot(&painted));
    #[cfg(feature = "highlight")]
    insta::assert_snapshot!("markdown_document_code_highlight", quads_snapshot(&painted));
}

#[test]
fn selected_link_text_uses_the_label_color() {
    let mut app = App::new();
    app.add_plugin(MarkdownPlugin);
    app.world_mut().insert_resource(Viewport::new(480.0, 200.0));
    let view = markdown_view(
        app.world_mut(),
        "See [the notes](https://example.com) for the source.",
    );
    let plain = frame(&mut app);
    let link = rndr_core::MarkdownTheme::default().link;
    assert!(
        plain.quads.iter().any(|quad| quad.color == link),
        "an unselected link keeps its color"
    );
    app.world_mut()
        .get_mut::<MarkdownView>(view)
        .unwrap()
        .select_all();
    let selected = frame(&mut app);
    assert!(
        selected.quads.iter().all(|quad| quad.color != link),
        "selected link glyphs and underlines use the label color"
    );
}
