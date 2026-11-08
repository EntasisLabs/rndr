//! `EffectSlot` entities become `Op::EffectSlot` in paint order.

use rndr_core::{set_parent, App, Color, EffectSlot, Layout, Opacity, Size, Viewport};
use rndr_paint::{frame, Op};

fn sized(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::row()
    }
}

#[test]
fn effect_slot_records_its_rect_before_its_children() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(100.0, 50.0));
    let root = app
        .world_mut()
        .spawn((sized(100.0, 50.0), Color::rgb(1.0, 1.0, 1.0)));
    let slot = app.world_mut().spawn((
        sized(40.0, 30.0),
        Color::rgb(1.0, 0.0, 0.0),
        EffectSlot::new(3)
            .with_data([1.0, 2.0, 3.0, 4.0])
            .with_backdrop(),
    ));
    let child = app
        .world_mut()
        .spawn((sized(10.0, 10.0), Color::rgb(0.0, 0.0, 1.0)));
    set_parent(app.world_mut(), slot, root);
    set_parent(app.world_mut(), child, slot);
    let painted = frame(&mut app);
    // The root and the child paint; the slot's own colour does not.
    assert_eq!(painted.quads.len(), 2);
    assert_eq!(painted.draws.len(), 1);
    assert_eq!(painted.draws[0].z, 1, "effect sits between root and child");
    let Op::EffectSlot(op) = &painted.draws[0].op else {
        panic!("expected an effect slot");
    };
    assert_eq!(op.id, 3);
    assert_eq!(op.data, [1.0, 2.0, 3.0, 4.0]);
    assert!(op.backdrop);
    assert_eq!((op.rect.w, op.rect.h), (40.0, 30.0));
    assert_eq!(op.clip.map(|clip| clip.w), Some(100.0));
}

#[test]
fn effect_slot_inside_a_faded_group_lands_in_the_layer() {
    let mut app = App::new();
    app.world_mut().insert_resource(Viewport::new(100.0, 50.0));
    let root = app.world_mut().spawn((sized(100.0, 50.0), Opacity(0.5)));
    let slot = app
        .world_mut()
        .spawn((sized(40.0, 30.0), EffectSlot::new(3)));
    set_parent(app.world_mut(), slot, root);
    let painted = frame(&mut app);
    let kinds: Vec<&str> = painted
        .draws
        .iter()
        .map(|placed| match placed.op {
            Op::PushLayer(_) => "push",
            Op::PopLayer => "pop",
            Op::EffectSlot(_) => "effect",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["push", "effect", "pop"]);
}
