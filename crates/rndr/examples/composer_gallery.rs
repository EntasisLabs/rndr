//! Headless pixels for a structured composer: inline marks, a link, mention
//! pills, and a list pasted from HTML. Writes one PNG; no window.
//!
//! `cargo run -p rndr --example composer_gallery -- OUTPUT.png`
use std::sync::Arc;

use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{MarkKind, StructuredDocument, StructuredSelection, TextSelection};
use rndr_core::structured_native::{attach_structured, StructuredDocuments};
use rndr_core::{
    set_parent, App, Buffer, Color, Editor, Label, Layout, Size, Spans, TextEngine, TextStyle,
    Viewport,
};
use rndr_render::{encode_png, Exporter, ParleyText};

const W: u32 = 720;
const H: u32 = 260;
const ORIGIN: EditOrigin = EditOrigin::Host(0);

fn select(doc: &mut StructuredDocument, anchor: usize, head: usize) {
    let anchor = doc.position_at(anchor, Affinity::After).unwrap();
    let head = doc.position_at(head, Affinity::After).unwrap();
    doc.set_selection(StructuredSelection::Text(TextSelection { anchor, head }))
        .unwrap();
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: composer_gallery OUTPUT.png")?;
    let mut doc = StructuredDocument::new("Ship bold italic underline struck code, ");
    for (start, end, mark) in [
        (5, 9, MarkKind::Strong),
        (10, 16, MarkKind::Emphasis),
        (17, 26, MarkKind::Underline),
        (27, 33, MarkKind::Strike),
        (34, 38, MarkKind::Code),
    ] {
        select(&mut doc, start, end);
        doc.toggle_mark(mark, ORIGIN)
            .map_err(|e| format!("{e:?}"))?;
    }
    let end = doc.plain_text().len();
    select(&mut doc, end, end);
    doc.insert_pill("@alice", "user:alice", ORIGIN)
        .map_err(|e| format!("{e:?}"))?;
    doc.replace_selection(" and ", ORIGIN)
        .map_err(|e| format!("{e:?}"))?;
    doc.insert_pill("#design", "channel:design", ORIGIN)
        .map_err(|e| format!("{e:?}"))?;
    let end = doc.plain_text().len();
    select(&mut doc, end, end);
    doc.enter(ORIGIN).map_err(|e| format!("{e:?}"))?;
    doc.paste_html(
        "<p>Pasted HTML: <a href=\"https://example.com\">a link</a></p>\
         <ul><li><b>bold</b> item</li><li>item with <span data-pill=\"user:bo\">@bo</span></li></ul>",
        ORIGIN,
    )
    .map_err(|e| format!("{e:?}"))?;

    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let mut docs = StructuredDocuments::default();
    let handle = docs.insert(doc);
    app.world_mut().insert_resource(docs);
    // The structured view projects the canonical tree into text and spans;
    // a plain label shows that projection without an interactive editor.
    let editor = app.world_mut().spawn((
        Layout {
            width: Size::Px(0.0),
            height: Size::Px(0.0),
            ..Layout::column()
        },
        Buffer::from_str(""),
        Editor::default(),
        Label::new(""),
    ));
    attach_structured(app.world_mut(), editor, handle).map_err(|e| format!("{e:?}"))?;
    let text = app.world().get::<Buffer>(editor).unwrap().text();
    let spans = app.world().get::<Spans>(editor).unwrap().clone();
    let page = app.world_mut().spawn((
        Layout {
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            padding: 20.0,
            ..Layout::column()
        },
        Color::rgb(0.11, 0.12, 0.15),
    ));
    let label = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(text).with_color(Color::rgb(0.92, 0.93, 0.95)),
        TextStyle {
            family: "Inter".into(),
            size: 20.0,
            line_height: Some(1.5),
            ..TextStyle::default()
        },
        spans,
    ));
    set_parent(app.world_mut(), label, page);

    let frame = rndr_paint::frame(&mut app);
    let mut exporter = Exporter::new()?;
    let image = exporter.render_frame(Some(app.world()), &frame, W * 2, H * 2, 2.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
