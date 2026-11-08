//! Scroll glides, fling, rubber-banding and shared-element flights on a
//! manual clock.

use std::sync::Arc;
use std::time::Duration;

use rndr_core::motion::{self, Curve, Easing, MotionSettings, Prop, ScrollPhysics};
use rndr_core::{
    set_parent, App, Color, Corners, Entity, Input, Layout, ManualClock, Opacity, Rect, Scroll,
    Shell, Size, Transform, Viewport,
};

struct Rig {
    app: App,
    clock: Arc<ManualClock>,
}

impl Rig {
    fn new() -> Self {
        let mut app = App::new();
        let clock = Arc::new(ManualClock::new());
        app.set_clock(Arc::clone(&clock) as Arc<dyn rndr_core::clock::Clock>);
        app.add_plugin(Shell);
        app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
        if app.world_mut().resource::<Input>().is_none() {
            app.world_mut().insert_resource(Input::default());
        }
        Self { app, clock }
    }

    fn step(&mut self, ms: u64) {
        self.clock.advance(Duration::from_millis(ms));
        self.app.update();
    }

    fn pointer(&mut self, x: f32, y: f32, pressed: bool, down: bool) {
        let input = self.app.world_mut().resource_mut::<Input>().unwrap();
        let was = input.pointer.down;
        input.pointer.position = [x, y];
        input.pointer.pressed = pressed;
        input.pointer.press_at = pressed.then_some([x, y]);
        input.pointer.released = was && !down;
        input.pointer.down = down;
    }

    fn offset(&mut self, port: Entity) -> f32 {
        self.app.world_mut().get::<Scroll>(port).unwrap().offset[1]
    }

    fn top(&mut self, child: Entity) -> f32 {
        self.app.world_mut().get::<Rect>(child).unwrap().y
    }
}

fn px(w: f32, h: f32) -> Layout {
    Layout {
        width: Size::Px(w),
        height: Size::Px(h),
        ..Layout::column()
    }
}

/// A 200×100 port over ten 40-point rows: offsets run from 0 to -300.
fn list(rig: &mut Rig, physics: bool) -> (Entity, Entity) {
    let world = rig.app.world_mut();
    let port = world.spawn((px(200.0, 100.0), Scroll::default()));
    if physics {
        world.insert(port, ScrollPhysics::default());
    }
    let mut first = None;
    for _ in 0..10 {
        let row = world.spawn((px(200.0, 40.0),));
        set_parent(world, row, port);
        first.get_or_insert(row);
    }
    rig.step(16);
    (port, first.unwrap())
}

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn scroll_to_glides_and_settles_on_the_clamped_target() {
    let mut rig = Rig::new();
    let (port, first) = list(&mut rig, false);
    motion::scroll_to(
        rig.app.world_mut(),
        port,
        [0.0, -900.0],
        Curve::tween(1.0, Easing::Linear),
    );
    rig.step(500);
    assert!(close(rig.offset(port), -150.0, 1.0), "{}", rig.offset(port));
    assert!(close(rig.top(first), rig.offset(port), 0.01));
    assert!(motion::animating(rig.app.world_mut()));
    rig.step(600);
    assert_eq!(rig.offset(port), -300.0);
    assert!(!motion::is_scrolling(rig.app.world_mut(), port));
    motion::scroll_by(
        rig.app.world_mut(),
        port,
        [0.0, 100.0],
        Curve::tween(0.2, Easing::Linear),
    );
    rig.step(300);
    assert_eq!(rig.offset(port), -200.0);
}

#[test]
fn a_wheel_stops_a_glide() {
    let mut rig = Rig::new();
    let (port, _) = list(&mut rig, false);
    motion::scroll_to(
        rig.app.world_mut(),
        port,
        [0.0, -300.0],
        Curve::tween(1.0, Easing::Linear),
    );
    rig.step(200);
    let before = rig.offset(port);
    rig.pointer(100.0, 50.0, false, false);
    rig.app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, -10.0];
    rig.step(16);
    let wheeled = rig.offset(port);
    assert!(close(wheeled, before - 10.0, 6.0), "{before} {wheeled}");
    rig.step(300);
    rig.step(300);
    assert_eq!(rig.offset(port), wheeled);
    assert!(!motion::is_scrolling(rig.app.world_mut(), port));
}

#[test]
fn reduced_motion_jumps_to_the_target() {
    let mut rig = Rig::new();
    rig.app.world_mut().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    let (port, _) = list(&mut rig, false);
    motion::scroll_to(
        rig.app.world_mut(),
        port,
        [0.0, -120.0],
        Curve::spring(0.5, 0.0),
    );
    rig.step(16);
    assert_eq!(rig.offset(port), -120.0);
}

// Press at y, then move `per` points every 16 ms for `frames` frames.
fn drag(rig: &mut Rig, y: f32, per: f32, frames: usize) -> f32 {
    rig.pointer(100.0, y, true, true);
    rig.step(16);
    let mut y = y;
    for _ in 0..frames {
        y += per;
        rig.pointer(100.0, y, false, true);
        rig.step(16);
    }
    y
}

#[test]
fn dragging_past_an_edge_rubber_bands_and_springs_back() {
    let mut rig = Rig::new();
    let (port, first) = list(&mut rig, true);
    let y = drag(&mut rig, 20.0, 10.0, 10);
    // 100 points of travel minus the slop frame, past the top edge.
    let shown = rig.top(first);
    assert_eq!(rig.offset(port), 0.0);
    assert!(shown > 10.0 && shown < 60.0, "{shown}");
    // Hold still, so the release doesn't fling.
    rig.step(100);
    rig.pointer(100.0, y, false, false);
    rig.step(16);
    rig.step(100);
    let mid = rig.top(first);
    assert!(mid > 0.0 && mid < shown, "{mid}");
    rig.step(1500);
    assert!(close(rig.top(first), 0.0, 0.05), "{}", rig.top(first));
    assert!(!motion::is_scrolling(rig.app.world_mut(), port));
}

#[test]
fn a_release_flings_into_the_end_and_bounces_back() {
    let mut rig = Rig::new();
    let (port, _) = list(&mut rig, true);
    let y = drag(&mut rig, 90.0, -15.0, 5);
    rig.pointer(100.0, y, false, false);
    rig.step(16);
    let released = rig.offset(port);
    rig.step(100);
    let coasting = rig.offset(port);
    assert!(coasting < released - 40.0, "{released} {coasting}");
    rig.step(3000);
    rig.step(1000);
    assert_eq!(rig.offset(port), -300.0);
    assert!(!motion::is_scrolling(rig.app.world_mut(), port));
    assert!(!motion::animating(rig.app.world_mut()));
}

#[test]
fn a_press_catches_a_fling() {
    let mut rig = Rig::new();
    let (port, _) = list(&mut rig, true);
    let y = drag(&mut rig, 90.0, -8.0, 4);
    rig.pointer(100.0, y, false, false);
    rig.step(16);
    rig.step(50);
    rig.pointer(100.0, 50.0, true, true);
    rig.step(16);
    let caught = rig.offset(port);
    rig.pointer(100.0, 50.0, false, true);
    rig.step(200);
    assert_eq!(rig.offset(port), caught);
    rig.pointer(100.0, 50.0, false, false);
    rig.step(16);
    rig.step(500);
    assert_eq!(rig.offset(port), caught);
}

#[test]
fn without_physics_a_drag_stops_at_release_and_the_edge() {
    let mut rig = Rig::new();
    let (port, first) = list(&mut rig, false);
    let y = drag(&mut rig, 90.0, -15.0, 5);
    rig.pointer(100.0, y, false, false);
    rig.step(16);
    let released = rig.offset(port);
    rig.step(300);
    assert_eq!(rig.offset(port), released);
    drag(&mut rig, 10.0, 30.0, 4);
    assert_eq!(rig.top(first), 0.0);
}

const RED: Color = Color::rgb(1.0, 0.0, 0.0);
const BLUE: Color = Color::rgb(0.0, 0.0, 1.0);

fn pair(rig: &mut Rig) -> (Entity, Entity) {
    let world = rig.app.world_mut();
    let root = world.spawn((Layout {
        width: Size::Px(200.0),
        height: Size::Px(100.0),
        ..Layout::row()
    },));
    let thumb = world.spawn((px(40.0, 20.0), RED, Corners::all(10.0)));
    let detail = world.spawn((px(120.0, 80.0), BLUE));
    set_parent(world, thumb, root);
    set_parent(world, detail, root);
    rig.step(16);
    (thumb, detail)
}

fn drawn(rig: &mut Rig, e: Entity) -> [f32; 4] {
    let world = rig.app.world_mut();
    let rect = *world.get::<Rect>(e).unwrap();
    let t = world.get::<Transform>(e).map_or([0.0; 2], |t| t.translate);
    [rect.x + t[0], rect.y + t[1], rect.w, rect.h]
}

#[test]
fn fly_starts_where_from_is_drawn_and_lands_on_its_own_layout() {
    let mut rig = Rig::new();
    let (thumb, detail) = pair(&mut rig);
    let from = drawn(&mut rig, thumb);
    let to = drawn(&mut rig, detail);
    assert_ne!(from, to);
    motion::fly(
        rig.app.world_mut(),
        thumb,
        detail,
        Curve::tween(1.0, Easing::Linear),
    );
    rig.step(16);
    let start = drawn(&mut rig, detail);
    for (a, b) in start.iter().zip(from) {
        assert!(close(*a, b, 0.5), "{start:?} {from:?}");
    }
    assert_eq!(
        *rig.app.world_mut().get::<Opacity>(thumb).unwrap(),
        Opacity(0.0)
    );
    let corners = rig.app.world_mut().get::<Corners>(detail).unwrap().0;
    assert!(corners[0] > 9.0, "{corners:?}");
    rig.step(484);
    let mid = drawn(&mut rig, detail);
    for k in 0..4 {
        assert!(close(mid[k], (from[k] + to[k]) / 2.0, 1.5), "{mid:?}");
    }
    let fill = *rig.app.world_mut().get::<Color>(detail).unwrap();
    assert!(fill.r > 0.1 && fill.b > 0.1, "{fill:?}");
    rig.step(600);
    assert_eq!(drawn(&mut rig, detail), to);
    let end = *rig.app.world_mut().get::<Color>(detail).unwrap();
    for (a, b) in [
        (end.r, BLUE.r),
        (end.g, BLUE.g),
        (end.b, BLUE.b),
        (end.a, 1.0),
    ] {
        assert!(close(a, b, 2.0e-3), "{end:?}");
    }
    assert_eq!(
        rig.app.world_mut().get::<Corners>(detail).unwrap().0,
        [0.0; 4]
    );
    // Flying back shows the thumbnail again.
    motion::fly(rig.app.world_mut(), detail, thumb, Curve::spring(0.3, 0.0));
    rig.step(16);
    assert_eq!(
        motion::value(rig.app.world_mut(), thumb, Prop::Opacity),
        Some(1.0)
    );
    assert_eq!(
        motion::value(rig.app.world_mut(), detail, Prop::Opacity),
        Some(0.0)
    );
}

#[test]
fn reduced_motion_fly_fades_in_place() {
    let mut rig = Rig::new();
    rig.app.world_mut().insert_resource(MotionSettings {
        reduced_motion: true,
        ..MotionSettings::default()
    });
    let (thumb, detail) = pair(&mut rig);
    let to = drawn(&mut rig, detail);
    motion::fly(rig.app.world_mut(), thumb, detail, Curve::spring(0.4, 0.0));
    rig.step(16);
    assert_eq!(drawn(&mut rig, detail), to);
    let fading = rig.app.world_mut().get::<Opacity>(detail).unwrap().0;
    assert!(fading < 0.5, "{fading}");
    rig.step(400);
    assert_eq!(
        *rig.app.world_mut().get::<Opacity>(detail).unwrap(),
        Opacity(1.0)
    );
}

#[test]
fn scroll_to_after_a_fling_glides() {
    let mut rig = Rig::new();
    let (port, _) = list(&mut rig, true);
    let y = drag(&mut rig, 90.0, -15.0, 5);
    rig.pointer(100.0, y, false, false);
    rig.step(16);
    rig.step(3000);
    rig.step(1000);
    motion::scroll_to(
        rig.app.world_mut(),
        port,
        [0.0, 0.0],
        Curve::spring(0.6, 0.0),
    );
    rig.step(0);
    // The frame that starts a glide asks for the next one.
    assert!(motion::animating(rig.app.world_mut()));
    rig.step(16);
    rig.step(2000);
    assert_eq!(rig.offset(port), 0.0);
}
