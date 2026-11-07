//! Pixels for paint decorations: gradients, shadows, rings, and corners.

use rndr_core::{
    set_parent, App, BorderSides, Color, Corners, Entity, Gradient, Layout, Shadow, Shadows, Size,
    Viewport,
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
