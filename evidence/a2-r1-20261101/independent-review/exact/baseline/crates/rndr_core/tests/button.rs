use rndr_core::{
    App, Axis, Button, Chrome, Color, Fired, Focus, Focusable, Input, Key, Label, Layout, Rect,
    Size, Viewport,
};

fn box_layout(w: f32, h: f32) -> Layout {
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

fn boot(enabled: bool) -> (App, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    app.world_mut().insert_resource(Focus::default());
    let button = app.world_mut().spawn((
        box_layout(80.0, 32.0),
        Color::rgb(0.2, 0.3, 0.4),
        Label::new("Go"),
        Focusable,
        Button {
            action: "go".into(),
            enabled,
        },
    ));
    app.update();
    (app, button)
}

fn click(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [x, y];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
}

fn fired(app: &App) -> Option<String> {
    app.world().resource::<Fired>().unwrap().name.clone()
}

#[test]
fn a_press_and_release_in_one_frame_fires() {
    let (mut app, button) = boot(true);
    let rect = *app.world().get::<Rect>(button).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [rect.x + 4.0, rect.y + 4.0];
    input.pointer.press_at = Some(input.pointer.position);
    input.pointer.pressed = true;
    input.pointer.released = true;
    input.pointer.down = false;
    app.update();
    assert_eq!(fired(&app).as_deref(), Some("go"));
}

#[test]
fn click_fires_once() {
    let (mut app, button) = boot(true);
    let rect = *app.world().get::<Rect>(button).unwrap();
    click(&mut app, rect.x + 4.0, rect.y + 4.0);
    assert_eq!(fired(&app).as_deref(), Some("go"));

    app.update();
    assert!(fired(&app).is_none());
}

#[test]
fn disabled_button_does_not_fire() {
    let (mut app, button) = boot(false);
    let rect = *app.world().get::<Rect>(button).unwrap();
    click(&mut app, rect.x + 4.0, rect.y + 4.0);
    assert!(fired(&app).is_none());
}

#[test]
fn enter_fires_the_focused_button() {
    let (mut app, button) = boot(true);
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(button);
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Enter);
    app.update();
    assert_eq!(fired(&app).as_deref(), Some("go"));
    assert!(app.world().resource::<Input>().unwrap().key.is_none());
}

#[test]
fn click_outside_does_not_fire() {
    let (mut app, _) = boot(true);
    click(&mut app, 180.0, 90.0);
    assert!(fired(&app).is_none());
}
