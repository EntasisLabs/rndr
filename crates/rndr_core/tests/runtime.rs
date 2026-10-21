//! Headless acceptance tests. The manual clock never sleeps.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rndr_core::{
    after, debounce, events, every, sleep, spawn, spawn_local, Event, Schedule, Sim, Time, World,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct QueryResult(String);

impl Event for QueryResult {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Tick;

impl Event for Tick {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ping;

impl Event for Ping {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LocalDone;

impl Event for LocalDone {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Called(u8);

impl Event for Called {}

#[test]
fn query_result_arrives_as_event() {
    let mut sim = Sim::new();
    let handle = spawn(sim.world(), async { QueryResult("files".into()) });
    sim.run_until_parked();
    assert_eq!(events::<QueryResult>(sim.world()).len(), 1);
    assert_eq!(events::<QueryResult>(sim.world())[0].0, "files");
    drop(handle);
}

fn count_queries(world: &mut World) {
    let n = events::<QueryResult>(world).len();
    world.insert_resource(Seen(n));
}

struct Seen(usize);

#[test]
fn update_system_observes_query_event() {
    let mut sim = Sim::new();
    sim.app.add_system(Schedule::Update, count_queries);
    let handle = spawn(sim.world(), async { QueryResult("files".into()) });
    sim.update();
    assert_eq!(sim.world().resource::<Seen>().unwrap().0, 1);
    drop(handle);
}

#[test]
fn dropping_handle_drops_stale_result() {
    let mut sim = Sim::new();
    let handle = spawn(sim.world(), async {
        sleep(Duration::from_millis(30)).await;
        QueryResult("stale".into())
    });
    sim.update();
    assert!(events::<QueryResult>(sim.world()).is_empty());

    drop(handle);
    sim.advance(Duration::from_millis(30));
    sim.update();
    assert!(
        events::<QueryResult>(sim.world()).is_empty(),
        "cancelled query must not commit"
    );
}

#[test]
fn kept_handle_delivers_after_sleep() {
    let mut sim = Sim::new();
    let handle = spawn(sim.world(), async {
        sleep(Duration::from_millis(30)).await;
        QueryResult("fresh".into())
    });
    sim.update();
    sim.advance(Duration::from_millis(30));
    sim.update();
    assert_eq!(events::<QueryResult>(sim.world()).len(), 1);
    assert_eq!(events::<QueryResult>(sim.world())[0].0, "fresh");
    drop(handle);
}

#[test]
fn cancel_drops_result_before_poll() {
    let mut sim = Sim::new();
    let handle = spawn(sim.world(), async { QueryResult("nope".into()) });
    handle.cancel();
    sim.run_until_parked();
    sim.update();
    assert!(events::<QueryResult>(sim.world()).is_empty());
}

#[test]
fn detach_still_delivers() {
    let mut sim = Sim::new();
    let handle = spawn(sim.world(), async { QueryResult("kept".into()) });
    handle.detach();
    sim.update();
    assert_eq!(events::<QueryResult>(sim.world())[0].0, "kept");
}

#[test]
fn debounce_coalesces_rapid_keystrokes() {
    let mut sim = Sim::new();
    let calls = Arc::new(AtomicUsize::new(0));
    for keystroke in 0..5u8 {
        let calls = Arc::clone(&calls);
        debounce(sim.world(), "palette", Duration::from_millis(50), {
            let calls = Arc::clone(&calls);
            move || {
                let calls = Arc::clone(&calls);
                let keystroke = keystroke;
                async move {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Called(keystroke)
                }
            }
        });
        sim.advance(Duration::from_millis(10));
        sim.run_until_parked();
        assert_eq!(
            calls.load(Ordering::SeqCst),
            0,
            "keystroke {keystroke} fired early"
        );
    }
    // Last arm was at t=40ms with a 50ms delay, so it is due at t=90ms.
    sim.advance(Duration::from_millis(40));
    sim.update();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(events::<Called>(sim.world()), &[Called(4)]);
}

#[test]
fn timer_fires_at_fake_time() {
    let mut sim = Sim::new();
    let handle = after(sim.world(), Duration::from_millis(500), Ping);
    sim.advance(Duration::from_millis(499));
    sim.update();
    assert!(events::<Ping>(sim.world()).is_empty());
    let time = *sim.world().resource::<Time>().unwrap();
    assert_eq!(time.now, Duration::from_millis(499));
    assert_eq!(time.delta, Duration::from_millis(499));
    assert_eq!(time.frame, 1);

    sim.advance(Duration::from_millis(1));
    sim.update();
    assert_eq!(events::<Ping>(sim.world()), &[Ping]);
    let time = *sim.world().resource::<Time>().unwrap();
    assert_eq!(time.now, Duration::from_millis(500));
    assert_eq!(time.delta, Duration::from_millis(1));
    assert_eq!(time.frame, 2);
    drop(handle);
}

#[test]
fn repeating_timer_stops_when_cancelled() {
    let mut sim = Sim::new();
    let handle = every(sim.world(), Duration::from_millis(100), Tick);
    sim.advance(Duration::from_millis(100));
    sim.update();
    assert_eq!(events::<Tick>(sim.world()), &[Tick]);

    handle.cancel();
    sim.advance(Duration::from_millis(100));
    sim.update();
    assert!(events::<Tick>(sim.world()).is_empty());
}

#[test]
fn repeating_timer_stops_when_dropped() {
    let mut sim = Sim::new();
    let handle = every(sim.world(), Duration::from_millis(100), Tick);
    sim.advance(Duration::from_millis(100));
    sim.update();
    assert_eq!(events::<Tick>(sim.world()).len(), 1);
    drop(handle);
    sim.advance(Duration::from_millis(100));
    sim.update();
    assert!(events::<Tick>(sim.world()).is_empty());
}

#[test]
fn foreground_task_holds_non_send_state_across_await() {
    let mut sim = Sim::new();
    let cell = Rc::new(Cell::new(0u32));
    let view = Rc::clone(&cell);
    let handle = spawn_local(sim.world(), async move {
        sleep(Duration::from_millis(15)).await;
        view.set(view.get() + 1);
        LocalDone
    });
    sim.update();
    assert_eq!(cell.get(), 0);
    sim.advance(Duration::from_millis(15));
    sim.update();
    assert_eq!(cell.get(), 1);
    assert_eq!(events::<LocalDone>(sim.world()), &[LocalDone]);
    drop(handle);
}

#[test]
fn system_sends_are_visible_next_frame() {
    let mut sim = Sim::new();
    sim.app.add_system(Schedule::Update, |world| {
        if world.resource::<Time>().unwrap().frame == 1 {
            rndr_core::send(world, Ping);
        }
        let n = events::<Ping>(world).len();
        world.insert_resource(Seen(n));
    });
    sim.update();
    assert_eq!(sim.world().resource::<Seen>().unwrap().0, 0);
    sim.update();
    assert_eq!(sim.world().resource::<Seen>().unwrap().0, 1);
    sim.update();
    assert_eq!(sim.world().resource::<Seen>().unwrap().0, 0);
}

#[test]
fn seeded_ready_order_is_deterministic() {
    let stable = completion_order(None);
    assert_eq!(stable, vec![0, 1, 2, 3, 4, 5]);
    let seeded = completion_order(Some(1));
    assert_eq!(seeded, completion_order(Some(1)));
    assert_ne!(seeded, stable);
    assert_eq!(completion_order(Some(2)), completion_order(Some(2)));
    assert_ne!(completion_order(Some(1)), completion_order(Some(2)));
}

#[cfg(feature = "pool")]
#[test]
fn thread_pool_result_arrives_as_event() {
    let mut app = rndr_core::App::new();
    app.set_executor(rndr_core::thread_pool());
    let handle = spawn(app.world_mut(), async { QueryResult("bg".into()) });
    let mut delivered = false;
    for _ in 0..100_000 {
        app.update();
        if events::<QueryResult>(app.world()).len() == 1 {
            delivered = true;
            break;
        }
        std::thread::yield_now();
    }
    assert!(delivered, "pool did not deliver");
    assert_eq!(events::<QueryResult>(app.world())[0].0, "bg");
    drop(handle);
}

fn completion_order(seed: Option<u64>) -> Vec<u8> {
    let mut sim = match seed {
        Some(seed) => Sim::with_seed(seed),
        None => Sim::new(),
    };
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut handles = Vec::new();
    for i in 0..6u8 {
        let log = Arc::clone(&log);
        handles.push(spawn(sim.world(), async move {
            log.lock().unwrap().push(i);
            Step(i)
        }));
    }
    sim.run_until_parked();
    drop(handles);
    let order = log.lock().unwrap().clone();
    order
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Step(u8);

impl Event for Step {}

#[test]
fn idle_sim_has_no_deadline_until_a_timer_is_armed() {
    let mut sim = Sim::new();
    sim.update();
    assert!(!sim.app.wants_frame());
    assert!(sim.app.next_deadline().is_none());

    let handle = after(sim.world(), Duration::from_millis(500), Ping);
    assert!(sim.app.next_deadline().is_some());
    assert!(!sim.app.wants_frame());

    sim.advance(Duration::from_millis(500));
    assert!(sim.app.wants_frame());
    sim.update();
    assert!(!sim.app.wants_frame());
    assert!(sim.app.next_deadline().is_none());
    drop(handle);
}
