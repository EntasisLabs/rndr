//! Colour, keyframe, size and press-scale motion on a manual clock.

use std::time::Duration;

use rndr_core::motion::{
    self, ColorProp, Curve, Easing, Keyframes, LayoutMotion, MotionSettings, PressScale, Prop,
    Spring,
};
use rndr_core::{
    set_parent, Border, Clip, Color, Entity, Input, Label, Layout, Opacity, Rect, Sim, Size,
    Transform, Viewport,
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

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);
const YELLOW: Color = Color::rgb(1.0, 1.0, 0.0);

#[test]
fn colours_blend_in_oklab_and_settle_exactly() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.world().insert(card, BLUE);
    sim.update();
    motion::animate_color(
        sim.world(),
        card,
        ColorProp::Fill,
        YELLOW,
        Curve::tween(1.0, Easing::Linear),
    );
    step(&mut sim, 500);
    let mid = *sim.world().get::<Color>(card).unwrap();
    // sRGB lerp would give (0.5, 0.5, 0.5); OKLab keeps the midpoint
    // saturated instead of grey.
    let spread = mid.r.max(mid.g).max(mid.b) - mid.r.min(mid.g).min(mid.b);
    assert!(spread > 0.2, "{mid:?}");
    assert_eq!(
        motion::color_value(sim.world(), card, ColorProp::Fill),
        Some(mid)
    );
    step(&mut sim, 600);
    let end = *sim.world().get::<Color>(card).unwrap();
    for (a, b) in [(end.r, 1.0), (end.g, 1.0), (end.b, 0.0), (end.a, 1.0)] {
        assert!(close(a, b, 2.0e-3), "{end:?}");
    }
    assert!(!motion::animating(sim.world()));
    // Once settled the runtime no longer owns the component.
    sim.world().insert(card, BLUE);
    step(&mut sim, 16);
    assert_eq!(*sim.world().get::<Color>(card).unwrap(), BLUE);
}

#[test]
fn colour_retargets_without_a_jump_on_border_and_text() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.world().insert(
        card,
        Border {
            color: BLUE,
            width: 2.0,
        },
    );
    sim.world().insert(card, Label::new("hi").with_color(BLUE));
    sim.update();
    motion::animate_color(sim.world(), card, ColorProp::Border, YELLOW, Spring::SMOOTH);
    motion::animate_color(sim.world(), card, ColorProp::Text, YELLOW, Spring::SMOOTH);
    step(&mut sim, 120);
    let before = sim.world().get::<Border>(card).unwrap().color;
    assert_ne!(before, BLUE);
    assert_eq!(sim.world().get::<Label>(card).unwrap().color, Some(before));
    motion::animate_color(sim.world(), card, ColorProp::Border, BLUE, Spring::SMOOTH);
    sim.update();
    let after = sim.world().get::<Border>(card).unwrap().color;
    for (a, b) in [
        (before.r, after.r),
        (before.g, after.g),
        (before.b, after.b),
    ] {
        assert!(close(a, b, 1.0e-3), "{before:?} {after:?}");
    }
    step(&mut sim, 3000);
    let border = sim.world().get::<Border>(card).unwrap();
    assert!(close(border.color.b, 1.0, 2.0e-3) && border.width == 2.0);
    let text = sim.world().get::<Label>(card).unwrap().color.unwrap();
    assert!(close(text.r, 1.0, 2.0e-3) && close(text.b, 0.0, 2.0e-3));
}

#[test]
fn missing_fill_fades_in_from_transparent() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    motion::animate_color(
        sim.world(),
        card,
        ColorProp::Fill,
        YELLOW,
        Curve::tween(0.2, Easing::Linear),
    );
    step(&mut sim, 100);
    let mid = *sim.world().get::<Color>(card).unwrap();
    assert!(close(mid.a, 0.5, 1.0e-3), "{mid:?}");
    // Premultiplied blending keeps the hue while it fades in.
    assert!(
        close(mid.r, 1.0, 2.0e-3) && close(mid.b, 0.0, 2.0e-3),
        "{mid:?}"
    );
    // No border or label: nothing to animate.
    motion::animate_color(sim.world(), card, ColorProp::Border, BLUE, Spring::SMOOTH);
    assert!(sim.world().get::<Border>(card).is_none());
}

#[test]
fn keyframes_play_steps_in_order_at_fixed_times() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    let track = Keyframes::new()
        .linear(100.0, 1.0)
        .jump(-50.0)
        .eased(0.0, 0.5, Easing::EaseInOut)
        .delay(0.25);
    motion::play(sim.world(), card, Prop::X, &track);
    let x = |sim: &mut Sim| sim.world().get::<Transform>(card).unwrap().translate[0];
    step(&mut sim, 250);
    assert_eq!(x(&mut sim), 0.0);
    step(&mut sim, 500);
    assert!(close(x(&mut sim), 50.0, 0.01));
    // One large frame crosses the end of the linear step and the jump.
    step(&mut sim, 750);
    assert!(close(x(&mut sim), -25.0, 0.01), "{}", x(&mut sim));
    assert!(motion::is_playing(sim.world(), card, Prop::X) || motion::animating(sim.world()));
    step(&mut sim, 300);
    assert_eq!(x(&mut sim), 0.0);
    assert!(!motion::animating(sim.world()));
    assert!(!motion::is_playing(sim.world(), card, Prop::X));
}

#[test]
fn repeats_alternate_and_endless_loops() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    let pulse = Keyframes::new().linear(0.0, 1.0).repeat(2).alternate();
    assert_eq!(pulse.final_value(1.0), Some(1.0));
    motion::play(sim.world(), card, Prop::Opacity, &pulse);
    let opacity = |sim: &mut Sim| sim.world().get::<Opacity>(card).unwrap().0;
    step(&mut sim, 500);
    assert!(close(opacity(&mut sim), 0.5, 1.0e-3));
    step(&mut sim, 1000);
    assert!(close(opacity(&mut sim), 0.5, 1.0e-3));
    step(&mut sim, 600);
    assert_eq!(opacity(&mut sim), 1.0);
    assert!(!motion::animating(sim.world()));

    let spin = Keyframes::new()
        .jump(0.0)
        .linear(std::f32::consts::TAU, 2.0)
        .forever();
    motion::play(sim.world(), card, Prop::Rotate, &spin);
    step(&mut sim, 9000);
    let r = sim.world().get::<Transform>(card).unwrap().rotate;
    assert!(close(r, std::f32::consts::PI, 1.0e-3), "{r}");
    assert!(motion::animating(sim.world()));
    // Any other animation on the property replaces the loop.
    motion::animate(sim.world(), card, Prop::Rotate, 0.0, Curve::Instant);
    step(&mut sim, 16);
    assert!(!motion::is_playing(sim.world(), card, Prop::Rotate));
    assert!(!motion::animating(sim.world()));
}

#[test]
fn spring_steps_hand_velocity_to_the_next_step() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.update();
    let track = Keyframes::new()
        .spring_for(100.0, 0.1, Spring::SMOOTH)
        .spring(0.0, Spring::SMOOTH);
    motion::play(sim.world(), card, Prop::X, &track);
    step(&mut sim, 100);
    let at_handoff = motion::value(sim.world(), card, Prop::X).unwrap();
    assert!(at_handoff > 0.0 && at_handoff < 100.0);
    // Still carried upward by the first spring's velocity just after it.
    step(&mut sim, 10);
    assert!(motion::value(sim.world(), card, Prop::X).unwrap() > at_handoff);
    step(&mut sim, 3000);
    assert_eq!(motion::value(sim.world(), card, Prop::X), Some(0.0));
}

#[test]
fn reduced_motion_ends_tracks_and_holds_loops() {
    let mut sim = Sim::new();
    let card = boxed(&mut sim, 40.0, 20.0);
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    sim.update();
    motion::play(
        sim.world(),
        card,
        Prop::X,
        &Keyframes::new().linear(40.0, 1.0).linear(80.0, 1.0),
    );
    sim.update();
    assert_eq!(
        sim.world().get::<Transform>(card).unwrap().translate[0],
        80.0
    );
    motion::play(
        sim.world(),
        card,
        Prop::Rotate,
        &Keyframes::new().linear(6.0, 1.0).forever(),
    );
    step(&mut sim, 500);
    assert_eq!(sim.world().get::<Transform>(card).unwrap().rotate, 0.0);
    assert!(!motion::animating(sim.world()));
}

fn accordion(sim: &mut Sim) -> (Entity, Entity, Entity) {
    sim.world().insert_resource(Viewport::new(200.0, 200.0));
    let root = boxed(sim, 200.0, 200.0);
    let panel = boxed(sim, 100.0, 20.0);
    let body = boxed(sim, 100.0, 80.0);
    set_parent(sim.world(), panel, root);
    set_parent(sim.world(), body, panel);
    sim.world().insert(panel, LayoutMotion::default());
    sim.update();
    step(sim, 16);
    (root, panel, body)
}

fn set_height(sim: &mut Sim, e: Entity, h: f32) {
    sim.world().get_mut::<Layout>(e).unwrap().height = Size::Px(h);
}

#[test]
fn size_changes_animate_without_feeding_back_into_layout() {
    let mut sim = Sim::new();
    let (_, panel, body) = accordion(&mut sim);
    assert_eq!(sim.world().get::<Rect>(panel).unwrap().h, 20.0);
    set_height(&mut sim, panel, 100.0);
    sim.update();
    // Drawn at the old size the frame layout grows it.
    let drawn = *sim.world().get::<Rect>(panel).unwrap();
    assert_eq!((drawn.y, drawn.w, drawn.h), (0.0, 100.0, 20.0));
    // Children get the final layout; the clip reveals them.
    assert_eq!(sim.world().get::<Rect>(body).unwrap().h, 80.0);
    assert_eq!(sim.world().get::<Clip>(body).unwrap().h, 20.0);
    step(&mut sim, 120);
    let mid = sim.world().get::<Rect>(panel).unwrap().h;
    assert!(mid > 20.0 && mid < 100.0, "{mid}");
    assert!(close(
        sim.world().get::<Clip>(panel).unwrap().h,
        mid,
        1.0e-3
    ));
    // Input-stage code sees layout's own rect, so hit tests use the final size.
    sim.world().insert_resource(Input::default());
    let mut hits = Vec::new();
    for _ in 0..3 {
        step(&mut sim, 16);
        hits.push(rndr_core::hit_test(sim.world(), 50.0, 90.0));
    }
    step(&mut sim, 3000);
    let end = *sim.world().get::<Rect>(panel).unwrap();
    assert_eq!(end.h, 100.0);
    assert!(!motion::animating(sim.world()));
    // A frame with no change keeps everything put: no feedback loop.
    step(&mut sim, 16);
    assert_eq!(*sim.world().get::<Rect>(panel).unwrap(), end);
    assert_eq!(sim.world().get::<Clip>(body).unwrap().h, 80.0);
    // Shrinking draws the panel larger than its final rect for a while.
    set_height(&mut sim, panel, 40.0);
    sim.update();
    assert_eq!(sim.world().get::<Rect>(panel).unwrap().h, 100.0);
    step(&mut sim, 3000);
    assert_eq!(sim.world().get::<Rect>(panel).unwrap().h, 40.0);
}

#[test]
fn reduced_motion_snaps_size() {
    let mut sim = Sim::new();
    let (_, panel, _) = accordion(&mut sim);
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    set_height(&mut sim, panel, 100.0);
    sim.update();
    assert_eq!(sim.world().get::<Rect>(panel).unwrap().h, 100.0);
}

fn pointer(sim: &mut Sim, at: [f32; 2], pressed: bool, down: bool, released: bool) {
    if sim.world().resource::<Input>().is_none() {
        sim.world().insert_resource(Input::default());
    }
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.pressed = pressed;
    input.pointer.down = down;
    input.pointer.released = released;
    input.pointer.press_at = pressed.then_some(at);
}

fn scale(sim: &mut Sim, e: Entity) -> f32 {
    sim.world().get::<Transform>(e).map_or(1.0, |t| t.scale[0])
}

#[test]
fn press_scale_follows_the_held_press() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let root = boxed(&mut sim, 200.0, 100.0);
    let button = boxed(&mut sim, 60.0, 30.0);
    let other = boxed(&mut sim, 60.0, 30.0);
    set_parent(sim.world(), button, root);
    set_parent(sim.world(), other, root);
    sim.world().insert(button, PressScale(0.9));
    sim.update();
    // A press elsewhere leaves it alone.
    pointer(&mut sim, [30.0, 45.0], true, true, false);
    step(&mut sim, 16);
    pointer(&mut sim, [30.0, 45.0], false, false, true);
    step(&mut sim, 300);
    assert_eq!(scale(&mut sim, button), 1.0);

    pointer(&mut sim, [30.0, 15.0], true, true, false);
    sim.update();
    pointer(&mut sim, [30.0, 15.0], false, true, false);
    step(&mut sim, 500);
    assert!(close(scale(&mut sim, button), 0.9, 1.0e-3));
    // Leaving while held springs back; coming back presses again.
    pointer(&mut sim, [150.0, 90.0], false, true, false);
    sim.update();
    step(&mut sim, 1000);
    assert!(close(scale(&mut sim, button), 1.0, 1.0e-3));
    pointer(&mut sim, [30.0, 15.0], false, true, false);
    sim.update();
    step(&mut sim, 500);
    assert!(close(scale(&mut sim, button), 0.9, 1.0e-3));
    // Release springs back with an overshoot past 1.
    pointer(&mut sim, [30.0, 15.0], false, false, true);
    sim.update();
    pointer(&mut sim, [30.0, 15.0], false, false, false);
    let mut peak: f32 = 0.0;
    for _ in 0..60 {
        step(&mut sim, 16);
        peak = peak.max(scale(&mut sim, button));
    }
    assert!(peak > 1.0, "{peak}");
    assert!(close(scale(&mut sim, button), 1.0, 1.0e-3));
    // Layout is untouched throughout.
    assert_eq!(sim.world().get::<Rect>(button).unwrap().w, 60.0);
}

#[test]
fn press_scale_is_off_under_reduced_motion() {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let button = boxed(&mut sim, 60.0, 30.0);
    sim.world().insert(button, PressScale::default());
    sim.world().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    sim.update();
    pointer(&mut sim, [30.0, 15.0], true, true, false);
    step(&mut sim, 200);
    assert_eq!(scale(&mut sim, button), 1.0);
}
