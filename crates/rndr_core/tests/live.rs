//! `Live` values: producer threads, wakes, bindings and live-only frames.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rndr_core::live;
use rndr_core::motion::Spring;
use rndr_core::{
    Color, Label, Layout, Live, LiveFill, LiveOffset, LiveOpacity, LiveRect, LiveScale, Opacity,
    Rect, Schedule, Sim, Size, Transform, Viewport, Wake, WindowPhase, World,
};

#[derive(Default)]
struct CountWake(AtomicUsize);

impl Wake for CountWake {
    fn wake(&self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

impl CountWake {
    fn count(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }
}

fn sim_with_wake() -> (Sim, Arc<CountWake>) {
    let mut sim = Sim::new();
    sim.world().insert_resource(Viewport::new(200.0, 100.0));
    let wake = Arc::new(CountWake::default());
    sim.app.set_live_wake(wake.clone());
    (sim, wake)
}

fn card(world: &mut World) -> rndr_core::Entity {
    world.spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Color::rgb(1.0, 1.0, 1.0),
    ))
}

#[test]
fn producer_threads_leave_the_latest_value() {
    let value = Live::new(0.0_f32);
    let workers: Vec<_> = (0..4)
        .map(|worker| {
            let value = value.clone();
            std::thread::spawn(move || {
                for i in 1..=1000 {
                    value.set((worker * 1000 + i) as f32);
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    assert_eq!(value.version(), 4000);
    assert!([1000.0, 2000.0, 3000.0, 4000.0].contains(&value.get()));
}

#[test]
fn a_set_wakes_once_until_the_next_frame_and_equal_values_do_not_wake() {
    let (mut sim, wake) = sim_with_wake();
    let e = card(sim.world());
    let fade = Live::new(1.0_f32);
    sim.world().insert(e, LiveOpacity::new(&fade));
    sim.update();
    assert!(!sim.app.wants_frame());

    assert!(!fade.set(1.0), "equal value is ignored");
    assert_eq!(wake.count(), 0);
    assert!(!live::pending(sim.world()));

    let producer = {
        let fade = fade.clone();
        std::thread::spawn(move || {
            for i in 0..100 {
                fade.set(i as f32 / 100.0);
            }
        })
    };
    producer.join().unwrap();
    assert_eq!(wake.count(), 1, "coalesced into one wake");
    assert!(live::pending(sim.world()));
    assert!(sim.app.wants_frame());
    assert!(!sim.app.wants_update(), "a live-only frame is enough");

    sim.app.update_live();
    assert_eq!(sim.world().get::<Opacity>(e).unwrap().0, 0.99);
    assert!(!sim.app.wants_frame());

    fade.set(0.5);
    assert_eq!(wake.count(), 2, "the next change after a frame wakes again");
}

#[test]
fn hidden_windows_are_not_woken() {
    let (mut sim, wake) = sim_with_wake();
    let e = card(sim.world());
    let fade = Live::new(1.0_f32);
    sim.world().insert(e, LiveOpacity::new(&fade));
    sim.world().insert_resource(WindowPhase {
        visible: false,
        ..WindowPhase::default()
    });
    sim.update();
    fade.set(0.25);
    assert_eq!(wake.count(), 0);
    assert!(!sim.app.wants_frame());

    sim.world().insert_resource(WindowPhase::default());
    sim.update();
    assert_eq!(sim.world().get::<Opacity>(e).unwrap().0, 0.25);
    fade.set(0.5);
    assert_eq!(wake.count(), 1);
}

#[test]
fn one_handle_wakes_every_window_that_draws_it() {
    let (mut a, wake_a) = sim_with_wake();
    let (mut b, wake_b) = sim_with_wake();
    let (mut c, wake_c) = sim_with_wake();
    let fade = Live::new(1.0_f32);
    for sim in [&mut a, &mut b] {
        let e = card(sim.world());
        sim.world().insert(e, LiveOpacity::new(&fade));
        sim.update();
    }
    card(c.world());
    c.update();
    fade.set(0.5);
    assert_eq!((wake_a.count(), wake_b.count(), wake_c.count()), (1, 1, 0));
    assert!(!c.app.wants_frame());
}

#[test]
fn live_only_frames_skip_update_and_layout() {
    struct Runs(u32);
    let (mut sim, _) = sim_with_wake();
    sim.world().insert_resource(Runs(0));
    sim.app.add_system(Schedule::Update, |world| {
        world.resource_mut::<Runs>().unwrap().0 += 1;
    });
    let root = sim.world().spawn((Layout {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Layout::row()
    },));
    let text = sim.world().spawn((
        Layout {
            width: Size::Hug,
            height: Size::Hug,
            ..Layout::column()
        },
        Label::new("wrapping label"),
    ));
    let sibling = card(sim.world());
    rndr_core::set_parent(sim.world(), text, root);
    rndr_core::set_parent(sim.world(), sibling, root);
    let place = Live::new([0.0_f32; 4]);
    sim.update();
    let laid = *sim.world().get::<Rect>(text).unwrap();
    let beside = *sim.world().get::<Rect>(sibling).unwrap();
    place.set([laid.x, laid.y, laid.w, laid.h]);
    sim.world().insert(text, LiveRect::new(&place));
    sim.update();
    let runs = sim.world().resource::<Runs>().unwrap().0;

    place.set([50.0, 60.0, laid.w * 2.0, laid.h * 2.0]);
    sim.app.update_live();
    assert_eq!(sim.world().resource::<Runs>().unwrap().0, runs);
    assert_eq!(*sim.world().get::<Rect>(text).unwrap(), laid);
    assert_eq!(*sim.world().get::<Rect>(sibling).unwrap(), beside);
    let t = *sim.world().get::<Transform>(text).unwrap();
    assert_eq!(t.origin, [0.0, 0.0]);
    assert_eq!(t.translate, [50.0 - laid.x, 60.0 - laid.y]);
    assert_eq!(t.scale, [2.0, 2.0]);
}

#[test]
fn bindings_write_opacity_offset_scale_and_fill() {
    let (mut sim, _) = sim_with_wake();
    let e = card(sim.world());
    let fade = Live::new(0.3_f32);
    let shift = Live::new([4.0_f32, -2.0]);
    let grow = Live::new([1.5_f32, 0.5]);
    let tint = Live::new(Color::rgb(0.0, 0.5, 1.0));
    sim.world().insert(e, LiveOpacity::new(&fade));
    sim.world().insert(e, LiveOffset::new(&shift));
    sim.world().insert(e, LiveScale::new(&grow));
    sim.world().insert(e, LiveFill::new(&tint));
    sim.update();
    assert_eq!(sim.world().get::<Opacity>(e).unwrap().0, 0.3);
    let t = *sim.world().get::<Transform>(e).unwrap();
    assert_eq!((t.translate, t.scale), ([4.0, -2.0], [1.5, 0.5]));
    assert_eq!(
        *sim.world().get::<Color>(e).unwrap(),
        Color::rgb(0.0, 0.5, 1.0)
    );

    shift.set([10.0, 0.0]);
    tint.set(Color::rgb(1.0, 0.0, 0.0));
    sim.app.update_live();
    let t = *sim.world().get::<Transform>(e).unwrap();
    assert_eq!((t.translate, t.scale), ([10.0, 0.0], [1.5, 0.5]));
    assert_eq!(
        *sim.world().get::<Color>(e).unwrap(),
        Color::rgb(1.0, 0.0, 0.0)
    );
}

#[test]
fn a_spring_binding_follows_and_keeps_frames_coming_until_it_settles() {
    let (mut sim, wake) = sim_with_wake();
    let e = card(sim.world());
    let x = Live::new([0.0_f32, 0.0]);
    sim.world()
        .insert(e, LiveOffset::spring(&x, Spring::SMOOTH));
    sim.update();
    x.set([100.0, 0.0]);
    assert_eq!(wake.count(), 1);

    sim.advance(Duration::from_millis(50));
    sim.app.update_live();
    let first = sim.world().get::<Transform>(e).unwrap().translate[0];
    assert!(first > 0.0 && first < 100.0, "{first}");
    assert!(live::pending(sim.world()), "still settling");
    assert!(!sim.app.wants_update());

    sim.advance(Duration::from_millis(50));
    sim.app.update_live();
    let second = sim.world().get::<Transform>(e).unwrap().translate[0];
    assert!(second > first && second < 100.0, "{second}");

    for _ in 0..200 {
        sim.advance(Duration::from_millis(16));
        sim.app.update_live();
    }
    assert_eq!(
        sim.world().get::<Transform>(e).unwrap().translate,
        [100.0, 0.0]
    );
    assert!(!live::pending(sim.world()));
    assert!(!sim.app.wants_frame());
}
