use rndr_core::{
    set_parent, App, Axis, Chrome, Color, Fired, Focus, Focusable, Input, Key, Label, Layer,
    Layout, Modal, Shell, Size, Viewport, DISMISS,
};

fn px(w: f32, h: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Px(h),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn click(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [x, y];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
}

#[test]
fn popover_click_does_not_focus_the_row() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 120.0));
    let row = app.world_mut().spawn((
        px(100.0, 24.0),
        Color::rgb(0.2, 0.2, 0.2),
        Label::new("Row"),
        Focusable,
    ));
    app.world_mut().spawn((
        px(100.0, 24.0),
        Color::rgb(0.3, 0.3, 0.4),
        Label::new("Menu"),
        Layer(1),
    ));
    app.update();

    click(&mut app, 8.0, 8.0);

    assert_ne!(app.world().resource::<Focus>().unwrap().entity, Some(row));
}

#[test]
fn modal_swallows_the_click() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 120.0));
    let row = app.world_mut().spawn((
        px(80.0, 24.0),
        Color::rgb(0.2, 0.2, 0.2),
        Label::new("Row"),
        Focusable,
    ));
    app.world_mut()
        .spawn((px(20.0, 20.0), Color::rgb(0.1, 0.1, 0.1), Layer(2), Modal));
    app.update();

    click(&mut app, 40.0, 8.0);

    assert_ne!(app.world().resource::<Focus>().unwrap().entity, Some(row));
}

#[test]
fn escape_removes_only_the_top_layer() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 120.0));
    let lower = app
        .world_mut()
        .spawn((px(40.0, 20.0), Color::rgb(0.2, 0.2, 0.2), Layer(1)));
    let upper = app
        .world_mut()
        .spawn((px(40.0, 20.0), Color::rgb(0.4, 0.2, 0.2), Layer(3)));
    let child = app
        .world_mut()
        .spawn((px(10.0, 10.0), Color::rgb(0.5, 0.5, 0.5)));
    set_parent(app.world_mut(), child, upper);
    app.update();

    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Escape);
    app.update();

    assert!(app.world().is_alive(lower));
    assert!(!app.world().is_alive(upper));
    assert!(!app.world().is_alive(child));
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some(DISMISS)
    );
}
