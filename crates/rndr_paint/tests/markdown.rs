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
    insta::assert_snapshot!(quads_snapshot(&painted));
}
