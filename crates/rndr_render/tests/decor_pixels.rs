//! Pixels for paint decorations: gradients, shadows, rings, and corners.

use rndr_core::{
    set_parent, App, BorderSides, ClipRounded, Color, Corners, Entity, Gradient, Layout, Shadow,
    Shadows, Size, Viewport,
};
use rndr_render::{ExportError, Exporter, RgbaImage};

fn boxed(app: &mut App, w: f32, h: f32, color: Color, gap: f32) -> Entity {
    app.world_mut().spawn((
        Layout {
            width: Size::Px(w),
            height: Size::Px(h),
            gap,
            ..Layout::row()
        },
        color,
    ))
}

fn pixel(image: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let index = ((y * image.width + x) * 4) as usize;
    image.rgba[index..index + 4].try_into().unwrap()
}

#[test]
fn decorations_paint_gradients_shadows_rings_and_corners() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(220.0, 60.0));
    let white = Color::rgb(1.0, 1.0, 1.0);
    let root = boxed(&mut app, 220.0, 60.0, white, 20.0);
    let tiles: Vec<_> = [
        white,
        Color::rgb(0.0, 0.6, 0.0),
        Color::rgb(0.0, 0.6, 0.0),
        Color::rgb(0.0, 0.0, 0.0),
    ]
    .into_iter()
    .map(|color| {
        let tile = boxed(&mut app, 40.0, 40.0, color, 0.0);
        set_parent(app.world_mut(), tile, root);
        tile
    })
    .collect();
    let world = app.world_mut();
    // x 0..40: red to blue, top to bottom.
    world.insert(
        tiles[0],
        Gradient::vertical(Color::rgb(1.0, 0.0, 0.0), Color::rgb(0.0, 0.0, 1.0)),
    );
    // x 60..100: black shadow 8 down.
    world.insert(
        tiles[1],
        Shadows(vec![Shadow::drop(8.0, 4.0, Color::rgb(0.0, 0.0, 0.0))]),
    );
    // x 120..160: a 6 point blue top edge, hairline elsewhere.
    world.insert(
        tiles[2],
        BorderSides {
            color: Color::rgb(0.0, 0.0, 1.0),
            widths: [6.0, 1.0, 1.0, 1.0],
            dash: None,
        },
    );
    // x 180..220: only the top-left corner is round.
    world.insert(tiles[3], Corners([20.0, 0.0, 0.0, 0.0]));
    let frame = rndr_paint::frame(&mut app);
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let image = exporter
        .render_frame(Some(app.world()), &frame, 220, 60, 1.0)
        .expect("render");

    let top = pixel(&image, 20, 2);
    let bottom = pixel(&image, 20, 37);
    let middle = pixel(&image, 20, 20);
    assert!(top[0] > 220 && top[2] < 100, "{top:?}");
    assert!(bottom[2] > 220 && bottom[0] < 100, "{bottom:?}");
    assert!(middle[0] > 60 && middle[2] > 60, "{middle:?}");

    let inside = pixel(&image, 80, 20);
    assert!(
        inside[1] > 120 && inside[0] < 20,
        "the fill covers its shadow"
    );
    let below = pixel(&image, 80, 44);
    assert!(below[0] < 140, "shadow below the tile: {below:?}");
    let left_of = pixel(&image, 56, 20);
    assert!(
        left_of[0] > 200,
        "no offset shadow to the side: {left_of:?}"
    );
    assert_eq!(pixel(&image, 80, 58)[0], 255, "shadow fades out");

    let edge = pixel(&image, 140, 3);
    assert!(edge[2] > 220 && edge[1] < 40, "{edge:?}");
    let fill = pixel(&image, 140, 20);
    assert!(fill[1] > 120 && fill[2] < 20, "{fill:?}");

    assert_eq!(pixel(&image, 181, 1), [255, 255, 255, 255], "round corner");
    assert_eq!(pixel(&image, 218, 1), [0, 0, 0, 255], "square corner");
    assert_eq!(pixel(&image, 181, 38), [0, 0, 0, 255], "square corner");
}

fn export(app: &mut App, w: u32, h: u32) -> RgbaImage {
    let frame = rndr_paint::frame(app);
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    exporter
        .render_frame(Some(app.world()), &frame, w, h, 1.0)
        .expect("render")
}

#[test]
fn clip_rounded_masks_children_with_hard_and_feathered_edges() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(100.0, 40.0));
    let white = Color::rgb(1.0, 1.0, 1.0);
    let blue = Color::rgb(0.0, 0.0, 1.0);
    let root = boxed(&mut app, 100.0, 40.0, white, 20.0);
    let round = boxed(&mut app, 40.0, 40.0, Color::rgb(1.0, 0.0, 0.0), 0.0);
    let soft = boxed(&mut app, 40.0, 40.0, Color::rgb(1.0, 0.0, 0.0), 0.0);
    for tile in [round, soft] {
        set_parent(app.world_mut(), tile, root);
        let child = boxed(&mut app, 40.0, 40.0, blue, 0.0);
        set_parent(app.world_mut(), child, tile);
    }
    // x 0..40: a circle; x 60..100: square with a 10 point soft edge.
    app.world_mut().insert(round, ClipRounded::radius(20.0));
    app.world_mut()
        .insert(soft, ClipRounded::radius(0.0).feathered(10.0));
    let image = export(&mut app, 100, 40);

    let corner = pixel(&image, 3, 3);
    let centre = pixel(&image, 20, 20);
    assert!(corner.iter().take(3).all(|&c| c > 240), "{corner:?}");
    assert!(centre[2] > 240 && centre[0] < 10, "{centre:?}");
    let edge = pixel(&image, 60, 20);
    let half = pixel(&image, 65, 20);
    let inside = pixel(&image, 80, 20);
    assert!(edge[0] > 200, "the soft edge starts near white: {edge:?}");
    assert!(half[0] > 60 && half[0] < 230, "{half:?}");
    assert!(inside[2] > 240 && inside[0] < 10, "{inside:?}");
}

#[cfg(feature = "svg")]
#[test]
fn svg_icons_display_through_an_image_view() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">
        <circle cx="4" cy="4" r="4" fill="#00ff00"/></svg>"##;
    let raster = rndr_render::decode_svg(svg, 32, 32).unwrap();
    assert_eq!((raster.width, raster.height), (32, 32));
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(32.0, 32.0));
    let root = boxed(&mut app, 32.0, 32.0, Color::rgb(1.0, 1.0, 1.0), 0.0);
    rndr_render::load_rgba(app.world_mut(), 7, raster.width, raster.height, raster.rgba).unwrap();
    let icon = app.world_mut().spawn((
        Layout {
            width: Size::Px(32.0),
            height: Size::Px(32.0),
            ..Layout::row()
        },
        rndr_core::ImageView::new(7),
    ));
    set_parent(app.world_mut(), icon, root);
    let image = export(&mut app, 32, 32);
    let centre = pixel(&image, 16, 16);
    let corner = pixel(&image, 1, 1);
    assert!(centre[1] > 240 && centre[0] < 10, "{centre:?}");
    assert!(corner.iter().take(3).all(|&c| c > 240), "{corner:?}");
}
