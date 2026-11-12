use rndr_core::{FeedConfig, Sim};
use rndr_flow::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
struct PanicDrop(Arc<AtomicBool>);
impl Drop for PanicDrop {
    fn drop(&mut self) {
        if self.0.swap(false, Ordering::SeqCst) {
            panic!("single payload destructor panic");
        }
    }
}
#[test]
fn producer_success_must_not_erase_payload_cleanup_panic() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let scope = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let (_, sender, exit, status) = flow
        .replace_feed::<_, PanicDrop>(
            sim.world(),
            scope.id(),
            LaneKey(1),
            Identity {
                target: 1u64,
                revision: 0,
                operation: 1,
            },
            DeliveryMode::Once,
            FeedConfig::queue(1),
        )
        .unwrap();
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(0);
    let (finish_tx, finish_rx) = std::sync::mpsc::sync_channel(0);
    let producer = std::thread::spawn(move || {
        exit.run(|| {
            sender
                .send(None, PanicDrop(Arc::new(AtomicBool::new(true))))
                .unwrap();
            ready_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Ok::<_, TaskFailure>(())
        })
    });
    ready_rx.recv().unwrap();
    drop(scope);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(status.observation().terminal, Terminal::Panicked);
    assert!(!status.observation().exit_acknowledged);
    finish_tx.send(()).unwrap();
    assert_eq!(producer.join().unwrap(), Ok(()));
    println!("after producer success: {:?}", status.observation());
    assert_eq!(
        status.observation().terminal,
        Terminal::Panicked,
        "a known cancellation/destructor panic must remain visible after producer succeeds"
    );
}

struct EffectCapture {
    status: Arc<std::sync::Mutex<Option<WorkStatus>>>,
    observed: Arc<std::sync::Mutex<Option<Observation>>>,
}
impl Drop for EffectCapture {
    fn drop(&mut self) {
        *self.observed.lock().unwrap() =
            Some(self.status.lock().unwrap().as_ref().unwrap().observation());
        panic!("single abandoned effect capture destructor panic");
    }
}
#[test]
fn world_abandonment_must_not_ack_before_capture_destruction_or_hide_panic() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut world = rndr_core::World::new();
    let owner = flow.register_world().unwrap();
    let scope = flow.create_scope(owner.scope()).unwrap();
    owner.bind(&mut world).unwrap();
    let req = flow
        .replace_request(
            &world,
            scope.id(),
            LaneKey(1),
            Identity {
                target: 1u64,
                revision: 0,
                operation: 1,
            },
            DeliveryMode::Once,
        )
        .unwrap();
    let shared = Arc::new(std::sync::Mutex::new(None));
    let observed = Arc::new(std::sync::Mutex::new(None));
    let capture = EffectCapture {
        status: shared.clone(),
        observed: observed.clone(),
    };
    let status = flow
        .own_world_effect(&mut world, &req, move |_| drop(capture))
        .unwrap();
    *shared.lock().unwrap() = Some(status.clone());
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(world)));
    let during = observed.lock().unwrap().unwrap();
    println!(
        "during capture destructor: {during:?}; after: {:?}",
        status.observation()
    );
    assert!(
        !during.exit_acknowledged,
        "effect acknowledged while capture destructor has not even run to completion"
    );
    assert_eq!(status.observation().terminal, Terminal::Panicked);
}

#[test]
fn timer_payload_destructor_must_be_normalized_before_exit_ack() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let owner = flow.register_world().unwrap();
    let scope = flow.create_scope(owner.scope()).unwrap();
    owner.bind(sim.world()).unwrap();
    let shared = Arc::new(std::sync::Mutex::new(None));
    let observed = Arc::new(std::sync::Mutex::new(None));
    let capture = EffectCapture {
        status: shared.clone(),
        observed: observed.clone(),
    };
    let (_, status) = flow
        .replace_timer(
            sim.world(),
            scope.id(),
            LaneKey(1),
            Identity {
                target: 1u64,
                revision: 0,
                operation: 1,
            },
            std::time::Duration::from_secs(999),
            capture,
        )
        .ok()
        .unwrap();
    *shared.lock().unwrap() = Some(status.clone());
    drop(scope);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    let update = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| sim.update()));
    let during = observed
        .lock()
        .unwrap()
        .expect("cancelled timer payload was dropped");
    println!(
        "timer update panicked={}; during destructor: {during:?}; after: {:?}",
        update.is_err(),
        status.observation()
    );
    assert_eq!(
        status.observation().terminal,
        Terminal::Panicked,
        "single timer payload destructor panic is not an unknown/successful exit"
    );
    assert!(!during.exit_acknowledged);
    assert!(
        update.is_ok(),
        "single owned timer destructor panic escaped update"
    );
    assert_eq!(during.pending_deliveries, 1);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(status.observation().pending_deliveries, 0);
}
