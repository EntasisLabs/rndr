use rndr_core::{events, Executor, Job, Sim};
use rndr_flow::*;
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};

#[derive(Default)]
struct RetainThenPanic(Mutex<Option<Job>>);
impl Executor for RetainThenPanic {
    fn spawn(&self, job: Job) {
        *self.0.lock().unwrap() = Some(job);
        panic!("controlled executor panic after retaining Job");
    }
}
#[derive(Default)]
struct Writer(usize);
impl CompletionTarget<u64, Result<u64, TaskFailure>> for Writer {
    type Output = ();
    type Error = ();
    fn commit(&mut self, _: &Identity<u64>, _: Result<u64, TaskFailure>) -> Result<(), ()> {
        self.0 += 1;
        Ok(())
    }
}
#[test]
fn retained_job_after_executor_start_panic_stays_charged_without_fabricated_cancel_or_success() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let executor = Arc::new(RetainThenPanic::default());
    sim.app.set_executor(executor.clone());
    let (request, status) = flow
        .replace_background(
            sim.world(),
            owner.id(),
            LaneKey(1),
            Identity {
                target: 17u64,
                operation: 1,
                revision: 0,
            },
            async { Ok::<_, TaskFailure>(44u64) },
        )
        .ok()
        .unwrap();
    assert!(!flow.request_is_current(request.token()));
    let observation = status.observation();
    assert!(observation.retired);
    assert!(!observation.cancellation_signalled);
    assert!(!observation.exit_acknowledged);
    assert_eq!(observation.terminal, Terminal::Panicked);
    assert_eq!(observation.pending_deliveries, 1);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts().charged, 1);
    let mut job = executor.0.lock().unwrap().take().unwrap();
    assert!(job.poll(&mut Context::from_waker(Waker::noop())).is_ready());
    drop(job);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(
        status.observation().terminal,
        Terminal::Panicked,
        "later user success must not erase an admitted startup panic"
    );
    assert!(!status.observation().cancellation_signalled);
    sim.update();
    let ready = events::<Completion<u64, Result<u64, TaskFailure>>>(sim.world()).to_vec();
    assert_eq!(ready.len(), 1);
    let mut writer = Writer::default();
    for event in ready {
        assert_eq!(
            flow.commit_completion(sim.world(), event, &mut writer),
            Err(CompletionError::Flow(Error::Stale))
        );
    }
    assert_eq!(writer.0, 0);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts().charged, 1); // still-published receipt
    sim.update();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
