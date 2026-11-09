//! `Prop::Blur`, `Prop::BackdropBlur` and `LiveBlur` at fixed clock times.

use std::time::Duration;

use rndr_core::motion::{self, Curve, Easing, MotionSettings, Prop};
use rndr_core::{BackdropBlur, Blur, Entity, Layout, Live, LiveBlur, Sim, Size, Viewport};

fn boxed(sim: &mut Sim) -> Entity {
    sim.world().spawn((Layout {
        width: Size::Px(40.0),
        height: Size::Px(20.0),
        ..Layout::column()
    },))
}

fn step(sim: &mut Sim, ms: u64) {
    sim.advance(Duration::from_millis(ms));
    sim.update();
}

fn sim() -> Sim {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    sim
}

#[test]
fn blur_tweens_from_the_component_value_and_settles() {
    let mut sim = sim();
    let card = boxed(&mut sim);
    sim.world().insert(card, Blur(2.0));
    sim.update();
    let linear = Curve::tween(1.0, Easing::Linear);
    motion::animate(sim.world(), card, Prop::Blur, 10.0, linear);
    sim.update();
    step(&mut sim, 500);
    let mid = sim.world().get::<Blur>(card).unwrap().0;
    assert!((mid - 6.0).abs() < 0.05, "{mid}");
    step(&mut sim, 600);
    assert_eq!(sim.world().get::<Blur>(card).unwrap().0, 10.0);
    assert!(!motion::animating(sim.world()));
}

#[test]
fn backdrop_blur_animates_and_never_goes_negative() {
    let mut sim = sim();
    let card = boxed(&mut sim);
    sim.update();
    assert!(sim.world().get::<BackdropBlur>(card).is_none());
    motion::animate(
        sim.world(),
        card,
        Prop::BackdropBlur,
        8.0,
        Curve::tween(0.4, Easing::Linear),
    );
    sim.update();
    step(&mut sim, 200);
    let mid = sim.world().get::<BackdropBlur>(card).unwrap().0;
    assert!((mid - 4.0).abs() < 0.05, "{mid}");
    motion::animate(
        sim.world(),
        card,
        Prop::BackdropBlur,
        0.0,
        motion::Spring::new(0.2, 0.6),
    );
    for _ in 0..60 {
        step(&mut sim, 16);
        assert!(sim.world().get::<BackdropBlur>(card).unwrap().0 >= 0.0);
    }
    assert_eq!(sim.world().get::<BackdropBlur>(card).unwrap().0, 0.0);
    // Opacity and transform animations don't add blur components.
    let other = boxed(&mut sim);
    motion::animate(sim.world(), other, Prop::Opacity, 0.5, Curve::Instant);
    step(&mut sim, 16);
    assert!(sim.world().get::<Blur>(other).is_none());
    assert!(sim.world().get::<BackdropBlur>(other).is_none());
}

#[test]
fn reduced_motion_jumps_blur_to_its_target() {
    let mut sim = sim();
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    let card = boxed(&mut sim);
    sim.update();
    motion::animate(
        sim.world(),
        card,
        Prop::Blur,
        6.0,
        Curve::tween(1.0, Easing::Linear),
    );
    step(&mut sim, 16);
    assert_eq!(sim.world().get::<Blur>(card).unwrap().0, 6.0);
}

#[test]
fn live_blur_writes_the_latest_value() {
    let mut sim = sim();
    let card = boxed(&mut sim);
    let sigma = Live::new(3.0_f32);
    sim.world().insert(card, LiveBlur::new(&sigma));
    sim.update();
    assert_eq!(sim.world().get::<Blur>(card).unwrap().0, 3.0);
    sigma.set(-1.0);
    sim.update();
    assert_eq!(sim.world().get::<Blur>(card).unwrap().0, 0.0);
}

#[test]
fn an_app_set_blur_survives_other_motion_on_the_entity() {
    let mut sim = sim();
    let card = boxed(&mut sim);
    motion::animate(sim.world(), card, Prop::Opacity, 0.5, Curve::Instant);
    step(&mut sim, 16);
    sim.world().insert(card, BackdropBlur(10.0));
    sim.world().insert(card, Blur(3.0));
    motion::animate(sim.world(), card, Prop::Opacity, 1.0, Curve::Instant);
    step(&mut sim, 16);
    assert_eq!(sim.world().get::<BackdropBlur>(card).unwrap().0, 10.0);
    assert_eq!(sim.world().get::<Blur>(card).unwrap().0, 3.0);
}
