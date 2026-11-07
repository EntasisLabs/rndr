//! Headless pixels for SVG icons and rounded, feathered clips. Writes one
//! PNG; no window.
//!
//! `cargo run -p rndr --example svg_clip_gallery -- OUTPUT.png`
use rndr_core::{
    set_parent, App, Axis, ClipRounded, Color, Entity, ImageView, Layout, Size, Viewport,
};
use rndr_render::{decode_svg, encode_png, load_rgba, Exporter};

const W: u32 = 760;
const H: u32 = 220;

const STAR: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24">
  <defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="#ffb000"/><stop offset="1" stop-color="#ff4060"/>
  </linearGradient></defs>
  <path fill="url(#g)" stroke="#402060" stroke-width="1" stroke-linejoin="round"
    d="M12 2l3 6.5 7 .8-5.2 4.8 1.5 7L12 17.6 5.7 21.1l1.5-7L2 9.3l7-.8z"/>
</svg>"##;

fn block(app: &mut App, parent: Entity, w: f32, h: f32, axis: Axis) -> Entity {
    let entity = app.world_mut().spawn(Layout {
        axis,
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::row()
    });
    set_parent(app.world_mut(), entity, parent);
    entity
}

fn fill(app: &mut App, parent: Entity, w: f32, h: f32, color: Color) -> Entity {
    let entity = block(app, parent, w, h, Axis::Row);
    app.world_mut().insert(entity, color);
    entity
}

fn icon(
    app: &mut App,
    parent: Entity,
    id: u64,
    size: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let raster = decode_svg(STAR.as_bytes(), size * 2, size * 2).map_err(|e| format!("{e:?}"))?;
    load_rgba(
        app.world_mut(),
        id,
        raster.width,
        raster.height,
        raster.rgba,
    )
    .map_err(|e| format!("{e:?}"))?;
    let view = block(app, parent, size as f32, size as f32, Axis::Row);
    app.world_mut().insert(view, ImageView::new(id));
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args_os()
        .nth(1)
        .ok_or("usage: svg_clip_gallery OUTPUT.png")?;
    let mut app = App::new();
    app.world_mut()
        .insert_resource(Viewport::new(W as f32, H as f32));
    let row = app.world_mut().spawn((
        Layout {
            axis: Axis::Row,
            gap: 40.0,
            padding: 30.0,
            width: Size::Px(W as f32),
            height: Size::Px(H as f32),
            ..Layout::row()
        },
        Color::rgb(0.93, 0.94, 0.96),
    ));

    // One SVG rasterized for each displayed size, so every icon is sharp.
    let icons = block(&mut app, row, 200.0, 160.0, Axis::Row);
    app.world_mut().get_mut::<Layout>(icons).unwrap().gap = 12.0;
    for (id, size) in [(1, 24), (2, 48), (3, 96)] {
        icon(&mut app, icons, id, size)?;
    }

    // Children cut by the parent's rounded corners.
    let card = block(&mut app, row, 160.0, 160.0, Axis::Column);
    app.world_mut().insert(card, ClipRounded::radius(40.0));
    for color in [
        Color::rgb(0.95, 0.3, 0.35),
        Color::rgb(0.98, 0.75, 0.2),
        Color::rgb(0.25, 0.7, 0.45),
        Color::rgb(0.25, 0.45, 0.9),
    ] {
        fill(&mut app, card, 160.0, 40.0, color);
    }

    // A list whose rows fade out at the top and bottom edge.
    let list = block(&mut app, row, 240.0, 160.0, Axis::Column);
    app.world_mut().get_mut::<Layout>(list).unwrap().gap = 6.0;
    app.world_mut()
        .insert(list, ClipRounded::radius(16.0).feathered(36.0));
    for i in 0..6 {
        let t = i as f32 / 5.0;
        fill(
            &mut app,
            list,
            240.0,
            22.0,
            Color::rgb(0.3 + 0.5 * t, 0.35, 0.9 - 0.5 * t),
        );
    }

    let frame = rndr_paint::frame(&mut app);
    let mut exporter = Exporter::new()?;
    let image = exporter.render_frame(Some(app.world()), &frame, W, H, 1.0)?;
    std::fs::write(out, encode_png(&image)?)?;
    Ok(())
}
