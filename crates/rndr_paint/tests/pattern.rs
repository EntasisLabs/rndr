//! Headless interaction pattern for later PRs.
//!
//! 1. Build an [`rndr_core::App`], insert a [`rndr_core::Viewport`], spawn the scene.
//! 2. Write one frame of [`rndr_core::Input`] (pointer edge, key, or chars).
//! 3. Call [`rndr_paint::frame`].
//! 4. Assert on the world, or snapshot [`rndr_paint::quads_snapshot`].
//!
//! [`press_at`] clears one-frame edges after the frame. The winit runner does
//! the same after present, so a following frame does not repeat the click.

use rndr_core::{
    App, Axis, Button, Chrome, Color, Entity, Fired, Focus, Focusable, Input, Label, Layout, Rect,
    Size, Viewport,
};
use rndr_paint::{frame, quads_snapshot, Frame};

fn button_layout() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(80.0),
        height: Size::Px(32.0),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn boot() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    app.world_mut().insert_resource(Focus::default());
    let button = app.world_mut().spawn((
        button_layout(),
        Color::rgb(0.2, 0.3, 0.4),
        Label::new("Go"),
        Focusable,
        Button::new("go"),
    ));
    (app, button)
}

/// Write a press, paint one frame, then drop the one-frame edges.
fn press_at(app: &mut App, x: f32, y: f32) -> Frame {
    {
        let input = app.world_mut().resource_mut::<Input>().expect("input");
        input.pointer.position = [x, y];
        input.pointer.pressed = true;
        input.pointer.down = true;
    }
    let painted = frame(app);
    let input = app.world_mut().resource_mut::<Input>().expect("input");
    input.pointer.pressed = false;
    input.scroll = [0.0, 0.0];
    input.key = None;
    input.chars.clear();
    input.ime_commit.clear();
    painted
}

#[test]
fn click_fires_and_paints_the_button() {
    let (mut app, button) = boot();
    let placed = frame(&mut app);
    assert!(!placed.quads.is_empty(), "layout should paint the button");
    let rect = *app.world().get::<Rect>(button).expect("placed button");

    let painted = press_at(&mut app, rect.x + 4.0, rect.y + 4.0);
    assert_eq!(
        app.world()
            .resource::<Fired>()
            .expect("fired")
            .name
            .as_deref(),
        Some("go")
    );
    insta::assert_snapshot!(quads_snapshot(&painted));

    let _again = frame(&mut app);
    assert!(
        app.world()
            .resource::<Fired>()
            .expect("fired")
            .name
            .is_none(),
        "a cleared press edge must not fire again"
    );
}
