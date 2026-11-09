//! `LiveText`, and live colours past the fill, in live-only frames.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use rndr_core::{
    set_parent, Border, Color, Gradient, GradientStop, Label, Layout, Live, LiveBorder,
    LiveGradient, LiveLabel, LiveShadow, LiveText, LiveTextColor, Rect, Schedule, Shadow, Shadows,
    Sim, Size, TextLayout, Viewport, Wake,
};

#[derive(Default)]
struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn sim() -> (Sim, Arc<CountWake>) {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(300.0, 100.0));
    let wake = Arc::new(CountWake::default());
    sim.app.set_live_wake(wake.clone());
    (sim, wake)
}

fn hug(text: &str) -> (Layout, Label) {
    (
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new(text),
    )
}

#[test]
fn live_text_reshapes_only_its_label_in_a_live_only_frame() {
    struct Runs(u32);
    let (mut sim, _) = sim();
    sim.world().insert_resource(Runs(0));
    sim.app.add_system(Schedule::Update, |world| {
        world.resource_mut::<Runs>().unwrap().0 += 1;
    });
    let root = sim.world().spawn((Layout {
        width: Size::Px(300.0),
        height: Size::Px(100.0),
        ..Layout::row()
    },));
    let clock = sim.world().spawn(hug("0"));
    let other = sim.world().spawn(hug("static"));
    set_parent(sim.world(), clock, root);
    set_parent(sim.world(), other, root);
    let text = LiveText::new("0");
    sim.world().insert(clock, LiveLabel::new(&text));
    sim.update();
    let runs = sim.world().resource::<Runs>().unwrap().0;
    let rect = *sim.world().get::<Rect>(clock).unwrap();
    let beside = *sim.world().get::<Rect>(other).unwrap();
    let narrow = sim.world().get::<TextLayout>(clock).unwrap().width;
    let other_layout = sim.world().get::<TextLayout>(other).unwrap().clone();

    assert!(text.set("12:34:56"));
    sim.app.update_live();
    assert_eq!(sim.world().resource::<Runs>().unwrap().0, runs);
    assert_eq!(sim.world().get::<Label>(clock).unwrap().text, "12:34:56");
    assert!(sim.world().get::<TextLayout>(clock).unwrap().width > narrow);
    assert_eq!(
        *sim.world().get::<Rect>(clock).unwrap(),
        rect,
        "layout keeps the box"
    );
    assert_eq!(*sim.world().get::<Rect>(other).unwrap(), beside);
    assert_eq!(*sim.world().get::<TextLayout>(other).unwrap(), other_layout);

    // The next full frame measures the new text.
    sim.update();
    assert!(sim.world().get::<Rect>(clock).unwrap().w > rect.w);
    assert!(sim.world().get::<Rect>(other).unwrap().x > beside.x);
}

#[test]
fn equal_text_does_not_wake_and_a_change_wakes_once() {
    let (mut sim, wake) = sim();
    let clock = sim.world().spawn(hug("a"));
    let text = LiveText::new("a");
    sim.world().insert(clock, LiveLabel::new(&text));
    sim.update();
    let woken = wake.0.load(Ordering::SeqCst);
    assert!(!text.set("a"));
    assert_eq!(wake.0.load(Ordering::SeqCst), woken);
    assert!(text.set("b"));
    assert!(text.set("c"));
    assert_eq!(wake.0.load(Ordering::SeqCst), woken + 1);
    assert!(sim.app.wants_frame());
    sim.app.update_live();
    assert_eq!(sim.world().get::<Label>(clock).unwrap().text, "c");
    assert!(!sim.app.wants_frame());
    assert_eq!(text.version(), 2);
}

#[test]
fn a_live_label_without_a_label_gets_one() {
    let (mut sim, _) = sim();
    let e = sim.world().spawn((Layout {
        width: Size::Px(80.0),
        height: Size::Px(20.0),
        ..Layout::column()
    },));
    let text = LiveText::new("hello");
    sim.world().insert(e, LiveLabel::new(&text));
    sim.update();
    assert_eq!(sim.world().get::<Label>(e).unwrap().text, "hello");
    assert!(sim.world().get::<TextLayout>(e).is_some());
}

#[test]
fn colour_bindings_write_text_border_shadow_and_gradient_ends() {
    let (mut sim, _) = sim();
    let red = Color::rgb(1.0, 0.0, 0.0);
    let grey = Color::rgb(0.5, 0.5, 0.5);
    let e = sim.world().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Label::new("hi"),
        Border {
            color: grey,
            width: 2.0,
        },
        Shadows(vec![
            Shadow::drop(4.0, 8.0, grey),
            Shadow::drop(1.0, 1.0, grey),
        ]),
        Gradient::Linear {
            angle: 0.0,
            stops: vec![
                GradientStop::new(0.0, grey),
                GradientStop::new(0.5, grey),
                GradientStop::new(1.0, grey),
            ],
        },
    ));
    let ink = Live::new(red);
    let edge = Live::new(red);
    let glow = Live::new(red);
    let top = Live::new(red);
    let bottom = Live::new(red);
    sim.world().insert(e, LiveTextColor::new(&ink));
    sim.world().insert(e, LiveBorder::new(&edge));
    sim.world().insert(e, LiveShadow::new(&glow));
    sim.world().insert(e, LiveGradient::new(&top, &bottom));
    sim.update();
    let layout = sim.world().get::<TextLayout>(e).unwrap().clone();

    let blue = Color::rgb(0.0, 0.0, 1.0);
    let green = Color::rgb(0.0, 1.0, 0.0);
    ink.set(blue);
    edge.set(green);
    glow.set(blue);
    top.set(green);
    bottom.set(blue);
    sim.app.update_live();
    let world = sim.world();
    assert_eq!(world.get::<Label>(e).unwrap().color, Some(blue));
    assert_eq!(*world.get::<TextLayout>(e).unwrap(), layout, "not reshaped");
    assert_eq!(world.get::<Border>(e).unwrap().color, green);
    let shadows = &world.get::<Shadows>(e).unwrap().0;
    assert_eq!((shadows[0].color, shadows[1].color), (blue, grey));
    assert_eq!((shadows[0].offset, shadows[0].blur), ([0.0, 4.0], 8.0));
    let Gradient::Linear { stops, .. } = world.get::<Gradient>(e).unwrap() else {
        panic!("linear");
    };
    let colors: Vec<Color> = stops.iter().map(|s| s.color).collect();
    assert_eq!(colors, [green, grey, blue]);
}

#[test]
fn colour_bindings_without_their_component_do_nothing() {
    let (mut sim, _) = sim();
    let e = sim.world().spawn((Layout::column(),));
    let tint = Live::new(Color::rgb(1.0, 0.0, 0.0));
    sim.world().insert(e, LiveTextColor::new(&tint));
    sim.world().insert(e, LiveBorder::new(&tint));
    sim.world().insert(e, LiveShadow::new(&tint));
    sim.world().insert(e, LiveGradient::new(&tint, &tint));
    sim.update();
    let world = sim.world();
    assert!(world.get::<Label>(e).is_none());
    assert!(world.get::<Border>(e).is_none());
    assert!(world.get::<Shadows>(e).is_none());
    assert!(world.get::<Gradient>(e).is_none());
}
