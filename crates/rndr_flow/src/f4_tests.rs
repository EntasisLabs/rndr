use super::*;
use std::future::{pending, Future};
use std::pin::Pin;
use std::task::{Context, Poll};

fn fixture() -> (Coordinator, ScopeOwner, ScopeOwner, rndr_core::Sim) {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = rndr_core::Sim::new();
    let world = flow.register_world().unwrap();
    let scope = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    (flow, root, scope, sim)
}
fn identity(op: u64) -> Identity<u64> {
    Identity {
        target: 5,
        operation: op,
        revision: 0,
    }
}
fn step(flow: &mut Coordinator) {
    let w = flow.pump_work(1);
    assert!(w.units <= 1);
    assert_eq!(
        w.units,
        w.metadata_visits + w.lease_visits + w.ack_visits + w.restarts + w.commands
    );
}
#[test]
fn lane_generation_max_refuses_atomically_with_old_valid_request() {
    let (mut flow, _root, owner, mut sim) = fixture();
    flow.replace_request(
        sim.world(),
        owner.id(),
        LaneKey(1),
        identity(1),
        DeliveryMode::Once,
    )
    .unwrap();
    flow.scopes
        .get_mut(&owner.id())
        .unwrap()
        .lanes
        .get_mut(&LaneKey(1))
        .unwrap()
        .generation = u64::MAX - 1;
    let old = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(2),
            DeliveryMode::Once,
        )
        .unwrap();
    let before = (
        flow.counts(),
        flow.work_counts(),
        flow.diagnostics.clone(),
        flow.rescan,
        flow.scopes[&owner.id()].lanes.len(),
    );
    let refused = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(3),
            DeliveryMode::Once,
            pending::<Result<u64, TaskFailure>>(),
        )
        .err()
        .unwrap();
    assert_eq!(refused.reason, Error::Exhausted);
    assert!(flow.request_is_current(old.token()));
    assert_eq!(old.token().generation, u64::MAX);
    assert_eq!(
        (
            flow.counts(),
            flow.work_counts(),
            flow.diagnostics.clone(),
            flow.rescan,
            flow.scopes[&owner.id()].lanes.len()
        ),
        before
    );
    let lane = &flow.scopes[&owner.id()].lanes[&LaneKey(1)];
    assert_eq!(lane.generation, u64::MAX);
    assert!(!lane.consumed);
    assert_eq!(
        lane.identity.downcast_ref::<Identity<u64>>(),
        Some(&identity(2))
    );
}
#[test]
fn unrecoverable_refusal_does_not_advance_activity_or_cancel_work() {
    let (mut flow, _root, owner, mut sim) = fixture();
    let (old, _sender, _exit, status) = flow
        .replace_feed::<_, u64>(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1),
            DeliveryMode::Ordered { recoverable: false },
            rndr_core::FeedConfig::default(),
        )
        .unwrap();
    let activity = flow.activity(owner.id()).unwrap();
    let counts = flow.work_counts();
    assert_eq!(
        flow.suspend(owner.id(), Recovery::ReconcileFromDurableState),
        Err(Error::Invalid)
    );
    assert_eq!(flow.activity(owner.id()).unwrap(), activity);
    assert_eq!(flow.work_counts(), counts);
    assert!(flow.request_is_current(old.token()));
    assert!(!status.observation().cancellation_signalled);
}
#[test]
fn one_unit_default_capacity_slots_lanes_and_full_commands_progress_without_scans() {
    let (mut flow, root, owner, mut sim) = fixture();
    let parent = owner.id();
    let mut owners = vec![owner];
    let mut exits = Vec::new();
    let n = WorkLimits::default().slots;
    let mut first = None;
    for index in 0..n {
        if index % 32 == 0 {
            owners.push(flow.create_scope(parent).unwrap());
        }
        let (request, _sender, exit, _status) = flow
            .replace_feed::<_, u64>(
                sim.world(),
                owners.last().unwrap().id(),
                LaneKey((index % 32) as u16),
                identity(index as u64),
                DeliveryMode::Ordered { recoverable: true },
                rndr_core::FeedConfig::default(),
            )
            .unwrap();
        if first.is_none() {
            first = Some(request);
        }
        exits.push(exit);
    }
    assert_eq!(flow.work_counts().charged, n);
    assert_eq!(
        flow.replace_local(
            sim.world(),
            parent,
            LaneKey(100),
            identity(100),
            DeliveryMode::Once,
            pending::<Result<u64, TaskFailure>>()
        )
        .err()
        .unwrap()
        .reason,
        Error::Capacity
    );
    assert!(flow.request_is_current(first.unwrap().token()));
    // Fill the exact existing command capacity; admitted dead flags do not
    // allocate any additional commands/queues to finish disposal.
    for _ in 0..flow.limits.commands {
        flow.commands.push_back(root.id());
    }
    drop(exits);
    drop(root);
    let mut units = 0;
    while !flow.scopes.is_empty() || flow.work.has_slots() {
        step(&mut flow);
        units += 1;
        assert!(units < 100_000, "units={units}");
    }
    assert_eq!(flow.work_counts(), WorkCounts::default());
    assert_eq!(flow.counts(), Counts::default());
    assert!(flow.diagnostics.len() <= flow.limits.diagnostics);
    println!("F4 budget capacity={n} lanes_per_scope=32 queue=1024 units={units} charged=0");
    drop(owners);
}
#[test]
fn effect_capacity_refusal_returns_untouched_capture_and_pending_is_not_acknowledged() {
    let (mut flow, _root, owner, mut sim) = fixture();
    flow.configure_work(WorkLimits {
        slots: 8,
        effects_per_world: 1,
        ..WorkLimits::default()
    })
    .unwrap();
    let req = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1),
            DeliveryMode::Once,
        )
        .unwrap();
    let first = flow.own_world_effect(sim.world(), &req, |_| {}).unwrap();
    let value = std::rc::Rc::new(std::cell::Cell::new(0));
    let capture = value.clone();
    let refused = flow
        .own_world_effect(sim.world(), &req, move |_| capture.set(1))
        .err()
        .unwrap();
    assert_eq!(refused.reason, Error::Capacity);
    assert_eq!(value.get(), 0);
    drop(refused);
    assert_eq!(value.get(), 0);
    assert!(!first.observation().retired);
    drop(owner);
    for _ in 0..100 {
        step(&mut flow);
    }
    assert!(!first.observation().exit_acknowledged);
    assert!(!first.observation().cleanup_attempted);
    drain_world_effects(sim.world(), 2);
    for _ in 0..100 {
        step(&mut flow);
    }
    assert!(first.observation().cleanup_attempted);
    assert!(first.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn work_serial_max_refuses_without_replacing_or_cancelling_valid_last_slot() {
    let (mut flow, _root, owner, mut sim) = fixture();
    flow.work.set_serial(u64::MAX - 1);
    let (old, status) = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1),
            DeliveryMode::Once,
            pending::<Result<u64, TaskFailure>>(),
        )
        .unwrap();
    let before = (
        flow.counts(),
        flow.work_counts(),
        flow.scopes[&owner.id()].lanes[&LaneKey(1)].generation,
        flow.diagnostics.clone(),
    );
    let refused = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(2),
            DeliveryMode::Once,
            pending::<Result<u64, TaskFailure>>(),
        )
        .err()
        .unwrap();
    assert_eq!(refused.reason, Error::Exhausted);
    assert!(flow.request_is_current(old.token()));
    assert!(!status.observation().cancellation_signalled);
    assert_eq!(
        (
            flow.counts(),
            flow.work_counts(),
            flow.scopes[&owner.id()].lanes[&LaneKey(1)].generation,
            flow.diagnostics.clone()
        ),
        before
    );
    drop(owner);
    for _ in 0..100 {
        step(&mut flow);
    }
    sim.update();
    for _ in 0..100 {
        step(&mut flow);
    }
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn default_lane_capacity_exact_native_identity_and_terminal_sequence_refusals_are_untouched() {
    let (mut flow, _root, owner, mut sim) = fixture();
    for key in 0..32 {
        flow.replace_request(
            sim.world(),
            owner.id(),
            LaneKey(key),
            Identity {
                target: (1u64, 2u64, 3u64),
                operation: 7,
                revision: 0,
            },
            DeliveryMode::Ordered { recoverable: true },
        )
        .unwrap();
    }
    let before = flow.counts();
    assert_eq!(
        flow.replace_request(
            sim.world(),
            owner.id(),
            LaneKey(32),
            identity(9),
            DeliveryMode::Once
        )
        .err(),
        Some(Error::Capacity)
    );
    assert_eq!(flow.counts(), before);
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(0),
            Identity {
                target: (1u64, 2u64, 3u64),
                operation: 7,
                revision: 0,
            },
            DeliveryMode::Ordered { recoverable: true },
        )
        .unwrap();
    #[derive(Default)]
    struct Writer(usize);
    impl CompletionTarget<(u64, u64, u64), u64> for Writer {
        type Output = ();
        type Error = ();
        fn commit(&mut self, _: &Identity<(u64, u64, u64)>, _: u64) -> Result<(), ()> {
            self.0 += 1;
            Ok(())
        }
    }
    let mut writer = Writer::default();
    for bad in [
        Identity {
            target: (9, 2, 3),
            operation: 7,
            revision: 0,
        },
        Identity {
            target: (1, 9, 3),
            operation: 7,
            revision: 0,
        },
        Identity {
            target: (1, 2, 9),
            operation: 7,
            revision: 0,
        },
        Identity {
            target: (1, 2, 3),
            operation: 8,
            revision: 0,
        },
        Identity {
            target: (1, 2, 3),
            operation: 7,
            revision: 1,
        },
    ] {
        let mut forged = request.clone();
        forged.identity = bad;
        assert!(flow
            .commit_completion(sim.world(), forged.completion(Some(1), 1), &mut writer)
            .is_err());
        assert_eq!(writer.0, 0);
        assert_eq!(flow.counts(), before);
    }
    flow.commit_completion(
        sim.world(),
        request.completion(Some(u64::MAX), 1),
        &mut writer,
    )
    .unwrap();
    assert_eq!(writer.0, 1);
    for sequence in [0, 1, u64::MAX] {
        assert!(flow
            .commit_completion(
                sim.world(),
                request.completion(Some(sequence), 1),
                &mut writer
            )
            .is_err());
        assert_eq!(writer.0, 1);
    }
    assert_eq!(flow.suspend_requests(owner.id()), Ok(()));
    for _ in 0..100 {
        step(&mut flow);
    }
    flow.resume(owner.id(), true).unwrap();
    assert!(!flow.request_is_current(request.token()));
}
struct DropPanic;
impl Future for DropPanic {
    type Output = Result<u64, TaskFailure>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Pending
    }
}
impl Drop for DropPanic {
    fn drop(&mut self) {
        panic!("controlled future destructor panic");
    }
}
#[test]
fn destructor_panic_is_observed_after_ack_and_independent_cleanup_continues() {
    let (mut flow, _root, owner, mut sim) = fixture();
    let (_, status) = flow
        .replace_local(
            sim.world(),
            owner.id(),
            LaneKey(1),
            identity(1),
            DeliveryMode::Once,
            DropPanic,
        )
        .unwrap();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let capture = calls.clone();
    flow.register_cleanup(owner.id(), CleanupKind::Lifetime, move |_| {
        capture.set(capture.get() + 1);
        Ok(())
    })
    .unwrap();
    sim.update();
    drop(owner);
    for _ in 0..100 {
        step(&mut flow);
    }
    assert_eq!(calls.get(), 1);
    assert!(!status.observation().exit_acknowledged);
    sim.update();
    for _ in 0..100 {
        step(&mut flow);
    }
    let obs = status.observation();
    assert_eq!(obs.terminal, Terminal::Panicked);
    assert!(obs.exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
