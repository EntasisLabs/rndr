use rndr_core::{
    caret_visible, publish_system_appearance, App, Buffer, CapturesKeys, Caret, Chrome, Editor,
    Focus, Label, Layout, ManualClock, Size, SystemAppearance, Viewport, CARET_BLINK,
};
use std::sync::Arc;

fn boot() -> (App, rndr_core::Entity, Arc<ManualClock>) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let clock = ManualClock::handle();
    app.set_clock(clock.clone());
    let world = app.world_mut();
    world.insert_resource(Viewport::new(300.0, 200.0));
    let body = world.spawn((
        Layout {
            width: Size::Px(280.0),
            height: Size::Px(120.0),
            ..Layout::column()
        },
        Label::new(""),
        Buffer::from_str("hello world"),
        Editor::default(),
        CapturesKeys,
    ));
    world.insert_resource(Focus { entity: Some(body) });
    app.update();
    app.update();
    (app, body, clock)
}

fn tick(app: &mut App, clock: &ManualClock) {
    clock.advance(CARET_BLINK);
    app.update();
}

#[test]
fn a_focused_insertion_point_blinks_and_any_move_shows_it_again() {
    let (mut app, body, clock) = boot();
    assert!(app.world().get::<Caret>(body).is_some());
    assert!(caret_visible(app.world()));
    // Idle frames without elapsed time never change the phase.
    for _ in 0..5 {
        app.update();
    }
    assert!(caret_visible(app.world()));
    tick(&mut app, &clock);
    assert!(!caret_visible(app.world()));
    tick(&mut app, &clock);
    assert!(caret_visible(app.world()));
    tick(&mut app, &clock);
    assert!(!caret_visible(app.world()));
    // Typing or moving restarts the on phase for a full interval.
    app.world_mut().get_mut::<Buffer>(body).unwrap().caret = 3;
    app.world_mut().get_mut::<Buffer>(body).unwrap().anchor = 3;
    app.update();
    assert!(caret_visible(app.world()));
    clock.advance(CARET_BLINK / 2);
    app.update();
    assert!(caret_visible(app.world()));
}

#[test]
fn selections_unfocused_text_and_reduced_motion_do_not_blink() {
    let (mut app, body, clock) = boot();
    {
        let buffer = app.world_mut().get_mut::<Buffer>(body).unwrap();
        buffer.anchor = 0;
        buffer.caret = 5;
    }
    app.update();
    tick(&mut app, &clock);
    assert!(caret_visible(app.world()), "a selection stays drawn");
    {
        let buffer = app.world_mut().get_mut::<Buffer>(body).unwrap();
        buffer.anchor = 5;
    }
    app.update();
    publish_system_appearance(
        app.world_mut(),
        SystemAppearance {
            reduced_motion: true,
            ..Default::default()
        },
    );
    app.update();
    for _ in 0..3 {
        tick(&mut app, &clock);
        assert!(
            caret_visible(app.world()),
            "reduced motion keeps the caret on"
        );
    }
    assert!(app.next_deadline().is_none(), "no blink timer is armed");
    publish_system_appearance(app.world_mut(), SystemAppearance::default());
    app.world_mut().resource_mut::<Focus>().unwrap().entity = None;
    app.update();
    tick(&mut app, &clock);
    assert!(caret_visible(app.world()));
    assert!(
        app.next_deadline().is_none(),
        "nothing focused arms nothing"
    );
}
