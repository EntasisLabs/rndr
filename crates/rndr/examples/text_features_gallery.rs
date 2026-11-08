//! Headless pixels for OpenType features, strikethrough and a read-only
//! selection. Writes one PNG; no window.
//!
//! `cargo run -p rndr --example text_features_gallery -- OUTPUT.png`
use std::sync::Arc;

use rndr_core::{
    set_parent, App, Axis, Color, Entity, FontFeature, Label, Layout, Selectable, Size, Spans,
    TextEngine, TextSpan, TextStyle, Viewport,
};
use rndr_render::{encode_png, Exporter, ParleyText};

const W: u32 = 720;
const H: u32 = 300;

fn label(app: &mut App, parent: Entity, text: &str, style: TextStyle) -> Entity {
    let entity = app.world_mut().spawn((
        Layout {
            width: Size::Px(330.0),
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(text).with_color(Color::rgb(0.1, 0.11, 0.14)),
        style,
    ));
    set_parent(app.world_mut(), entity, parent);
    entity
}

fn row(app: &mut App, parent: Entity) -> Entity {
    let entity = app.world_mut().spawn(Layout {
        axis: Axis::Row,
        gap: 20.0,
        width: Size::Fill,
        height: Size::Hug,
        ..Layout::row()
    });
    set_parent(app.world_mut(), entity, parent);
    entity
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: text_features_gallery OUTPUT.png")?;
    let mut app = App::new();
    let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::bundled());
    app.world_mut().insert_resource(engine);
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let page = app.world_mut().spawn((
        Layout {
            axis: Axis::Column,
            gap: 8.0,
            padding: 20.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::column()
        },
        Color::rgb(0.96, 0.96, 0.97),
    ));
    let base = TextStyle {
        family: "Inter".into(),
        size: 22.0,
        ..TextStyle::default()
    };

    // Proportional against tabular figures: the columns only line up with tnum.
    let figures = row(&mut app, page);
    let proportional = base.clone().feature(FontFeature::on(b"pnum"));
    let tabular = base.clone().feature(FontFeature::on(b"tnum"));
    label(&mut app, figures, "pnum 1111\n       8080", proportional);
    label(&mut app, figures, "tnum 1111\n       8080", tabular);

    // Character variants: Inter's cv11 swaps in a single-storey a.
    let variants = row(&mut app, page);
    label(&mut app, variants, "default: banana", base.clone());
    label(
        &mut app,
        variants,
        "cv11: banana",
        base.clone().feature(FontFeature::on(b"cv11")),
    );

    // Strikethrough and underline on ranges.
    let marks = row(&mut app, page);
    let price = label(&mut app, marks, "Was $40, now $25", base.clone());
    app.world_mut().insert(
        price,
        Spans {
            runs: vec![
                TextSpan::new(4, 7, Color::rgb(0.75, 0.15, 0.2)).strike(),
                TextSpan::new(13, 16, Color::rgb(0.1, 0.45, 0.25)).underline(),
            ],
        },
    );

    // A read-only label with a selection, as a drag or double-click leaves it.
    let selectable = label(&mut app, marks, "Select these words", base);
    app.world_mut().insert(
        selectable,
        Selectable {
            anchor: 7,
            head: 12,
        },
    );

    let frame = rndr_paint::frame(&mut app);
    let mut exporter = Exporter::new()?;
    let image = exporter.render_frame(Some(app.world()), &frame, W * 2, H * 2, 2.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
