//! Headless pixels for grid layout, sticky section headers and a horizontally
//! scrolled strip. Writes one PNG; no window.
//!
//! `cargo run -p rndr --example grid_scroll_gallery -- OUTPUT.png`
use rndr_core::{
    set_parent, App, Axis, Color, Entity, Grid, GridSpan, Layout, Radius, Scroll, Size, Sticky,
    Track, Viewport,
};
use rndr_render::{encode_png, Exporter};

const W: u32 = 1080;
const H: u32 = 300;

fn node(app: &mut App, parent: Entity, layout: Layout, fill: Color) -> Entity {
    let e = app.world_mut().spawn((layout, fill, Radius(6.0)));
    set_parent(app.world_mut(), e, parent);
    e
}

fn sized(w: Size, h: Size) -> Layout {
    Layout {
        width: w,
        height: h,
        ..Layout::column()
    }
}

fn panel(app: &mut App, root: Entity, width: f32, scroll: [f32; 2]) -> Entity {
    let panel = node(
        app,
        root,
        Layout {
            gap: 8.0,
            padding: 12.0,
            width: Size::Px(width),
            height: Size::Fill,
            ..Layout::column()
        },
        Color::rgb(1.0, 1.0, 1.0),
    );
    let world = app.world_mut();
    world.insert(panel, Radius(12.0));
    world.insert(
        panel,
        Scroll {
            offset: scroll,
            content: [0.0, 0.0],
        },
    );
    panel
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: grid_scroll_gallery OUTPUT.png")?;
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let root = app.world_mut().spawn((
        Layout {
            axis: Axis::Row,
            gap: 16.0,
            padding: 20.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::row()
        },
        Color::rgb(0.92, 0.93, 0.95),
    ));
    let ink = Color::rgb(0.25, 0.35, 0.8);
    let warm = Color::rgb(0.95, 0.55, 0.3);
    let soft = Color::rgb(0.82, 0.86, 0.95);

    // Grid: a fixed sidebar column and two fractional columns, with spans.
    let grid = panel(&mut app, root, 340.0, [0.0, 0.0]);
    app.world_mut().insert(
        grid,
        Grid {
            columns: vec![Track::Px(60.0), Track::Fr(1.0), Track::Fr(2.0)],
            column_gap: 8.0,
            row_gap: 8.0,
        },
    );
    for (i, span) in [1, 1, 1, 2, 1, 1, 1, 1, 3].into_iter().enumerate() {
        let fill = if span > 1 { warm } else { ink };
        let h = if i % 4 == 1 { 70.0 } else { 50.0 };
        let cell = node(&mut app, grid, sized(Size::Fill, Size::Px(h)), fill);
        app.world_mut().insert(cell, GridSpan(span));
    }

    // Sticky: section headers in a list scrolled partway; the first header
    // stays pinned while its rows scroll under it.
    let list = panel(&mut app, root, 300.0, [0.0, -100.0]);
    for _ in 0..3 {
        let section = node(
            &mut app,
            list,
            Layout {
                gap: 6.0,
                ..sized(Size::Fill, Size::Hug)
            },
            Color::rgba(0.0, 0.0, 0.0, 0.0),
        );
        let header = node(&mut app, section, sized(Size::Fill, Size::Px(28.0)), warm);
        app.world_mut().insert(header, Sticky::top(8.0));
        for _ in 0..4 {
            node(&mut app, section, sized(Size::Fill, Size::Px(30.0)), soft);
        }
    }

    // Horizontal scroll: a filmstrip offset to the left inside its port.
    let film = panel(&mut app, root, 340.0, [0.0, 0.0]);
    let strip = node(
        &mut app,
        film,
        Layout {
            gap: 8.0,
            ..Layout {
                axis: Axis::Row,
                ..sized(Size::Fill, Size::Px(120.0))
            }
        },
        soft,
    );
    app.world_mut().insert(
        strip,
        Scroll {
            offset: [-70.0, 0.0],
            content: [0.0, 0.0],
        },
    );
    for i in 0..8 {
        let fill = if i % 2 == 0 { ink } else { warm };
        node(&mut app, strip, sized(Size::Px(90.0), Size::Fill), fill);
    }

    let frame = rndr_paint::frame(&mut app);
    let image = Exporter::new()?.render_frame(Some(app.world()), &frame, W, H, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
