//! Pixels for UI groups: isolated opacity, translation, and rotation.

use rndr_core::{set_parent, App, Color, Entity, Layout, Opacity, Size, Transform, Viewport};
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
fn ui_groups_composite_opacity_and_transforms() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(160.0, 80.0));
    let root = boxed(&mut app, 160.0, 80.0, Color::rgb(1.0, 1.0, 1.0), 20.0);
    // x 0..40: red at half opacity, with an opaque blue child over its corner.
    let faded = boxed(&mut app, 40.0, 40.0, Color::rgb(1.0, 0.0, 0.0), 0.0);
    let inner = boxed(&mut app, 20.0, 20.0, Color::rgb(0.0, 0.0, 1.0), 0.0);
    // x 60..80, moved down to y 40..60.
    let moved = boxed(&mut app, 20.0, 20.0, Color::rgb(0.0, 0.6, 0.0), 0.0);
    // x 100..120, turned 45 degrees about (110, 10).
    let turned = boxed(&mut app, 20.0, 20.0, Color::rgb(0.0, 0.0, 0.0), 0.0);
    for child in [faded, moved, turned] {
        set_parent(app.world_mut(), child, root);
    }
    set_parent(app.world_mut(), inner, faded);
    app.world_mut().insert(faded, Opacity(0.5));
    app.world_mut()
        .insert(moved, Transform::translate(0.0, 40.0));
    app.world_mut()
        .insert(turned, Transform::rotate(std::f32::consts::FRAC_PI_4));
    let frame = rndr_paint::frame(&mut app);
    let mut exporter = match Exporter::new() {
        Ok(exporter) => exporter,
        Err(ExportError::NoAdapter) => panic!("no graphics adapter"),
        Err(err) => panic!("{err}"),
    };
    let image = exporter
        .render_frame(Some(app.world()), &frame, 160, 80, 1.0)
        .expect("render");

    // Half red over white, and the blue child replaces red inside the group
    // before the group fades: no red shows through the blue.
    let red = pixel(&image, 30, 30);
    assert!(red[0] > 240 && red[1] > 100 && red[1] < 230, "{red:?}");
    assert_eq!(red[1], red[2]);
    let blue = pixel(&image, 10, 10);
    assert!(blue[2] > 240 && blue[0] > 100 && blue[0] < 230, "{blue:?}");
    assert_eq!(blue[0], blue[1], "red leaked into the blue: {blue:?}");

    // Translation: the layout spot is background, the moved spot is green.
    assert_eq!(pixel(&image, 70, 10), [255, 255, 255, 255]);
    let green = pixel(&image, 70, 50);
    assert!(
        green[1] > 100 && green[0] < 40 && green[2] < 40,
        "{green:?}"
    );

    // Rotation: an unrotated corner is background; a point past the
    // unrotated right edge is inside the diamond.
    assert_eq!(pixel(&image, 101, 1), [255, 255, 255, 255]);
    let tip = pixel(&image, 122, 10);
    assert!(tip[0] < 40 && tip[1] < 40 && tip[2] < 40, "{tip:?}");
}
