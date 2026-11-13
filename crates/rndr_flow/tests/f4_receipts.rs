use rndr_core::{events, Executor, Job, Sim};
use rndr_flow::*;
use std::{
    sync::{Arc, Mutex},
    task::{Context, Waker},
};
#[derive(Default)]
struct Holding(Mutex<Vec<Job>>);
impl Executor for Holding {
    fn spawn(&self, job: Job) {
        self.0.lock().unwrap().push(job);
    }
}
fn run(executor: &Holding) {
    let jobs = std::mem::take(&mut *executor.0.lock().unwrap());
    let mut cx = Context::from_waker(Waker::noop());
    for mut job in jobs {
        assert!(job.poll(&mut cx).is_ready());
    }
}
fn id(op: u64) -> Identity<u64> {
    Identity {
        target: 9,
        operation: op,
        revision: 0,
    }
}
#[derive(Default)]
struct Writer(usize);
impl CompletionTarget<u64, Result<u64, TaskFailure>> for Writer {
    type Output = ();
    type Error = ();
    fn commit(&mut self, _: &Identity<u64>, value: Result<u64, TaskFailure>) -> Result<(), ()> {
        value.map_err(|_| ())?;
        self.0 += 1;
        Ok(())
    }
}
#[test]
fn acknowledged_future_cannot_evade_capacity_through_queued_or_published_payloads() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    flow.configure_work(WorkLimits {
        slots: 2,
        ..WorkLimits::default()
    })
    .unwrap();
    let executor = Arc::new(Holding::default());
    sim.app.set_executor(executor.clone());
    let (_, old) = flow
        .replace_background(sim.world(), owner.id(), LaneKey(1), id(1), async {
            Ok::<_, TaskFailure>(1u64)
        })
        .ok()
        .unwrap();
    run(&executor);
    assert!(old.observation().exit_acknowledged);
    assert_eq!(old.observation().pending_deliveries, 1);
    let (new, status) = flow
        .replace_background(sim.world(), owner.id(), LaneKey(1), id(2), async {
            Ok::<_, TaskFailure>(2u64)
        })
        .ok()
        .unwrap();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    run(&executor);
    assert_eq!(flow.work_counts().charged, 2);
    assert_eq!(flow.work_counts().pending_deliveries, 2);
    let refused = flow
        .replace_background(sim.world(), owner.id(), LaneKey(1), id(3), async {
            Ok::<_, TaskFailure>(3u64)
        })
        .err()
        .unwrap();
    assert_eq!(refused.reason, Error::Capacity);
    assert!(flow.request_is_current(new.token()));
    assert!(!status.observation().cancellation_signalled);
    let mut writer = Writer::default();
    assert_eq!(writer.0, 0);
    sim.update();
    let ready = events::<Completion<u64, Result<u64, TaskFailure>>>(sim.world()).to_vec();
    assert_eq!(ready.len(), 1);
    flow.commit_completion(sim.world(), ready[0].clone(), &mut writer)
        .unwrap();
    assert_eq!(writer.0, 1);
    assert!(flow
        .commit_completion(sim.world(), ready[0].clone(), &mut writer)
        .is_err());
    assert_eq!(writer.0, 1);
    drop(ready);
    flow.cancel_request(new.token()).unwrap();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts().charged, 1);
    assert_eq!(flow.work_counts().pending_deliveries, 1);
    sim.run_until_parked();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(
        flow.work_counts().charged,
        1,
        "quiet runtime cannot release published receipt"
    );
    sim.update();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts(), WorkCounts::default());
    assert_eq!(status.observation().pending_deliveries, 0);
}
#[test]
fn application_scope_before_target_world_is_revisited_after_world_retirement() {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let application = flow.create_scope(root.id()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let world_scope = world.scope();
    world.bind(sim.world()).unwrap();
    let (request, status) = flow
        .replace_local(
            sim.world(),
            application.id(),
            LaneKey(1),
            id(1),
            DeliveryMode::Once,
            std::future::pending::<Result<u64, TaskFailure>>(),
        )
        .ok()
        .unwrap();
    flow.dispose(world_scope).unwrap();
    assert!(!flow.request_is_current(request.token()));
    for _ in 0..200 {
        let work = flow.pump_work(1);
        assert!(work.units <= 1);
    }
    assert_eq!(flow.state(application.id()), State::Visible);
    assert!(status.observation().cancellation_signalled);
    assert!(!status.observation().exit_acknowledged);
    sim.update();
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
