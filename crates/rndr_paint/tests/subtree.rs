use rndr_core::{App, Color, Layout, Rect, Size, Viewport};
use rndr_paint::paint_subtree;

#[test]
fn a_sibling_is_not_painted_with_the_subtree() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(200.0, 80.0));
    let parent = app.world_mut().spawn((Layout {
        width: Size::Px(120.0),
        height: Size::Px(40.0),
        ..Layout::column()
    },));
    let left = app.world_mut().spawn((
        Layout {
            width: Size::Px(40.0),
            height: Size::Px(20.0),
            ..Layout::column()
        },
        Color::rgb(0.9, 0.1, 0.1),
    ));
    let right = app.world_mut().spawn((
        Layout {
            width: Size::Px(40.0),
            height: Size::Px(20.0),
            ..Layout::column()
        },
        Color::rgb(0.1, 0.1, 0.9),
    ));
    rndr_core::set_parent(app.world_mut(), left, parent);
    rndr_core::set_parent(app.world_mut(), right, parent);
    app.update();
    let (quads, _) = paint_subtree(app.world(), left);
    assert!(!quads.is_empty());
    assert!(quads.iter().all(|quad| quad.color.b < 0.5));
    let _ = app.world().get::<Rect>(right);
}
