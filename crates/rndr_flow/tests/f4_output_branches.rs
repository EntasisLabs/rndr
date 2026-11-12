//! R3 shared wrapper branches; no private observation mutation or new executor.
use rndr_core::{events, Executor, FeedConfig, Job, Sim};
use rndr_flow::*;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::task::{Context, Poll, Waker};

#[derive(Default)]
struct Drops {
    future: AtomicUsize,
    output: AtomicUsize,
    status: Mutex<Option<WorkStatus>>,
    inside_future: Mutex<Vec<Observation>>,
}
struct Output {
    drops: Arc<Drops>,
    panic: bool,
}
impl Drop for Output {
    fn drop(&mut self) {
        assert!(!std::thread::panicking(), "sequential unwinds only");
        assert_eq!(self.drops.output.fetch_add(1, Ordering::SeqCst), 0);
        assert_eq!(self.drops.future.load(Ordering::SeqCst), 1);
        if self.panic {
            panic!("single output destructor panic after future destruction returned");
        }
    }
}
// mode: Ok, explicit Failed, explicit Panicked, actual poll panic, Pending.
struct UserFuture {
    mode: u8,
    future_panic: bool,
    output_panic: bool,
    drops: Arc<Drops>,
}
impl Future for UserFuture {
    type Output = Result<Output, TaskFailure>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        match self.mode {
            0 => Poll::Ready(Ok(Output {
                drops: self.drops.clone(),
                panic: self.output_panic,
            })),
            1 => Poll::Ready(Err(TaskFailure::Failed)),
            2 => Poll::Ready(Err(TaskFailure::Panicked)),
            3 => panic!("single poll panic before future destructor runs"),
            4 => Poll::Pending,
            _ => unreachable!(),
        }
    }
}
impl Drop for UserFuture {
    fn drop(&mut self) {
        assert!(
            !std::thread::panicking(),
            "poll unwind must already be caught"
        );
        assert_eq!(self.drops.future.fetch_add(1, Ordering::SeqCst), 0);
        let observation = self
            .drops
            .status
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .observation();
        assert!(!observation.exit_acknowledged);
        self.drops.inside_future.lock().unwrap().push(observation);
        if self.future_panic {
            panic!("single future destructor panic before output destructor runs");
        }
    }
}
fn id() -> Identity<u64> {
    Identity {
        target: 1,
        operation: 1,
        revision: 0,
    }
}
fn pump(flow: &mut Coordinator) {
    for _ in 0..400 {
        flow.pump_work(1);
    }
}
fn check_drops(drops: &Drops, mode: u8, receipts: usize) {
    assert_eq!(drops.future.load(Ordering::SeqCst), 1);
    assert_eq!(drops.output.load(Ordering::SeqCst), usize::from(mode == 0));
    let during = drops.inside_future.lock().unwrap();
    assert_eq!(during.len(), 1);
    assert_eq!(during[0].pending_deliveries, receipts);
    assert!(!during[0].exit_acknowledged);
}
#[test]
fn tracked_nonunit_ready_error_poll_panic_and_pending_drop_branches() {
    for mode in 0..5 {
        for future_panic in [false, true] {
            for output_panic in [false, true] {
                // A successful returned output is caller-owned, not discarded
                // by the library; test that normal branch without a caller panic.
                if output_panic && (mode != 0 || !future_panic) {
                    continue;
                }
                let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
                let mut sim = Sim::new();
                let world = flow.register_world().unwrap();
                let owner = flow.create_scope(world.scope()).unwrap();
                world.bind(sim.world()).unwrap();
                let (_, sender, exit, status) = flow
                    .replace_feed::<_, u64>(
                        sim.world(),
                        owner.id(),
                        LaneKey(1),
                        id(),
                        DeliveryMode::Once,
                        FeedConfig::queue(1),
                    )
                    .unwrap();
                let drops = Arc::new(Drops::default());
                *drops.status.lock().unwrap() = Some(status.clone());
                let mut tracked = Box::pin(exit.track(UserFuture {
                    mode,
                    future_panic,
                    output_panic,
                    drops: drops.clone(),
                }));
                let result = catch_unwind(AssertUnwindSafe(|| {
                    tracked
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                }))
                .expect("owned normalization must return, not unwind");
                if mode == 4 {
                    assert!(result.is_pending());
                    assert!(!status.observation().exit_acknowledged);
                    drop(owner);
                    pump(&mut flow);
                    assert_eq!(flow.work_counts().charged, 1);
                    assert!(!status.observation().exit_acknowledged);
                } else {
                    let expected = if future_panic || mode >= 2 {
                        Some(TaskFailure::Panicked)
                    } else if mode == 1 {
                        Some(TaskFailure::Failed)
                    } else {
                        None
                    };
                    match (result, expected) {
                        (Poll::Ready(Err(actual)), Some(expected)) => assert_eq!(actual, expected),
                        (Poll::Ready(Ok(value)), None) => {
                            assert_eq!(drops.output.load(Ordering::SeqCst), 0);
                            drop(value); // returned payload remains caller-owned
                        }
                        _ => panic!("wrong normalized non-unit result"),
                    }
                    drop(owner);
                }
                assert!(catch_unwind(AssertUnwindSafe(|| drop(tracked))).is_ok());
                assert!(status.observation().exit_acknowledged);
                let expected = if future_panic || (2..=3).contains(&mode) {
                    Terminal::Panicked
                } else {
                    match mode {
                        0 => Terminal::Succeeded,
                        1 => Terminal::Failed,
                        _ => Terminal::DroppedUnknown,
                    }
                };
                assert_eq!(status.observation().terminal, expected);
                check_drops(&drops, mode, 0);
                drop(sender);
                pump(&mut flow);
                assert_eq!(flow.work_counts(), WorkCounts::default());
                println!("R3-track mode={mode} future_panic={future_panic} output_panic={output_panic} exactly_once=true actual_exit=true reclaimed=true");
            }
        }
    }
}
#[derive(Default)]
struct Held {
    job: Mutex<Option<Job>>,
    panic: bool,
}
impl Executor for Held {
    fn spawn(&self, job: Job) {
        *self.job.lock().unwrap() = Some(job);
        if self.panic {
            panic!("caught startup failure with actual Job retained");
        }
    }
}
#[test]
fn local_background_and_custom_tasks_keep_nonunit_drop_panics_and_receipts_owned() {
    // local, default background, held custom, held custom with startup panic.
    for path in 0..4 {
        for mode in 0..5 {
            for future_panic in [false, true] {
                for output_panic in [false, true] {
                    let suppressed = future_panic || path == 3;
                    if output_panic && (mode != 0 || !suppressed) {
                        continue;
                    }
                    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
                    let mut sim = Sim::new();
                    let world = flow.register_world().unwrap();
                    let owner = flow.create_scope(world.scope()).unwrap();
                    world.bind(sim.world()).unwrap();
                    let held = Arc::new(Held {
                        panic: path == 3,
                        ..Held::default()
                    });
                    if path >= 2 {
                        sim.app.set_executor(held.clone());
                    }
                    let drops = Arc::new(Drops::default());
                    let future = UserFuture {
                        mode,
                        future_panic,
                        output_panic,
                        drops: drops.clone(),
                    };
                    let (_, status) = if path == 0 {
                        flow.replace_local(
                            sim.world(),
                            owner.id(),
                            LaneKey(1),
                            id(),
                            DeliveryMode::Once,
                            future,
                        )
                    } else {
                        flow.replace_background(sim.world(), owner.id(), LaneKey(1), id(), future)
                    }
                    .ok()
                    .unwrap();
                    *drops.status.lock().unwrap() = Some(status.clone());
                    assert!(!status.observation().exit_acknowledged);
                    assert_eq!(status.observation().pending_deliveries, 1);
                    pump(&mut flow);
                    assert_eq!(flow.work_counts().charged, 1);
                    assert!(!status.observation().exit_acknowledged);
                    let mut retained = if path >= 2 {
                        let mut job = held.job.lock().unwrap().take().unwrap();
                        let polled = catch_unwind(AssertUnwindSafe(|| {
                            job.poll(&mut Context::from_waker(Waker::noop()))
                        }))
                        .unwrap();
                        assert_eq!(polled.is_pending(), mode == 4);
                        Some(job)
                    } else {
                        None
                    };
                    assert!(catch_unwind(AssertUnwindSafe(|| sim.update())).is_ok());
                    if mode == 4 {
                        assert!(!status.observation().exit_acknowledged);
                        assert_eq!(status.observation().pending_deliveries, 1);
                        drop(owner);
                        pump(&mut flow);
                        assert!(!status.observation().exit_acknowledged);
                        assert_eq!(flow.work_counts().charged, 1);
                        assert!(catch_unwind(AssertUnwindSafe(|| drop(retained.take()))).is_ok());
                        assert!(catch_unwind(AssertUnwindSafe(|| sim.update())).is_ok());
                    } else {
                        drop(retained);
                        // An escaping Tracked panic can be caught by core but then no
                        // completion is published. Require the actual failure envelope.
                        assert_eq!(
                            events::<Completion<u64, Result<Output, TaskFailure>>>(sim.world())
                                .len(),
                            1
                        );
                        assert!(status.observation().exit_acknowledged);
                        assert_eq!(
                            status.observation().terminal,
                            if suppressed || mode >= 2 {
                                Terminal::Panicked
                            } else if mode == 1 {
                                Terminal::Failed
                            } else {
                                Terminal::Succeeded
                            }
                        );
                        assert_eq!(
                            drops.output.load(Ordering::SeqCst),
                            usize::from(mode == 0 && suppressed)
                        );
                        assert_eq!(status.observation().pending_deliveries, 1);
                        drop(owner);
                        pump(&mut flow);
                        assert_eq!(flow.work_counts().charged, 1); // real published receipt
                        assert!(catch_unwind(AssertUnwindSafe(|| sim.update())).is_ok());
                    }
                    check_drops(&drops, mode, 1);
                    assert!(status.observation().exit_acknowledged);
                    assert_eq!(
                        status.observation().terminal,
                        if suppressed || (2..=3).contains(&mode) {
                            Terminal::Panicked
                        } else {
                            match mode {
                                0 => Terminal::Succeeded,
                                1 => Terminal::Failed,
                                _ => Terminal::DroppedUnknown,
                            }
                        }
                    );
                    assert_eq!(status.observation().pending_deliveries, 0);
                    pump(&mut flow);
                    assert_eq!(flow.work_counts(), WorkCounts::default());
                    println!("R3-task path={path} mode={mode} future_panic={future_panic} output_panic={output_panic} exactly_once=true receipt_preserved=true actual_exit=true reclaimed=true");
                }
            }
        }
    }
}
