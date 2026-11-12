use rndr_core::{events, Sim};
use rndr_flow::*;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Native {
    domain: u64,
    session: u64,
    turn: u64,
}
fn identity(domain: u64, op: u64) -> Identity<Native> {
    Identity {
        target: Native {
            domain,
            session: 10,
            turn: 20,
        },
        operation: op,
        revision: 0,
    }
}
#[derive(Default)]
struct Model {
    writes: Vec<(u64, u64)>,
    revision: u64,
    history: usize,
}
impl CompletionTarget<Native, u64> for Model {
    type Output = ();
    type Error = ();
    fn commit(&mut self, id: &Identity<Native>, value: u64) -> Result<(), ()> {
        if id.revision != self.revision {
            return Err(());
        }
        self.writes.push((id.target.domain, value));
        self.revision += 1;
        self.history += 1;
        Ok(())
    }
}
fn fixture(seed: u64) -> (Coordinator, ScopeOwner, ScopeOwner, Sim) {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::with_seed(seed);
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    (flow, root, owner, sim)
}
fn pump(flow: &mut Coordinator, n: usize) {
    for _ in 0..n {
        let w = flow.pump_work(1);
        assert!(w.units <= 1);
        assert_eq!(
            w.units,
            w.metadata_visits + w.lease_visits + w.ack_visits + w.restarts + w.commands
        );
    }
}
fn refuse(flow: &mut Coordinator, sim: &mut Sim, event: Completion<Native, u64>) {
    let mut model = Model::default();
    assert!(flow
        .commit_completion(sim.world(), event, &mut model)
        .is_err());
    assert!(model.writes.is_empty());
    assert_eq!((model.history, model.revision), (0, 0));
}
type SharedValue<T> = Arc<Mutex<(Option<T>, Option<Waker>)>>;
struct Controlled<T> {
    state: SharedValue<T>,
    drop_count: Arc<CellCounter>,
}
#[derive(Default)]
struct CellCounter(std::sync::atomic::AtomicUsize);
impl Drop for CellCounter {
    fn drop(&mut self) {}
}
impl<T> Future for Controlled<T> {
    type Output = Result<T, TaskFailure>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.state.lock().unwrap();
        match state.0.take() {
            Some(value) => Poll::Ready(Ok(value)),
            None => {
                state.1 = Some(cx.waker().clone());
                Poll::Pending
            }
        }
    }
}
impl<T> Drop for Controlled<T> {
    fn drop(&mut self) {
        self.drop_count
            .0
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}
fn controlled<T>() -> (Controlled<T>, SharedValue<T>, Arc<CellCounter>) {
    let state = Arc::new(Mutex::new((None, None)));
    let drops = Arc::new(CellCounter::default());
    (
        Controlled {
            state: state.clone(),
            drop_count: drops.clone(),
        },
        state,
        drops,
    )
}
fn release<T>(state: &Mutex<(Option<T>, Option<Waker>)>, value: T) {
    let wake = {
        let mut state = state.lock().unwrap();
        state.0 = Some(value);
        state.1.take()
    };
    if let Some(w) = wake {
        w.wake();
    }
}

#[test]
fn same_owner_replacement_independent_lanes_and_duplicate_zero_writes() {
    for seed in [1, 7, 42] {
        let (mut flow, _root, owner, mut sim) = fixture(seed);
        let old = flow
            .replace_request(
                sim.world(),
                owner.id(),
                LaneKey(1),
                identity(1, 1),
                DeliveryMode::Once,
            )
            .unwrap();
        let independent = flow
            .replace_request(
                sim.world(),
                owner.id(),
                LaneKey(2),
                identity(2, 2),
                DeliveryMode::Once,
            )
            .unwrap();
        let new = flow
            .replace_request(
                sim.world(),
                owner.id(),
                LaneKey(1),
                identity(1, 3),
                DeliveryMode::Once,
            )
            .unwrap();
        refuse(&mut flow, &mut sim, old.completion(None, 99));
        let mut first = Model::default();
        flow.commit_completion(sim.world(), new.completion(None, 3), &mut first)
            .unwrap();
        assert_eq!(first.writes, [(1, 3)]);
        refuse(&mut flow, &mut sim, new.completion(None, 3));
        let mut second = Model::default();
        flow.commit_completion(sim.world(), independent.completion(None, 2), &mut second)
            .unwrap();
        assert_eq!(second.writes, [(2, 2)]);
    }
}
#[test]
fn ordered_native_session_turn_and_revision_identity_never_substitutes_target() {
    let (mut flow, _root, owner, mut sim) = fixture(42);
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(17, 1),
            DeliveryMode::Ordered { recoverable: true },
        )
        .unwrap();
    // Native writer validates captured immutable revision. This fixture's
    // stream models chunk sequences without changing the native document rev.
    struct Stream(Vec<(Native, u64)>);
    impl CompletionTarget<Native, u64> for Stream {
        type Output = ();
        type Error = ();
        fn commit(&mut self, id: &Identity<Native>, value: u64) -> Result<(), ()> {
            self.0.push((id.target.clone(), value));
            Ok(())
        }
    }
    let mut model = Stream(Vec::new());
    flow.commit_completion(sim.world(), request.completion(Some(4), 4), &mut model)
        .unwrap();
    for seq in [Some(4), Some(3), None] {
        assert!(flow
            .commit_completion(sim.world(), request.completion(seq, 99), &mut model)
            .is_err());
        assert_eq!(model.0.len(), 1);
    }
    flow.commit_completion(sim.world(), request.completion(Some(9), 9), &mut model)
        .unwrap();
    assert!(model
        .0
        .iter()
        .all(|(target, _)| *target == identity(17, 1).target));
    let mut next = identity(17, 2);
    next.target.session = 11;
    next.target.turn = 21;
    let _new = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            next,
            DeliveryMode::Ordered { recoverable: false },
        )
        .unwrap();
    let before = model.0.clone();
    assert!(flow
        .commit_completion(sim.world(), request.completion(Some(10), 10), &mut model)
        .is_err());
    assert_eq!(model.0, before);
    assert_eq!(
        flow.suspend(owner.id(), Recovery::ReconcileFromDurableState),
        Err(Error::Invalid)
    );
    assert!(flow.request_is_current(_new.token()));
}
#[test]
fn hidden_and_recoverable_suspend_restart_activity_not_stream_buffering() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Ordered { recoverable: true },
        )
        .unwrap();
    flow.set_visibility(owner.id(), State::Hidden).unwrap();
    assert!(flow.request_is_current(request.token()));
    flow.set_visibility(owner.id(), State::Visible).unwrap();
    assert!(flow.request_is_current(request.token()));
    flow.suspend(owner.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    refuse(&mut flow, &mut sim, request.completion(Some(1), 1));
    flow.resume(owner.id(), true).unwrap();
    refuse(&mut flow, &mut sim, request.completion(Some(2), 2));
    let fresh = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 2),
            DeliveryMode::Once,
        )
        .unwrap();
    let mut model = Model::default();
    flow.commit_completion(sim.world(), fresh.completion(None, 2), &mut model)
        .unwrap();
}
#[test]
fn lane_and_slot_capacity_refusal_returns_untouched_future_and_old_request() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    flow.configure_work(WorkLimits {
        lanes_per_scope: 1,
        slots: 1,
        effects_per_world: 1,
    })
    .unwrap();
    let (future, _state, drops) = controlled::<u64>();
    let (old, status) = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Once,
            future,
        )
        .ok()
        .unwrap();
    let (future, _, rejected_drops) = controlled::<u64>();
    let refused = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 2),
            DeliveryMode::Once,
            future,
        )
        .err()
        .unwrap();
    assert_eq!(refused.reason, Error::Capacity);
    assert_eq!(
        rejected_drops.0.load(std::sync::atomic::Ordering::SeqCst),
        0
    );
    assert!(flow.request_is_current(old.token()));
    assert_eq!(flow.work_counts().charged, 1);
    assert!(!status.observation().cancellation_signalled);
    assert_eq!(
        flow.replace_request(
            sim.world(),
            owner.id(),
            LaneKey(2),
            identity(2, 2),
            DeliveryMode::Once
        )
        .err(),
        Some(Error::Capacity)
    );
    assert!(flow.request_is_current(old.token()));
    drop(refused);
    assert_eq!(
        rejected_drops.0.load(std::sync::atomic::Ordering::SeqCst),
        1
    );
    drop(owner);
    pump(&mut flow, 100);
    sim.update();
    pump(&mut flow, 100);
    assert_eq!(drops.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn local_rc_panic_failure_cancel_before_poll_and_future_drop_are_distinct() {
    for seed in [1, 7, 42] {
        let (mut flow, _root, owner, mut sim) = fixture(seed);
        let rc = Rc::new(Cell::new(0));
        let capture = rc.clone();
        let (_, success) = flow
            .replace_local(
                sim.world(),
                owner.id(),
                LaneKey(1),
                identity(1, 1),
                DeliveryMode::Once,
                async move {
                    capture.set(1);
                    Ok::<_, TaskFailure>(1u64)
                },
            )
            .ok()
            .unwrap();
        let (_, panic) = flow
            .replace_local(
                sim.world(),
                owner.id(),
                LaneKey(2),
                identity(2, 2),
                DeliveryMode::Once,
                async {
                    panic!("local poll panic");
                    #[allow(unreachable_code)]
                    Ok::<u64, TaskFailure>(0)
                },
            )
            .ok()
            .unwrap();
        let (_, failure) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(3), identity(3, 3), async {
                Err::<u64, _>(TaskFailure::Failed)
            })
            .ok()
            .unwrap();
        let (_, background_panic) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(4), identity(4, 4), async {
                panic!("background poll panic");
                #[allow(unreachable_code)]
                Ok::<u64, TaskFailure>(0)
            })
            .ok()
            .unwrap();
        sim.update();
        assert_eq!(rc.get(), 1);
        for (status, terminal) in [
            (success, Terminal::Succeeded),
            (panic, Terminal::Panicked),
            (failure, Terminal::Failed),
            (background_panic, Terminal::Panicked),
        ] {
            let o = status.observation();
            assert_eq!(o.terminal, terminal);
            assert!(o.exit_acknowledged);
            assert!(!o.retired);
        }
        let (future, _, drops) = controlled::<u64>();
        let (_, status) = flow
            .replace_local(
                sim.world(),
                owner.id(),
                LaneKey(5),
                identity(5, 5),
                DeliveryMode::Once,
                future,
            )
            .ok()
            .unwrap();
        drop(owner);
        pump(&mut flow, 300);
        assert!(status.observation().cancellation_signalled);
        assert!(!status.observation().exit_acknowledged);
        sim.update();
        pump(&mut flow, 300);
        assert_eq!(status.observation().terminal, Terminal::DroppedUnknown);
        assert!(status.observation().exit_acknowledged);
        assert_eq!(drops.0.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(flow.work_counts(), WorkCounts::default());
    }
}
#[test]
fn already_published_event_rejected_even_in_input_after_drop() {
    for seed in [1, 7, 42] {
        let (mut flow, _root, owner, mut sim) = fixture(seed);
        let (_, status) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(1), identity(1, 1), async {
                Ok::<_, TaskFailure>(7u64)
            })
            .ok()
            .unwrap();
        // Input is the earliest consumer stage; published events remain visible
        // after dropping owner, regardless of cancellation/inbox suppression.
        let dead = Rc::new(RefCell::new(Some(owner)));
        let drop_at_input = dead.clone();
        let shared = Rc::new(RefCell::new(flow));
        let at_input = shared.clone();
        let seen = Rc::new(Cell::new(0));
        let observed = seen.clone();
        #[derive(Clone)]
        struct InputFixture {
            owner: Rc<RefCell<Option<ScopeOwner>>>,
            flow: Rc<RefCell<Coordinator>>,
            seen: Rc<Cell<usize>>,
        }
        sim.world().insert_local(InputFixture {
            owner: drop_at_input,
            flow: at_input,
            seen: observed,
        });
        sim.app.add_system(rndr_core::Schedule::Input, |world| {
            let InputFixture {
                owner: drop_at_input,
                flow: at_input,
                seen: observed,
            } = world.local::<InputFixture>().unwrap().clone();
            drop(drop_at_input.borrow_mut().take());
            let ready = events::<Completion<Native, Result<u64, TaskFailure>>>(world).to_vec();
            assert!(!ready.is_empty());
            // Map producer Result using a concrete writer of the Result type.
            struct Writer(Model);
            impl CompletionTarget<Native, Result<u64, TaskFailure>> for Writer {
                type Output = ();
                type Error = ();
                fn commit(
                    &mut self,
                    id: &Identity<Native>,
                    value: Result<u64, TaskFailure>,
                ) -> Result<(), ()> {
                    self.0.commit(id, value.map_err(|_| ())?)
                }
            }
            for event in ready {
                let mut writer = Writer(Model::default());
                assert!(at_input
                    .borrow_mut()
                    .commit_completion(world, event, &mut writer)
                    .is_err());
                assert_eq!(
                    (writer.0.writes.len(), writer.0.revision, writer.0.history),
                    (0, 0, 0)
                );
                observed.set(observed.get() + 1);
            }
        });
        sim.update();
        assert_eq!(seen.get(), 1);
        assert!(status.observation().exit_acknowledged);
        assert!(dead.borrow().is_none());
        assert!(shared.borrow().counts().pending_scopes > 0);
    }
}
#[test]
fn same_logical_key_reopen_foreign_coordinator_world_and_lag_reject() {
    let (mut flow, _root, owner, mut sim) = fixture(7);
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Once,
        )
        .unwrap();
    let mut other = Sim::with_seed(1);
    let world = flow.register_world().unwrap();
    let world_scope = world.scope();
    world.bind(other.world()).unwrap();
    refuse(&mut flow, &mut other, request.completion(None, 1));
    let (mut foreign, _foreign_root) = Coordinator::new(Limits::default()).unwrap();
    let mut model = Model::default();
    assert!(foreign
        .commit_completion(sim.world(), request.completion(None, 1), &mut model)
        .is_err());
    assert!(model.writes.is_empty());
    drop(owner); // no local drain yet
    refuse(&mut flow, &mut sim, request.completion(None, 1));
    let reopened = flow.create_scope(world_scope).unwrap();
    let fresh = flow
        .replace_request(
            other.world(),
            reopened.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Once,
        )
        .unwrap();
    refuse(&mut flow, &mut other, request.completion(None, 1));
    let mut model = Model::default();
    flow.commit_completion(other.world(), fresh.completion(None, 2), &mut model)
        .unwrap();
    assert_eq!(model.writes, [(1, 2)]);
}
#[test]
fn cooperative_and_blocked_feed_stay_charged_independently_of_eligibility() {
    let (mut flow, _root, owner, mut sim) = fixture(42);
    let (request, sender, exit, status) = flow
        .replace_feed::<_, u64>(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Ordered { recoverable: true },
            rndr_core::FeedConfig::default(),
        )
        .unwrap();
    sender.try_send(Some(1), 1).unwrap();
    sim.update();
    let queued = events::<Completion<Native, u64>>(sim.world()).to_vec();
    assert_eq!(queued.len(), 1);
    drop(owner);
    pump(&mut flow, 200);
    for event in queued {
        refuse(&mut flow, &mut sim, event);
    }
    assert!(!flow.request_is_current(request.token()));
    assert!(sender.is_cancelled());
    assert!(sender.try_send(Some(2), 2).is_err());
    assert!(status.observation().retired);
    assert!(!status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts().retired_unacknowledged, 1);
    assert_eq!(flow.work_counts().charged, 1);
    // Arbitrarily many quiet pumps do not imply blocked producer exit.
    pump(&mut flow, 1000);
    assert_eq!(flow.work_counts().charged, 1);
    exit.run(|| Ok(())).unwrap();
    pump(&mut flow, 100);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts().pending_deliveries, 1);
    // Producer exit does not release a still-published payload receipt.
    sim.update();
    pump(&mut flow, 100);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn captured_epoch_rejects_old_sender_before_mutation() {
    #[derive(Debug)]
    struct Tick;
    impl rndr_core::Event for Tick {}
    let mut sim = Sim::new();
    let (mut feed, sender) =
        rndr_core::feed::open::<Tick>(sim.world(), rndr_core::FeedConfig::default());
    let captured = feed.epoch();
    sender.try_send_at(captured, Tick).unwrap();
    let newer = advance_feed(&mut feed).unwrap();
    assert_ne!(captured, newer);
    assert_eq!(
        sender.try_send_at(captured, Tick),
        Err(rndr_core::FeedError::Stale)
    );
    sim.update();
    assert!(events::<Tick>(sim.world()).is_empty());
}
#[test]
fn timer_manual_deadline_replacement_and_published_disposal() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    let (old, _) = flow
        .replace_timer(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            Duration::from_millis(5),
            1u64,
        )
        .unwrap();
    sim.advance(Duration::from_millis(4));
    sim.update();
    assert!(events::<TimerCompletion<Native, u64>>(sim.world()).is_empty());
    let (new, status) = flow
        .replace_timer(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 2),
            Duration::from_millis(1),
            2u64,
        )
        .unwrap();
    sim.advance(Duration::from_millis(1));
    sim.update();
    let ready: Vec<_> = events::<TimerCompletion<Native, u64>>(sim.world())
        .iter()
        .map(|e| e.completion())
        .collect();
    assert_eq!(ready.len(), 2);
    refuse(&mut flow, &mut sim, old.completion(None, 1));
    let mut model = Model::default();
    flow.commit_completion(sim.world(), new.completion(None, 2), &mut model)
        .unwrap();
    assert_eq!(model.writes, [(1, 2)]);
    drop(owner);
    for event in ready {
        refuse(&mut flow, &mut sim, event);
    }
    pump(&mut flow, 300);
    sim.update();
    sim.update();
    pump(&mut flow, 300);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn owning_world_effect_runs_before_custom_cleanup_child_first() {
    let (mut flow, _root, parent, mut sim) = fixture(1);
    let child = flow.create_scope(parent.id()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for (scope, label) in [(child.id(), 1), (parent.id(), 3)] {
        let request = flow
            .replace_request(
                sim.world(),
                scope,
                LaneKey(1),
                identity(1, label),
                DeliveryMode::Once,
            )
            .unwrap();
        let effects = log.clone();
        flow.own_world_effect(sim.world(), &request, move |_| {
            effects.borrow_mut().push(label)
        })
        .unwrap();
        let cleanup = log.clone();
        flow.register_cleanup(scope, CleanupKind::Lifetime, move |_| {
            cleanup.borrow_mut().push(label + 1);
            Ok(())
        })
        .unwrap();
    }
    let mut other = Sim::new();
    flow.register_world().unwrap().bind(other.world()).unwrap();
    drop(parent);
    pump(&mut flow, 100);
    assert!(log.borrow().is_empty());
    drain_world_effects(other.world(), 100);
    assert!(log.borrow().is_empty());
    for _ in 0..200 {
        pump(&mut flow, 1);
        assert!(drain_world_effects(sim.world(), 1) <= 1);
    }
    assert_eq!(*log.borrow(), [1, 2, 3, 4]);
    assert_eq!(flow.work_counts(), WorkCounts::default());
    drop(child);
}
#[test]
fn one_hundred_twenty_cycles_return_every_registry_and_ack_to_baseline() {
    for seed in [1, 7, 42] {
        let (mut flow, root, _initial, mut sim) = fixture(seed);
        drop(_initial);
        pump(&mut flow, 100);
        let baseline = flow.counts();
        let mut acknowledged = 0;
        for cycle in 0..120 {
            let world = sim.world().local::<WorldOwner>().unwrap().scope();
            let owner = flow.create_scope(world).unwrap();
            let (_, task) = flow
                .replace_local(
                    sim.world(),
                    owner.id(),
                    LaneKey(1),
                    identity(1, cycle),
                    DeliveryMode::Once,
                    async { Ok::<_, TaskFailure>(1u64) },
                )
                .ok()
                .unwrap();
            let (_, _, exit, feed) = flow
                .replace_feed::<_, u64>(
                    sim.world(),
                    owner.id(),
                    LaneKey(2),
                    identity(2, cycle),
                    DeliveryMode::Ordered { recoverable: true },
                    rndr_core::FeedConfig::default(),
                )
                .unwrap();
            sim.update();
            drop(owner);
            pump(&mut flow, 120);
            exit.run(|| Ok(())).unwrap();
            sim.update();
            sim.update();
            pump(&mut flow, 120);
            for status in [task, feed] {
                assert!(status.observation().exit_acknowledged);
                acknowledged += 1;
            }
            assert_eq!(flow.counts(), baseline);
            assert_eq!(flow.work_counts(), WorkCounts::default());
            println!("F4 seed={seed} cycle={} charged=0 handles=0 retired_unack=0 scopes={} cleanups=0 queued=0 acknowledged={acknowledged}",cycle+1,baseline.scopes);
        }
        drop(root);
    }
}

#[derive(Default)]
struct Holding(Mutex<Vec<rndr_core::Job>>);
impl rndr_core::Executor for Holding {
    fn spawn(&self, job: rndr_core::Job) {
        self.0.lock().unwrap().push(job);
    }
}
fn poll_held(executor: &Holding) {
    let mut jobs = std::mem::take(&mut *executor.0.lock().unwrap());
    let mut cx = Context::from_waker(Waker::noop());
    jobs.retain_mut(|job| job.poll(&mut cx).is_pending());
    executor.0.lock().unwrap().extend(jobs);
}
#[derive(Default)]
struct ResultWriter(Model);
impl CompletionTarget<Native, Result<u64, TaskFailure>> for ResultWriter {
    type Output = ();
    type Error = ();
    fn commit(&mut self, id: &Identity<Native>, value: Result<u64, TaskFailure>) -> Result<(), ()> {
        self.0.commit(id, value.map_err(|_| ())?)
    }
}
#[test]
fn controlled_old_finishes_last_custom_executor_and_inbox_cancel_orderings() {
    for seed in [1, 7, 42] {
        let (mut flow, _root, owner, mut sim) = fixture(seed);
        let executor = Arc::new(Holding::default());
        sim.app.set_executor(executor.clone());
        let (future, state, _drops) = controlled::<u64>();
        let (old, old_status) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(1), identity(1, 1), future)
            .ok()
            .unwrap();
        poll_held(&executor);
        assert_eq!(old_status.observation().terminal, Terminal::Running);
        let (_, new_status) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(1), identity(1, 2), async {
                Ok::<_, TaskFailure>(2u64)
            })
            .ok()
            .unwrap();
        poll_held(&executor);
        release(&state, 1);
        poll_held(&executor);
        assert!(new_status.observation().exit_acknowledged);
        assert!(old_status.observation().exit_acknowledged);
        sim.update();
        let ready = events::<Completion<Native, Result<u64, TaskFailure>>>(sim.world()).to_vec();
        assert_eq!(ready.len(), 2);
        let mut writer = ResultWriter::default();
        let mut accepted = 0;
        for event in ready {
            let before = (writer.0.writes.clone(), writer.0.revision, writer.0.history);
            match flow.commit_completion(sim.world(), event, &mut writer) {
                Ok(()) => accepted += 1,
                Err(_) => assert_eq!(
                    (writer.0.writes.clone(), writer.0.revision, writer.0.history),
                    before
                ),
            }
        }
        assert_eq!(accepted, 1);
        assert_eq!(writer.0.writes, [(1, 2)]);
        assert!(!flow.request_is_current(old.token()));
        // Complete into the actual runtime inbox without draining it, then
        // cancel. Core suppresses delivery, not just our consumer gate.
        let (_, status) = flow
            .replace_background(sim.world(), owner.id(), LaneKey(2), identity(2, 3), async {
                Ok::<_, TaskFailure>(3u64)
            })
            .ok()
            .unwrap();
        poll_held(&executor);
        assert!(status.observation().exit_acknowledged);
        drop(owner);
        pump(&mut flow, 500);
        sim.update();
        assert!(events::<Completion<Native, Result<u64, TaskFailure>>>(sim.world()).is_empty());
    }
}
#[test]
fn cancelled_custom_job_is_charged_until_job_future_actually_drops() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    let executor = Arc::new(Holding::default());
    sim.app.set_executor(executor.clone());
    let (future, _state, drops) = controlled::<u64>();
    let (_, status) = flow
        .replace_background(sim.world(), owner.id(), LaneKey(1), identity(1, 1), future)
        .ok()
        .unwrap();
    poll_held(&executor);
    drop(owner);
    pump(&mut flow, 300);
    sim.run_until_parked();
    assert!(status.observation().cancellation_signalled);
    assert!(!status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts().retired_unacknowledged, 1);
    assert_eq!(drops.0.load(std::sync::atomic::Ordering::SeqCst), 0);
    drop(std::mem::take(&mut *executor.0.lock().unwrap()));
    pump(&mut flow, 100);
    assert_eq!(drops.0.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn owner_drops_during_future_completion_and_world_loss_are_not_successful_ui_delivery() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    let scope = owner.id();
    let (_, status) = flow
        .replace_local(
            sim.world(),
            scope,
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Once,
            async move {
                drop(owner);
                Ok::<_, TaskFailure>(1u64)
            },
        )
        .ok()
        .unwrap();
    sim.update();
    assert_eq!(flow.state(scope), State::Disposed);
    let ready = events::<Completion<Native, Result<u64, TaskFailure>>>(sim.world()).to_vec();
    assert_eq!(ready.len(), 1);
    let mut writer = ResultWriter::default();
    assert!(flow
        .commit_completion(sim.world(), ready[0].clone(), &mut writer)
        .is_err());
    assert_eq!(
        (writer.0.writes.len(), writer.0.history, writer.0.revision),
        (0, 0, 0)
    );
    assert_eq!(status.observation().terminal, Terminal::Succeeded);
    drop(ready);
    let world = sim.world().local::<WorldOwner>().unwrap().scope();
    let scope = flow.create_scope(world).unwrap();
    let (_, pending) = flow
        .replace_local(
            sim.world(),
            scope.id(),
            LaneKey(2),
            identity(2, 2),
            DeliveryMode::Once,
            std::future::pending::<Result<u64, TaskFailure>>(),
        )
        .ok()
        .unwrap();
    let request = flow
        .replace_request(
            sim.world(),
            scope.id(),
            LaneKey(3),
            identity(3, 3),
            DeliveryMode::Once,
        )
        .unwrap();
    let effect = flow
        .own_world_effect(sim.world(), &request, |_| {
            panic!("must not execute at World Drop")
        })
        .unwrap();
    drop(sim);
    assert!(pending.observation().exit_acknowledged);
    assert!(effect.observation().exit_acknowledged);
    assert!(!effect.observation().cleanup_attempted);
    assert_eq!(effect.observation().terminal, Terminal::DroppedUnknown);
    pump(&mut flow, 500);
    assert_eq!(flow.work_counts(), WorkCounts::default());
    assert!(!effect.observation().cleanup_attempted);
}
#[test]
fn producer_panic_and_error_normalize_and_world_effect_panic_does_not_skip_cleanup() {
    let (mut flow, _root, owner, mut sim) = fixture(1);
    let (req, _, exit, status) = flow
        .replace_feed::<_, u64>(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1, 1),
            DeliveryMode::Once,
            rndr_core::FeedConfig::default(),
        )
        .unwrap();
    assert_eq!(
        exit.run(|| panic!("controlled producer panic")),
        Err(TaskFailure::Panicked)
    );
    assert!(status.observation().exit_acknowledged);
    assert_eq!(status.observation().terminal, Terminal::Panicked);
    let effect = flow
        .own_world_effect(sim.world(), &req, |_| panic!("controlled removal panic"))
        .unwrap();
    let calls = Rc::new(Cell::new(0));
    let capture = calls.clone();
    flow.register_cleanup(owner.id(), CleanupKind::Lifetime, move |_| {
        capture.set(1);
        Ok(())
    })
    .unwrap();
    drop(owner);
    pump(&mut flow, 100);
    assert_eq!(calls.get(), 0);
    drain_world_effects(sim.world(), 2);
    pump(&mut flow, 200);
    assert_eq!(calls.get(), 1);
    assert_eq!(effect.observation().terminal, Terminal::Panicked);
    assert!(effect.observation().cleanup_attempted);
    assert!(effect.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
