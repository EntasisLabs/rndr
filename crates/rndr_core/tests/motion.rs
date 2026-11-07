//! Motion runtime driven by a manual clock.

use std::time::Duration;

use rndr_core::motion::{
    self, Anim, Curve, Easing, LayoutMotion, MotionSettings, MotionSettled, Prop, Spring,
    Transition,
};
use rndr_core::{
    despawn, events, set_parent, Entity, Layout, Opacity, Rect, Sim, Size, Transform, Viewport,
};

fn boxed(sim: &mut Sim, w: f32, h: f32) -> Entity {
    sim.world().spawn((Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::column()
    },))
}

fn step(sim: &mut Sim, ms: u64) {
    sim.advance(Duration::from_millis(ms));
    sim.update();
}

fn opacity(sim: &mut Sim, e: Entity) -> f32 {
    sim.world().get::<Opacity>(e).unwrap().0
}

fn transform(sim: &mut Sim, e: Entity) -> Transform {
    *sim.world().get::<Transform>(e).unwrap()
}

#[test]
fn a_spring_moves_writes_components_and_settles_once() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    motion::animate(sim.world(), card, Prop::Opacity, 0.0, Spring::SMOOTH);
    motion::animate(sim.world(), card, Prop::Scale, 1.2, Spring::SMOOTH);
    sim.update();
    assert!(sim.app.wants_frame());
    step(&mut sim, 120);
    let mid = opacity(&mut sim, card);
    assert!(mid > 0.0 && mid < 1.0, "{mid}");
    let scale = transform(&mut sim, card).scale;
    assert!(scale[0] > 1.0 && scale[0] < 1.2 && scale[0] == scale[1]);
    assert!(motion::is_animating(sim.world(), card));
    step(&mut sim, 3000);
    assert_eq!(opacity(&mut sim, card), 0.0);
    assert_eq!(transform(&mut sim, card).scale, [1.2, 1.2]);
    assert!(!motion::animating(sim.world()));
    assert!(!sim.app.wants_frame());
    step(&mut sim, 16);
    assert_eq!(
        events::<MotionSettled>(sim.world()),
        &[MotionSettled {
            entity: card,
            removed: false
        }]
    );
    step(&mut sim, 16);
    assert!(events::<MotionSettled>(sim.world()).is_empty());
}

#[test]
fn retargeting_mid_flight_does_not_jump() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    motion::animate(sim.world(), card, Prop::X, 100.0, Spring::BOUNCY);
    sim.update();
    step(&mut sim, 90);
    let before = motion::value(sim.world(), card, Prop::X).unwrap();
    motion::animate(
        sim.world(),
        card,
        Prop::X,
        -50.0,
        Curve::tween(0.3, Easing::EaseOut),
    );
    assert_eq!(motion::value(sim.world(), card, Prop::X), Some(before));
    assert_eq!(motion::target(sim.world(), card, Prop::X), Some(-50.0));
    step(&mut sim, 16);
    let after = transform(&mut sim, card).translate[0];
    assert!((after - before).abs() < 30.0, "{before} -> {after}");
    step(&mut sim, 400);
    assert_eq!(transform(&mut sim, card).translate[0], -50.0);
}

#[test]
fn release_velocity_and_set_for_gestures() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    motion::set(sim.world(), card, Prop::Rotate, 0.5);
    sim.update();
    assert_eq!(transform(&mut sim, card).rotate, 0.5);
    motion::animate(
        sim.world(),
        card,
        Prop::Rotate,
        0.0,
        Anim::new(Spring::SMOOTH.into()).velocity(4.0),
    );
    step(&mut sim, 30);
    assert!(transform(&mut sim, card).rotate > 0.5);
}

#[test]
fn enter_exit_and_revive() {
    let mut sim = Sim::new();
    let root = boxed(&mut sim, 200.0, 100.0);
    let card = boxed(&mut sim, 40.0, 20.0);
    set_parent(sim.world(), card, root);
    motion::enter(sim.world(), card, Transition::scale(0.9));
    sim.update();
    assert_eq!(opacity(&mut sim, card), 0.0);
    assert_eq!(transform(&mut sim, card).scale, [0.9, 0.9]);
    step(&mut sim, 2000);
    assert_eq!(opacity(&mut sim, card), 1.0);
    assert_eq!(transform(&mut sim, card).scale, [1.0, 1.0]);

    motion::exit(sim.world(), card, Transition::slide(0.0, 12.0));
    step(&mut sim, 60);
    assert!(sim.world().is_alive(card));
    let leaving = opacity(&mut sim, card);
    assert!(leaving < 1.0 && leaving > 0.0);
    motion::enter(sim.world(), card, Transition::slide(0.0, 12.0));
    assert_eq!(
        motion::value(sim.world(), card, Prop::Opacity),
        Some(leaving)
    );
    step(&mut sim, 2000);
    assert!(sim.world().is_alive(card));
    assert_eq!(opacity(&mut sim, card), 1.0);

    motion::exit(sim.world(), card, Transition::fade());
    step(&mut sim, 2000);
    assert!(!sim.world().is_alive(card));
    step(&mut sim, 16);
    assert!(
        events::<MotionSettled>(sim.world()).contains(&MotionSettled {
            entity: card,
            removed: true
        })
    );
}

#[test]
fn layout_motion_glides_siblings_into_place() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 200.0));
    let root = boxed(&mut sim, 200.0, 200.0);
    let first = boxed(&mut sim, 40.0, 30.0);
    let second = boxed(&mut sim, 40.0, 30.0);
    set_parent(sim.world(), first, root);
    set_parent(sim.world(), second, root);
    sim.world().insert(second, LayoutMotion::default());
    sim.update();
    step(&mut sim, 16);
    assert_eq!(transform(&mut sim, second).translate, [0.0, 0.0]);
    let start = sim.world().get::<Rect>(second).unwrap().y;
    despawn(sim.world(), first);
    sim.update();
    let moved = sim.world().get::<Rect>(second).unwrap().y;
    assert_eq!(start - moved, 30.0);
    // Drawn where it was, then glides to the new slot.
    assert_eq!(transform(&mut sim, second).translate[1], 30.0);
    step(&mut sim, 100);
    let mid = transform(&mut sim, second).translate[1];
    assert!(mid > 0.0 && mid < 30.0, "{mid}");
    step(&mut sim, 2000);
    assert_eq!(transform(&mut sim, second).translate[1], 0.0);
    assert!(!motion::animating(sim.world()));
}

#[test]
fn reduced_motion_and_time_scale() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    sim.update();
    motion::animate(sim.world(), card, Prop::X, 80.0, Spring::SMOOTH);
    motion::animate(sim.world(), card, Prop::Opacity, 0.0, Spring::SMOOTH);
    sim.update();
    assert_eq!(transform(&mut sim, card).translate[0], 80.0);
    step(&mut sim, 90);
    let fading = opacity(&mut sim, card);
    assert!(fading > 0.0 && fading < 1.0);
    step(&mut sim, 100);
    assert_eq!(opacity(&mut sim, card), 0.0);

    sim.world().insert_resource(MotionSettings {
        reduced_motion: false,
        time_scale: 0.0,
    });
    motion::animate(sim.world(), card, Prop::Opacity, 1.0, Spring::SMOOTH);
    step(&mut sim, 500);
    assert_eq!(opacity(&mut sim, card), 0.0);
    sim.world().insert_resource(MotionSettings::default());
    step(&mut sim, 3000);
    assert_eq!(opacity(&mut sim, card), 1.0);
}

#[test]
fn hit_testing_follows_the_animated_transform() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let root = boxed(&mut sim, 200.0, 100.0);
    let card = boxed(&mut sim, 40.0, 20.0);
    set_parent(sim.world(), card, root);
    sim.update();
    motion::animate(sim.world(), card, Prop::X, 100.0, Spring::SNAPPY);
    step(&mut sim, 2000);
    assert_eq!(rndr_core::hit_test(sim.world(), 110.0, 10.0), Some(card));
    assert_eq!(rndr_core::hit_test(sim.world(), 10.0, 10.0), Some(root));
}
