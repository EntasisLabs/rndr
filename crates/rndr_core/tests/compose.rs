//! Hit testing follows `Transform`; `Opacity` leaves it alone.

use rndr_core::{hit_test, set_parent, App, Entity, Layout, Opacity, Size, Transform, Viewport};

fn scene(transform: Option<Transform>) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Layout::column()
    },));
    let child = app.world_mut().spawn((Layout {
        width: Size::Px(40.0),
        height: Size::Px(20.0),
        ..Layout::column()
    },));
    if let Some(transform) = transform {
        app.world_mut().insert(child, transform);
    }
    set_parent(app.world_mut(), child, root);
    app.update();
    (app, root, child)
}

#[test]
fn translate_moves_the_hit_area() {
    let (app, root, child) = scene(Some(Transform::translate(100.0, 0.0)));
    assert_eq!(hit_test(app.world(), 110.0, 10.0), Some(child));
    assert_eq!(hit_test(app.world(), 10.0, 10.0), Some(root));
}

#[test]
fn scale_grows_the_hit_area_about_the_center() {
    let (app, root, child) = scene(Some(Transform::scale(2.0)));
    // Layout box is 0..40 x 0..20; doubled about (20, 10) it reaches x 60.
    assert_eq!(hit_test(app.world(), 50.0, 15.0), Some(child));
    assert_eq!(hit_test(app.world(), 70.0, 15.0), Some(root));
}

#[test]
fn rotation_turns_the_hit_area() {
    let (app, root, child) = scene(Some(Transform::rotate(std::f32::consts::FRAC_PI_2)));
    // A quarter turn about (20, 10) makes the box 20 wide and 40 tall.
    assert_eq!(hit_test(app.world(), 20.0, 25.0), Some(child));
    assert_eq!(hit_test(app.world(), 35.0, 10.0), Some(root));
}

#[test]
fn a_group_outside_its_parent_clip_is_not_hit() {
    let (app, _, _) = scene(Some(Transform::translate(300.0, 0.0)));
    assert_eq!(hit_test(app.world(), 310.0, 10.0), None);
}

#[test]
fn a_collapsed_transform_is_not_hit_and_opacity_is_ignored() {
    let (app, root, _) = scene(Some(Transform::scale(0.0)));
    assert_eq!(hit_test(app.world(), 20.0, 10.0), Some(root));
    let (mut app, _, child) = scene(None);
    app.world_mut().insert(child, Opacity(0.0));
    assert_eq!(hit_test(app.world(), 20.0, 10.0), Some(child));
}
