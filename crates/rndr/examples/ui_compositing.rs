//! Headless pixels for UI compositing: semantic cards under group opacity,
//! scale, rotation, and translation. Writes one PNG; no window.
//!
//! `cargo run -p rndr --example ui_compositing -- OUTPUT.png`
use rndr_core::{set_parent, App, Axis, Color, Entity, Layout, Opacity, Size, Transform, Viewport};
use rndr_render::{encode_png, Exporter};
use rndr_ui::controls::{Button, ButtonProps, HostIntent};
use rndr_ui::{Card, Typography, UiId, UiPlugin};

fn card(app: &mut App, row: Entity, id: u64, title: &str, body: &str) -> Entity {
    let host = app.world_mut().spawn((Layout {
        width: Size::Px(220.0),
        height: Size::Px(150.0),
        ..Layout::column()
    },));
    set_parent(app.world_mut(), host, row);
    let card = Card::mount(app.world_mut(), host, UiId(id), title).unwrap();
    rndr_ui::text(app.world_mut(), card.content, body, Typography::Label).unwrap();
    Button::mount(
        app.world_mut(),
        card.content,
        UiId(id + 100),
        ButtonProps::new("Open", HostIntent::Command(id)),
    )
    .unwrap();
    host
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: ui_compositing OUTPUT.png")?;
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(760.0, 240.0));
    #[cfg(feature = "parley")]
    {
        let engine: std::sync::Arc<dyn rndr_core::TextEngine> =
            std::sync::Arc::new(rndr_render::ParleyText::bundled());
        app.world_mut().insert_resource(engine);
    }
    let row = app.world_mut().spawn((
        Layout {
            axis: Axis::Row,
            gap: 24.0,
            padding: 24.0,
            width: Size::Px(760.0),
            height: Size::Px(240.0),
            ..Layout::row()
        },
        Color::rgb(0.93, 0.94, 0.96),
    ));
    let faded = card(&mut app, row, 1, "Opacity 0.5", "Composited once.");
    let scaled = card(&mut app, row, 2, "Scale + rotate", "Hit testing follows.");
    let moved = card(&mut app, row, 3, "Translate", "Siblings keep layout.");
    app.world_mut().insert(faded, Opacity(0.5));
    app.world_mut().insert(
        scaled,
        Transform {
            scale: [1.08, 1.08],
            rotate: -0.08,
            ..Transform::IDENTITY
        },
    );
    app.world_mut()
        .insert(moved, Transform::translate(0.0, 30.0));
    let frame = rndr_paint::frame(&mut app);
    let image = Exporter::new()?.render_frame(Some(app.world()), &frame, 760, 240, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
