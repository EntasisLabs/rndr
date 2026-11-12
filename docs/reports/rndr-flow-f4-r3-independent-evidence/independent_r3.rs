//! Independent R3 probe: pause both destructors and observe the real public boundary.
use rndr_core::{events, Executor, Job, Sim};
use rndr_flow::*;
use std::future::Future;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

#[derive(Default)]
struct Held {
    job: Mutex<Option<Job>>,
    startup_panic: bool,
}
impl Executor for Held {
    fn spawn(&self, job: Job) {
        *self.job.lock().unwrap() = Some(job);
        if self.startup_panic {
            panic!("startup returned/unwound while actual Job is retained");
        }
    }
}
#[derive(Default)]
struct State {
    status: Mutex<Option<WorkStatus>>,
    future_drops: AtomicUsize,
    output_drops: AtomicUsize,
}
#[derive(Clone)]
struct Output {
    state: Arc<State>,
    entered: mpsc::SyncSender<Observation>,
    release: Arc<Mutex<mpsc::Receiver<()>>>,
    panic: bool,
}
impl Drop for Output {
    fn drop(&mut self) {
        assert!(!std::thread::panicking());
        assert_eq!(self.state.future_drops.load(Ordering::SeqCst), 1);
        assert_eq!(self.state.output_drops.fetch_add(1, Ordering::SeqCst), 0);
        let during = self
            .state
            .status
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .observation();
        self.entered.send(during).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        if self.panic {
            panic!("output panic only after earlier future unwind was caught");
        }
    }
}
struct User {
    state: Arc<State>,
    output: Option<Output>,
    entered: mpsc::SyncSender<Observation>,
    release: mpsc::Receiver<()>,
    panic: bool,
}
impl Future for User {
    type Output = Result<Output, TaskFailure>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(Ok(self.output.take().unwrap()))
    }
}
impl Drop for User {
    fn drop(&mut self) {
        assert!(!std::thread::panicking());
        assert_eq!(self.state.future_drops.fetch_add(1, Ordering::SeqCst), 0);
        let during = self
            .state
            .status
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .observation();
        self.entered.send(during).unwrap();
        self.release.recv_timeout(Duration::from_secs(10)).unwrap();
        if self.panic {
            panic!("future destructor panic, separately caught before output destruction");
        }
    }
}
fn identity(op: u64) -> Identity<u64> {
    Identity {
        target: 9,
        operation: op,
        revision: 0,
    }
}
fn pump(flow: &mut Coordinator) {
    for _ in 0..600 {
        flow.pump_work(1);
    }
}
#[derive(Default)]
struct Writer(usize);
impl CompletionTarget<u64, Result<Output, TaskFailure>> for Writer {
    type Output = ();
    type Error = ();
    fn commit(&mut self, _: &Identity<u64>, _: Result<Output, TaskFailure>) -> Result<(), ()> {
        self.0 += 1;
        Ok(())
    }
}
#[test]
fn paused_future_then_suppressed_output_preserves_reserved_queued_published_cloned_charge() {
    // Normal custom startup requires a future destructor panic to suppress Ok;
    // panicking startup supplies aggregate panic independently. All three have
    // both normal and panicking output controls. No private status mutation.
    for (startup_panic, future_panic) in [(false, true), (true, false), (true, true)] {
        for output_panic in [false, true] {
            let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
            flow.configure_work(WorkLimits {
                slots: 1,
                ..WorkLimits::default()
            })
            .unwrap();
            let mut sim = Sim::new();
            let world = flow.register_world().unwrap();
            let owner = flow.create_scope(world.scope()).unwrap();
            let survivor = flow.create_scope(world.scope()).unwrap();
            world.bind(sim.world()).unwrap();
            let held = Arc::new(Held {
                startup_panic,
                ..Held::default()
            });
            sim.app.set_executor(held.clone());
            let state = Arc::new(State::default());
            let (f_tx, f_rx) = mpsc::sync_channel(0);
            let (f_go, f_wait) = mpsc::sync_channel(0);
            let (o_tx, o_rx) = mpsc::sync_channel(0);
            let (o_go, o_wait) = mpsc::sync_channel(0);
            let user = User {
                state: state.clone(),
                entered: f_tx,
                release: f_wait,
                panic: future_panic,
                output: Some(Output {
                    state: state.clone(),
                    entered: o_tx,
                    release: Arc::new(Mutex::new(o_wait)),
                    panic: output_panic,
                }),
            };
            let (request, status) = flow
                .replace_background(sim.world(), owner.id(), LaneKey(1), identity(1), user)
                .ok()
                .unwrap();
            *state.status.lock().unwrap() = Some(status.clone());
            pump(&mut flow);
            assert!(!status.observation().exit_acknowledged);
            assert_eq!(flow.work_counts().charged, 1);
            if startup_panic {
                assert_eq!(status.observation().terminal, Terminal::Panicked);
                assert!(!status.observation().cancellation_signalled);
                assert!(!flow.request_is_current(request.token()));
            }
            let mut job = held.job.lock().unwrap().take().unwrap();
            let worker = std::thread::spawn(move || {
                catch_unwind(AssertUnwindSafe(|| {
                    job.poll(&mut Context::from_waker(Waker::noop()))
                }))
            });
            let during_future = f_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert!(!during_future.exit_acknowledged);
            assert_eq!(during_future.pending_deliveries, 1);
            drop(owner); // observer cancellation is not an actual-future join
            pump(&mut flow);
            assert!(!status.observation().exit_acknowledged);
            assert_eq!(flow.work_counts().charged, 1);
            f_go.send(()).unwrap();
            let during_output = o_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            // Precisely USER-future exit, not wrapper/output/backend join.
            assert!(during_output.exit_acknowledged);
            assert_eq!(during_output.terminal, Terminal::Panicked);
            assert_eq!(during_output.pending_deliveries, 1);
            pump(&mut flow);
            assert_eq!(flow.work_counts().charged, 1);
            let refusal = flow
                .replace_background(sim.world(), survivor.id(), LaneKey(2), identity(2), async {
                    Ok::<_, TaskFailure>(7u64)
                })
                .err()
                .unwrap();
            assert_eq!(refusal.reason, Error::Capacity);
            drop(refusal.value);
            o_go.send(()).unwrap();
            assert!(worker.join().unwrap().unwrap().is_ready());
            assert_eq!(state.future_drops.load(Ordering::SeqCst), 1);
            assert_eq!(state.output_drops.load(Ordering::SeqCst), 1);
            assert_eq!(status.observation().terminal, Terminal::Panicked);
            assert_eq!(
                status.observation().pending_deliveries,
                usize::from(startup_panic)
            ); // cancellation suppresses inbox output
            pump(&mut flow);
            assert_eq!(flow.work_counts().charged, usize::from(startup_panic));
            sim.update();
            let published = events::<Completion<u64, Result<Output, TaskFailure>>>(sim.world());
            assert_eq!(published.len(), usize::from(startup_panic));
            // A returned cancellation handle suppresses publication after cancel;
            // a startup-panic retained Job has no such handle and publishes Err.
            if startup_panic {
                let first = published[0].clone();
                let second = first.clone();
                assert_eq!(first.clone().transpose().err(), Some(TaskFailure::Panicked));
                let mut writer = Writer::default();
                assert!(flow
                    .commit_completion(sim.world(), first, &mut writer)
                    .is_err());
                assert_eq!(writer.0, 0);
                sim.update(); // release published buffers, not caller-owned clone
                pump(&mut flow);
                assert_eq!(flow.work_counts().charged, 1);
                assert_eq!(status.observation().pending_deliveries, 1);
                drop(second);
            }
            sim.update();
            pump(&mut flow);
            assert_eq!(flow.work_counts(), WorkCounts::default());
            println!("independent-paused startup_panic={startup_panic} future_panic={future_panic} output_panic={output_panic} future_ack_false=true output_user_ack_true=true reserved_until_final_clone=true no_write=true exactly_once=true");
        }
    }
}
