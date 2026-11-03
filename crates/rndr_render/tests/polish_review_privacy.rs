#![cfg(feature = "observe")]
use rndr_core::*;
use rndr_render::{
    capture::{capture, Error, Target},
    observe::{ObservationContent, ObservationSensitive},
    Exporter,
};

#[test]
fn border_cross_and_scroll_decoration_cannot_bypass_whole_frame_privacy() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(80., 80.));
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(80.),
            height: Size::Px(80.),
            ..Layout::column()
        },
        Color::rgb(0., 0., 0.),
        Border {
            width: 2.,
            color: Color::rgb(1., 1., 1.),
        },
        Radius(10.),
        CrossMark {
            color: Color::rgb(1., 1., 1.),
        },
        Scroll::default(),
    ));
    app.world_mut().insert(
        root,
        ScrollIndicator {
            color: Color::rgb(1., 1., 1.),
        },
    );
    let child = app.world_mut().spawn((Layout {
        width: Size::Px(120.),
        height: Size::Px(240.),
        ..Layout::column()
    },));
    set_parent(app.world_mut(), child, root);
    let mut gpu = Exporter::new().expect("real headless adapter required");
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None),
        Err(Error::Sensitive)
    ));
    app.world_mut().insert_resource(ObservationContent::Public);
    assert!(capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None).is_ok());
    // Sensitivity outside the captured target still denies the composite.
    let secret = app.world_mut().spawn((ObservationSensitive,));
    for target in [root, child] {
        assert!(matches!(
            capture(&mut app, &mut gpu, Target::Entity(target), |_, _| None),
            Err(Error::Sensitive)
        ));
    }
    app.world_mut().remove::<ObservationSensitive>(secret);
    app.world_mut().insert(child, ObservationSensitive);
    assert!(matches!(
        capture(&mut app, &mut gpu, Target::Entity(root), |_, _| None),
        Err(Error::Sensitive)
    ));
}
