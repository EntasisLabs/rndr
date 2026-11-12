//! Independent public-API probes. No source fixes or private status injection.
use rndr_core::{FeedConfig, Sim};
use rndr_flow::*;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

struct Output {
    panic: bool,
    drops: Arc<AtomicUsize>,
}
impl Drop for Output {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
        assert!(
            !std::thread::panicking(),
            "probe must not drop during an existing unwind"
        );
        if self.panic {
            panic!("single discarded output destructor panic; no unwind was active");
        }
    }
}
struct QueueValue;
impl Drop for QueueValue {
    fn drop(&mut self) {
        panic!("earlier queue cleanup panic; caught before producer is released");
    }
}
fn pump(flow: &mut Coordinator) {
    for _ in 0..400 {
        flow.pump_work(1);
    }
}
fn id() -> Identity<u64> {
    Identity {
        target: 1,
        operation: 1,
        revision: 0,
    }
}
fn discarded_success(panic: bool) {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let (_, sender, exit, status) = flow
        .replace_feed::<_, QueueValue>(
            sim.world(),
            owner.id(),
            LaneKey(1),
            id(),
            DeliveryMode::Once,
            FeedConfig::queue(1),
        )
        .unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let output_drops = drops.clone();
    let (ready_tx, ready_rx) = mpsc::sync_channel(0);
    let (finish_tx, finish_rx) = mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        let mut tracked = Box::pin(exit.track(async move {
            sender.try_send(None, QueueValue).unwrap();
            ready_tx.send(()).unwrap();
            finish_rx.recv().unwrap();
            Ok::<_, TaskFailure>(Output {
                panic,
                drops: output_drops,
            })
        }));
        // Only this outer REVIEW boundary catches the escaped poll panic.
        let result = catch_unwind(AssertUnwindSafe(|| {
            tracked
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
        }));
        match result {
            Ok(Poll::Ready(Err(TaskFailure::Panicked))) => true,
            Err(_) => false,
            _ => panic!("unexpected tracked output"),
        }
    });
    ready_rx.recv().unwrap();
    drop(owner);
    pump(&mut flow); // the first unwind is completely caught and has returned
    let before = status.observation();
    assert_eq!(before.terminal, Terminal::Panicked);
    assert!(before.cleanup_attempted);
    assert!(!before.exit_acknowledged);
    assert_eq!(flow.work_counts().charged, 1);
    pump(&mut flow);
    assert!(!status.observation().exit_acknowledged);
    finish_tx.send(()).unwrap();
    let normalized = worker.join().unwrap();
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(status.observation().terminal, Terminal::Panicked);
    pump(&mut flow);
    assert_eq!(flow.work_counts(), WorkCounts::default());
    println!("discarded-success output_panic={panic} normalized={normalized}; before={before:?}; after={:?}", status.observation());
    assert!(normalized, "ProducerExit::track discarded an owned successful output outside the catch boundary; no panic was already unwinding");
}
#[test]
fn tracked_discarded_output_single_unwind_is_normalized() {
    discarded_success(true);
}
#[test]
fn tracked_discarded_nonpanicking_output_control() {
    discarded_success(false);
}

struct Native {
    state: Arc<Mutex<(Option<WorkStatus>, Vec<Observation>)>>,
    next: Arc<AtomicUsize>,
    serial: usize,
}
impl Clone for Native {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            next: self.next.clone(),
            serial: self.next.fetch_add(1, Ordering::SeqCst),
        }
    }
}
impl PartialEq for Native {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for Native {}
impl Drop for Native {
    fn drop(&mut self) {
        // clone 1 is the lane identity; clone 2 is the timer completion identity.
        if self.serial == 2 {
            let mut state = self.state.lock().unwrap();
            let observation = state.0.as_ref().unwrap().observation();
            state.1.push(observation);
            drop(state);
            panic!("single event native identity destructor panic");
        }
    }
}
#[test]
fn timer_native_identity_destruction_also_stays_under_owned_catch() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let state = Arc::new(Mutex::new((None, Vec::new())));
    let native = Native {
        state: state.clone(),
        next: Arc::new(AtomicUsize::new(1)),
        serial: 0,
    };
    let (_request, status) = flow
        .replace_timer(
            sim.world(),
            owner.id(),
            LaneKey(1),
            Identity {
                target: native,
                operation: 1,
                revision: 0,
            },
            Duration::ZERO,
            17u64,
        )
        .ok()
        .unwrap();
    state.lock().unwrap().0 = Some(status.clone());
    sim.update();
    assert!(!status.observation().exit_acknowledged);
    let result = catch_unwind(AssertUnwindSafe(|| sim.update()));
    assert!(result.is_ok());
    let during = state.lock().unwrap().1[0];
    assert!(!during.exit_acknowledged);
    assert_eq!(during.pending_deliveries, 1);
    assert_eq!(status.observation().terminal, Terminal::Panicked);
    assert!(status.observation().exit_acknowledged);
    drop(owner);
    pump(&mut flow);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
