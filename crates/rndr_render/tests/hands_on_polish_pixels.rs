#![cfg(feature = "observe")]
use rndr_core::*;
use rndr_render::{
    capture::{capture, Error, Target},
    observe::{ObservationContent, ObservationSensitive},
    Exporter,
};
fn scene(width: f32, height: f32, scale: f32) -> (App, Entity) {
    let mut app = App::new();
    let mut viewport = Viewport::new(width, height);
    viewport.scale = scale;
    app.world_mut().insert_resource(viewport);
    app.world_mut().insert_resource(ObservationContent::Public);
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(width),
            height: Size::Px(height),
            ..Layout::column()
        },
        Color::rgb(0., 0., 0.),
    ));
    (app, root)
}
#[test]
fn centered_cross_ink_is_symmetric_at_1x_2x_and_text_scaled_sizes() {
    let mut gpu = Exporter::new().expect("real headless adapter required");
    for size in [18., 27., 36.] {
        for scale in [1., 2.] {
            let (mut app, root) = scene(size, size, scale);
            app.world_mut().insert(
                root,
                CrossMark {
                    color: Color::rgb(1., 1., 1.),
                },
            );
            let image = capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None)
                .unwrap()
                .image;
            let side = image.width as usize;
            let mut mass = 0.;
            let mut xmass = 0.;
            let mut ymass = 0.;
            let mut reflected_error = 0.;
            let mut max_error = 0;
            for y in 0..side {
                for x in 0..side {
                    let ink = image.rgba[(y * side + x) * 4] as f64;
                    mass += ink;
                    xmass += ink * x as f64;
                    ymass += ink * y as f64;
                    let reflect_x = image.rgba[(y * side + side - 1 - x) * 4] as i16;
                    let reflect_y = image.rgba[((side - 1 - y) * side + x) * 4] as i16;
                    reflected_error +=
                        (ink - reflect_x as f64).abs() + (ink - reflect_y as f64).abs();
                    max_error = max_error
                        .max((ink as i16 - reflect_x).abs())
                        .max((ink as i16 - reflect_y).abs());
                }
            }
            assert!(mass > 255.);
            let center = (side as f64 - 1.) / 2.;
            println!("size={size} scale={scale}: centroid=({:.5},{:.5}) expected={center:.5}, mirror_error={:.5}, max_sample_difference={max_error}",xmass/mass,ymass/mass,reflected_error/mass);
            assert!((xmass / mass - center).abs() < 0.1 && (ymass / mass - center).abs() < 0.1);
            // Path tessellation/MSAA can distribute one edge sample differently
            // across reflected caps. Compare integrated ink, not exact edge bytes.
            assert!(reflected_error / mass < 0.125);
            app.world_mut().insert(root, ObservationSensitive);
            assert!(matches!(
                capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None),
                Err(Error::Sensitive)
            ));
        }
    }
}
#[test]
fn rounded_outline_is_uniform_inset_clipped_and_above_opaque_descendants() {
    let mut gpu = Exporter::new().expect("real headless adapter required");
    for scale in [1., 2.] {
        let (mut app, root) = scene(60., 40., scale);
        app.world_mut().insert(
            root,
            Border {
                color: Color::rgb(1., 1., 1.),
                width: 2.,
            },
        );
        app.world_mut().insert(root, Radius(10.));
        let child = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            Color::rgb(0., 0., 0.),
        ));
        set_parent(app.world_mut(), child, root);
        let image = capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None)
            .unwrap()
            .image;
        let pixel =
            |x: usize, y: usize| image.rgba[(y * image.width as usize + x) * 4] as f64 / 255.;
        let top: f64 = (0..(6. * scale) as usize)
            .map(|y| pixel(image.width as usize / 2, y))
            .sum();
        let left: f64 = (0..(6. * scale) as usize)
            .map(|x| pixel(x, image.height as usize / 2))
            .sum();
        assert!((top - 2. * scale as f64).abs() < 0.3, "top coverage {top}");
        assert!((left - top).abs() < 0.15, "left/top width {left} {top}");
        assert_eq!(
            pixel(image.width as usize / 2, image.height as usize / 2),
            0.
        );
        assert_eq!(pixel(0, 0), 0.);
        // Corners retain antialiasing rather than hard binary stair steps.
        assert!(
            (0..(10. * scale) as usize).any(|y| (0..(10. * scale) as usize).any(|x| {
                let v = pixel(x, y);
                v > 0.05 && v < 0.95
            }))
        );
    }
}
