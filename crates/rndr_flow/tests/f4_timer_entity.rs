use rndr_core::{events, every, Sim};
use rndr_flow::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};
#[derive(Default)]
struct Writer {
    writes: usize,
    history: usize,
    revision: u64,
}
impl CompletionTarget<u64, u64> for Writer {
    type Output = ();
    type Error = ();
    fn commit(&mut self, id: &Identity<u64>, _: u64) -> Result<(), ()> {
        if id.target != 17 || id.revision != self.revision {
            return Err(());
        }
        self.writes += 1;
        self.history += 1;
        self.revision += 1;
        Ok(())
    }
}
fn id(op: u64) -> Identity<u64> {
    Identity {
        target: 17,
        operation: op,
        revision: 0,
    }
}
#[test]
fn native_every_repeated_and_already_published_ticks_are_gated_without_new_timer_wheel() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            id(1),
            DeliveryMode::Once,
        )
        .unwrap();
    let timer = every(
        sim.world(),
        Duration::from_millis(1),
        request.completion(None, 1u64),
    );
    // A consumer may own an existing cancellation effect. This status certifies
    // explicit cancellation-effect completion, NOT native timer/event exit.
    flow.own_world_effect(sim.world(), &request, move |_| drop(timer))
        .unwrap();
    sim.advance(Duration::from_millis(3));
    sim.update();
    let ticks = events::<Completion<u64, u64>>(sim.world()).to_vec();
    assert_eq!(ticks.len(), 3);
    let mut writer = Writer::default();
    let mut accepted = 0;
    for tick in ticks {
        let before = (writer.writes, writer.history, writer.revision);
        if flow
            .commit_completion(sim.world(), tick, &mut writer)
            .is_ok()
        {
            accepted += 1;
        } else {
            assert_eq!((writer.writes, writer.history, writer.revision), before);
        }
    }
    assert_eq!(
        (accepted, writer.writes, writer.history, writer.revision),
        (1, 1, 1, 1)
    );
    drop(owner);
    sim.advance(Duration::from_millis(2));
    sim.update();
    let ticks = events::<Completion<u64, u64>>(sim.world()).to_vec();
    assert_eq!(ticks.len(), 2);
    for tick in ticks {
        assert!(flow
            .commit_completion(sim.world(), tick, &mut writer)
            .is_err());
        assert_eq!((writer.writes, writer.history, writer.revision), (1, 1, 1));
    }
    for _ in 0..100 {
        flow.pump_work(1);
    }
    drain_world_effects(sim.world(), 3);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    sim.advance(Duration::from_secs(1));
    sim.update();
    assert!(events::<Completion<u64, u64>>(sim.world()).is_empty());
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
#[test]
fn recycled_entity_never_substitutes_recreated_semantic_scope() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let parent = world.scope();
    world.bind(sim.world()).unwrap();
    let owner = flow.create_scope(parent).unwrap();
    let request = flow
        .replace_request(
            sim.world(),
            owner.id(),
            LaneKey(1),
            id(1),
            DeliveryMode::Once,
        )
        .unwrap();
    struct Marker;
    impl rndr_core::Component for Marker {}
    let old = sim.world().spawn(Marker);
    owner.bind(sim.world(), old).unwrap();
    sim.world().despawn_entity(old);
    let fresh = sim.world().spawn(Marker);
    assert_eq!(old.index(), fresh.index());
    assert_ne!(old, fresh);
    let new_owner = flow.create_scope(parent).unwrap();
    let new = flow
        .replace_request(
            sim.world(),
            new_owner.id(),
            LaneKey(1),
            id(1),
            DeliveryMode::Once,
        )
        .unwrap();
    new_owner.bind(sim.world(), fresh).unwrap();
    let mut writer = Writer::default();
    assert!(flow
        .commit_completion(sim.world(), request.completion(None, 1), &mut writer)
        .is_err());
    assert_eq!((writer.writes, writer.history, writer.revision), (0, 0, 0));
    flow.commit_completion(sim.world(), new.completion(None, 1), &mut writer)
        .unwrap();
    assert_eq!((writer.writes, writer.history, writer.revision), (1, 1, 1));
}
#[test]
fn extracted_world_effect_allows_bounded_callback_reentry_without_losing_new_effect() {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let old = flow.create_scope(world.scope()).unwrap();
    let live = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    flow.configure_work(WorkLimits {
        slots: 2,
        effects_per_world: 2,
        ..WorkLimits::default()
    })
    .unwrap();
    let first = flow
        .replace_request(sim.world(), old.id(), LaneKey(1), id(1), DeliveryMode::Once)
        .unwrap();
    let next = flow
        .replace_request(
            sim.world(),
            live.id(),
            LaneKey(1),
            id(2),
            DeliveryMode::Once,
        )
        .unwrap();
    let flow = Rc::new(RefCell::new(flow));
    let reentrant = flow.clone();
    let calls = Rc::new(Cell::new(0));
    let seen = calls.clone();
    flow.borrow_mut()
        .own_world_effect(sim.world(), &first, move |world| {
            seen.set(seen.get() + 1);
            let seen = seen.clone();
            reentrant
                .borrow_mut()
                .own_world_effect(world, &next, move |_| seen.set(seen.get() + 1))
                .unwrap();
        })
        .unwrap();
    drop(old);
    for _ in 0..100 {
        flow.borrow_mut().pump_work(1);
    }
    drain_world_effects(sim.world(), 1);
    assert_eq!(calls.get(), 1);
    for _ in 0..100 {
        flow.borrow_mut().pump_work(1);
    }
    assert_eq!(flow.borrow().work_counts().charged, 1);
    drop(root);
    for _ in 0..100 {
        flow.borrow_mut().pump_work(1);
    }
    drain_world_effects(sim.world(), 10);
    for _ in 0..100 {
        flow.borrow_mut().pump_work(1);
    }
    assert_eq!(calls.get(), 2);
    assert_eq!(flow.borrow().work_counts(), WorkCounts::default());
    drop(live);
}
