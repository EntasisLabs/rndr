//! `frame_live` paints bound `Live` values without a full update.

use rndr_core::{set_parent, App, Color, Layout, Live, LiveFill, LiveOpacity, Size, Viewport};
use rndr_paint::{frame, frame_live, Op};

fn sized(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::row()
    }
}

#[test]
fn a_live_frame_paints_the_latest_fill_and_opacity() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(100.0, 50.0));
    let root = app.world_mut().spawn((sized(100.0, 50.0),));
    let tile = app
        .world_mut()
        .spawn((sized(40.0, 30.0), Color::rgb(1.0, 1.0, 1.0)));
    set_parent(app.world_mut(), tile, root);
    let tint = Live::new(Color::rgb(1.0, 1.0, 1.0));
    let fade = Live::new(1.0_f32);
    app.world_mut().insert(tile, LiveFill::new(&tint));
    let faded = app
        .world_mut()
        .spawn((sized(40.0, 30.0), Color::rgb(0.0, 0.0, 1.0)));
    set_parent(app.world_mut(), faded, root);
    app.world_mut().insert(faded, LiveOpacity::new(&fade));
    let painted = frame(&mut app);
    assert_eq!(painted.quads[0].color, Color::rgb(1.0, 1.0, 1.0));

    tint.set(Color::rgb(1.0, 0.0, 0.0));
    let painted = frame_live(&mut app);
    assert_eq!(painted.quads[0].color, Color::rgb(1.0, 0.0, 0.0));

    fade.set(0.5);
    let painted = frame_live(&mut app);
    let opacity = painted.draws.iter().find_map(|draw| match &draw.op {
        Op::PushLayer(layer) => Some(layer.opacity),
        _ => None,
    });
    assert_eq!(opacity, Some(0.5));
}
