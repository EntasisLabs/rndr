//! Headless pixels for layout options: main-axis justify, cross-axis items,
//! wrapping with a max width, absolute children and aspect ratio.
//! Writes one PNG; no window.
//!
//! `cargo run -p rndr --example layout_gallery -- OUTPUT.png`
use rndr_core::{
    set_parent, Absolute, Align, App, AspectRatio, Axis, Color, Entity, Items, Justify, Layout,
    MaxSize, Padding, Radius, Size, Viewport, Wrap,
};
use rndr_render::{encode_png, Exporter};

const W: u32 = 1080;
const H: u32 = 300;

fn node(app: &mut App, parent: Entity, layout: Layout, fill: Color) -> Entity {
    let e = app.world_mut().spawn((layout, fill, Radius(6.0)));
    set_parent(app.world_mut(), e, parent);
    e
}

fn px(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::column()
    }
}

fn panel(app: &mut App, root: Entity, axis: Axis) -> Entity {
    let panel = node(
        app,
        root,
        Layout {
            axis,
            gap: 8.0,
            padding: 12.0,
            width: Size::Px(196.0),
            height: Size::Fill,
            ..Layout::column()
        },
        Color::rgb(1.0, 1.0, 1.0),
    );
    app.world_mut().insert(panel, Radius(12.0));
    panel
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: layout_gallery OUTPUT.png")?;
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

    // Justify: one track per mode.
    let justify = panel(&mut app, root, Axis::Column);
    for mode in [
        Justify::Start,
        Justify::Center,
        Justify::End,
        Justify::SpaceBetween,
        Justify::SpaceAround,
        Justify::SpaceEvenly,
    ] {
        let track = node(
            &mut app,
            justify,
            Layout {
                gap: 4.0,
                width: Size::Fill,
                height: Size::Px(32.0),
                ..Layout::row()
            },
            soft,
        );
        app.world_mut().insert(
            track,
            Align {
                justify: mode,
                items: Items::Center,
            },
        );
        for _ in 0..3 {
            node(&mut app, track, px(24.0, 24.0), ink);
        }
    }

    // Items: start, center, end, stretch across a row.
    let items = panel(&mut app, root, Axis::Column);
    for mode in [Items::Start, Items::Center, Items::End, Items::Stretch] {
        let track = node(
            &mut app,
            items,
            Layout {
                gap: 6.0,
                width: Size::Fill,
                height: Size::Px(56.0),
                ..Layout::row()
            },
            soft,
        );
        app.world_mut().insert(
            track,
            Align {
                justify: Justify::Center,
                items: mode,
            },
        );
        for h in [16.0, 32.0, 24.0] {
            let chip = Layout {
                height: if mode == Items::Stretch {
                    Size::Hug
                } else {
                    Size::Px(h)
                },
                ..px(28.0, h)
            };
            node(&mut app, track, chip, ink);
        }
    }

    // Wrap: tags breaking onto lines inside a max width.
    let wrap = panel(&mut app, root, Axis::Column);
    let cloud = node(
        &mut app,
        wrap,
        Layout {
            gap: 6.0,
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::row()
        },
        soft,
    );
    let world = app.world_mut();
    world.insert(cloud, Wrap { line_gap: 6.0 });
    world.insert(cloud, MaxSize::width(150.0));
    world.insert(cloud, Padding::all(6.0));
    for w in [40.0, 64.0, 28.0, 52.0, 36.0, 70.0, 30.0, 44.0, 58.0] {
        node(&mut app, cloud, px(w, 22.0), ink);
    }

    // Absolute: a badge and a floating button pinned inside a card.
    let absolute = panel(&mut app, root, Axis::Column);
    let card = node(
        &mut app,
        absolute,
        Layout {
            gap: 8.0,
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        soft,
    );
    app.world_mut().insert(card, Padding::all(12.0));
    for w in [120.0, 90.0, 140.0] {
        node(&mut app, card, px(w, 14.0), Color::rgb(0.65, 0.7, 0.82));
    }
    let badge = node(&mut app, card, px(22.0, 22.0), warm);
    let world = app.world_mut();
    world.insert(badge, Radius(11.0));
    world.insert(
        badge,
        Absolute {
            top: Some(10.0),
            right: Some(10.0),
            ..Absolute::default()
        },
    );
    let fab = node(&mut app, card, px(44.0, 44.0), ink);
    let world = app.world_mut();
    world.insert(fab, Radius(22.0));
    world.insert(
        fab,
        Absolute {
            right: Some(12.0),
            bottom: Some(12.0),
            ..Absolute::default()
        },
    );

    // Aspect ratio: 16:9 and 1:1 boxes sized from their width.
    let aspect = panel(&mut app, root, Axis::Column);
    let wide = node(
        &mut app,
        aspect,
        Layout {
            width: Size::Fill,
            height: Size::Hug,
            ..Layout::column()
        },
        ink,
    );
    app.world_mut().insert(wide, AspectRatio(16.0 / 9.0));
    let square = node(
        &mut app,
        aspect,
        Layout {
            width: Size::Px(80.0),
            height: Size::Hug,
            ..Layout::column()
        },
        warm,
    );
    app.world_mut().insert(square, AspectRatio(1.0));

    let frame = rndr_paint::frame(&mut app);
    let image = Exporter::new()?.render_frame(Some(app.world()), &frame, W, H, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
