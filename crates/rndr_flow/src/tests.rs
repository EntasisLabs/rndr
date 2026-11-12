use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
struct Marker;
impl rndr_core::Component for Marker {}

fn fixture() -> (Coordinator, ScopeOwner) {
    Coordinator::new(Limits::default()).unwrap()
}
fn trace(flow: &mut Coordinator, scope: ScopeId, value: u8, log: &Rc<RefCell<Vec<u8>>>) {
    let log = log.clone();
    flow.register_cleanup(scope, CleanupKind::Lifetime, move |_| {
        log.borrow_mut().push(value);
        Ok(())
    })
    .unwrap();
}
fn drain(flow: &mut Coordinator) {
    for _ in 0..10000 {
        let counts = flow.pump();
        if counts.pending_scopes == 0 && counts.pending_cleanups == 0 {
            return;
        }
    }
    panic!("drain did not converge");
}

#[test]
fn subtree_logical_death_precedes_child_first_reverse_cleanup() {
    let (mut flow, root) = fixture();
    let parent = flow.create_scope(root.id()).unwrap();
    let a = flow.create_scope(parent.id()).unwrap();
    let b = flow.create_scope(parent.id()).unwrap();
    let grandchild = flow.create_scope(a.id()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, parent.id(), 9, &log);
    trace(&mut flow, a.id(), 3, &log);
    trace(&mut flow, a.id(), 2, &log);
    trace(&mut flow, grandchild.id(), 1, &log);
    trace(&mut flow, b.id(), 4, &log);
    let live = b.liveness();
    flow.dispose(parent.id()).unwrap();
    assert!(!live.is_live());
    assert_eq!(flow.state(grandchild.id()), State::Disposed);
    assert_eq!(flow.counts().pending_cleanups, 5);
    assert!(log.borrow().is_empty());
    flow.dispose(parent.id()).unwrap();
    drain(&mut flow);
    assert_eq!(*log.borrow(), [1, 2, 3, 4, 9]);
    flow.dispose(parent.id()).unwrap();
    flow.pump();
    assert_eq!(flow.counts().scopes, 1);
    assert_eq!(flow.state(parent.id()), State::Disposed);
    assert_eq!(flow.create_scope(parent.id()).err(), Some(Error::Stale));
    assert_eq!(
        flow.set_visibility(parent.id(), State::Visible),
        Err(Error::Stale)
    );
}

#[test]
fn shared_documents_survive_presentation_and_world_loss() {
    let (mut flow, root) = fixture();
    let window_a = flow.register_world().unwrap();
    let window_b = flow.register_world().unwrap();
    assert_ne!(window_a.id(), window_b.id());
    let a = flow.create_scope(window_a.scope()).unwrap();
    let b = flow.create_scope(window_b.scope()).unwrap();
    let document = flow.create_document().unwrap();
    let _lease_a = flow.lease(a.id(), document).unwrap();
    let lease_b = flow.lease(b.id(), document).unwrap();
    assert_eq!(flow.close_document(document), Err(Error::InUse));
    assert_eq!(flow.dispose(document.scope()), Err(Error::Invalid));
    let mut world = World::new();
    window_a.bind(&mut world).unwrap();
    drop(world);
    assert_eq!(flow.state(a.id()), State::Disposed);
    assert_eq!(flow.close_document(document), Err(Error::InUse));
    drain(&mut flow);
    assert!(flow.document_is_live(document));
    assert_eq!(flow.counts().active_leases, 1);
    drop(lease_b);
    flow.pump();
    assert!(flow.document_is_live(document)); // last release never closes a document
    assert_eq!(flow.counts().leases, 0);
    flow.close_document(document).unwrap();
    drain(&mut flow);
    assert!(!flow.document_is_live(document));
    assert_eq!(flow.lease(b.id(), document).err(), Some(Error::Stale));
    flow.close_document(document).unwrap();
    drop(root);
    drain(&mut flow);
    assert_eq!(flow.counts(), Counts::default());
}

#[test]
fn removed_components_and_refused_bindings_retain_single_ownership() {
    let (mut flow, root) = fixture();
    let window = flow.register_world().unwrap();
    let window_scope = window.scope();
    let mut world = World::new();
    window.bind(&mut world).unwrap();
    let second_world = flow.register_world().unwrap();
    let refused = second_world.bind(&mut world).unwrap_err();
    assert_eq!(refused.reason, Error::InUse);
    assert_eq!(flow.state(window_scope), State::Visible);
    drop(refused.value);
    let owner = flow.create_scope(window_scope).unwrap();
    let id = owner.id();
    let entity = world.spawn(Marker);
    owner.bind(&mut world, entity).unwrap();
    let other = flow.create_scope(window_scope).unwrap();
    let other_id = other.id();
    let refused = other.bind(&mut world, entity).unwrap_err();
    assert_eq!(refused.reason, Error::Invalid);
    assert_eq!(flow.state(other_id), State::Visible);
    drop(refused.value);
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, id, 1, &log);
    // The supported removal contract consumes the returned component now.
    drop(world.remove::<ScopeOwner>(entity));
    assert_eq!(flow.state(id), State::Disposed);
    flow.dispose(id).unwrap();
    drain(&mut flow);
    assert_eq!(*log.borrow(), [1]);
    let terminal = flow.create_scope(root.id()).unwrap();
    flow.dispose(terminal.id()).unwrap();
    assert_eq!(
        terminal.bind(&mut world, entity).unwrap_err().reason,
        Error::Stale
    );
    let dead_world = flow.register_world().unwrap();
    flow.dispose(dead_world.scope()).unwrap();
    let mut fresh_world = World::new();
    assert_eq!(
        dead_world.bind(&mut fresh_world).unwrap_err().reason,
        Error::Stale
    );
}

#[test]
fn cleanup_captures_release_once_on_success_error_unwind_and_rejection_drop() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let (mut flow, root) = Coordinator::new(Limits {
        cleanups_per_scope: 3,
        ..Limits::default()
    })
    .unwrap();
    let drops = Rc::new(Cell::new(0));
    for outcome in 0..3 {
        let probe = Probe(drops.clone());
        flow.register_cleanup(root.id(), CleanupKind::Lifetime, move |_| {
            let _capture = probe;
            match outcome {
                0 => Ok(()),
                1 => Err(CleanupError),
                _ => panic!("expected capture unwind"),
            }
        })
        .unwrap();
    }
    let probe = Probe(drops.clone());
    let rejected = flow
        .register_cleanup(root.id(), CleanupKind::Lifetime, move |_| {
            let _capture = probe;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(rejected.reason, Error::Capacity);
    assert_eq!(drops.get(), 0);
    drop(rejected.value); // caller cleans a refused registration without invoking it
    assert_eq!(drops.get(), 1);
    flow.dispose(root.id()).unwrap();
    drain(&mut flow);
    assert_eq!(drops.get(), 4);
    flow.dispose(root.id()).unwrap();
    flow.pump();
    assert_eq!(drops.get(), 4);
}

#[test]
fn hidden_retains_and_suspension_restarts_only_recoverable_activity() {
    let (mut flow, root) = fixture();
    let child = flow.create_scope(root.id()).unwrap();
    let first = flow.activity(child.id()).unwrap();
    let calls = Rc::new(Cell::new(0));
    let activity_calls = calls.clone();
    flow.register_cleanup(child.id(), CleanupKind::Activity, move |_| {
        activity_calls.set(activity_calls.get() + 1);
        Ok(())
    })
    .unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, child.id(), 7, &log);
    flow.set_visibility(child.id(), State::Hidden).unwrap();
    flow.pump();
    assert!(flow.activity_is_current(first));
    assert_eq!(calls.get(), 0);
    assert_eq!(flow.counts().cleanups, 2);
    flow.suspend(child.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    assert!(!flow.activity_is_current(first));
    assert_eq!(flow.resume(child.id(), true), Err(Error::Pending));
    assert_eq!(
        flow.set_visibility(child.id(), State::Visible),
        Err(Error::Suspended)
    );
    assert_eq!(
        flow.register_cleanup(child.id(), CleanupKind::Activity, |_| Ok(()))
            .err()
            .unwrap()
            .reason,
        Error::Suspended
    );
    flow.pump();
    assert_eq!(calls.get(), 1);
    assert!(log.borrow().is_empty());
    let second = flow.resume(child.id(), false).unwrap();
    assert_ne!(first, second);
    assert!(!flow.activity_is_current(first));
    assert!(flow.activity_is_current(second));
    flow.dispose(child.id()).unwrap();
    assert!(!flow.activity_is_current(second));
    assert_eq!(flow.resume(child.id(), true), Err(Error::Stale));
    drain(&mut flow);
    assert_eq!(*log.borrow(), [7]);
}

#[test]
fn cleanup_rejection_returns_untouched_ownership_and_never_calls_in_admission() {
    let (mut flow, root) = fixture();
    let child = flow.create_scope(root.id()).unwrap();
    flow.dispose(child.id()).unwrap();
    let calls = Rc::new(Cell::new(0));
    let captured = calls.clone();
    let rejected = flow
        .register_cleanup(child.id(), CleanupKind::Lifetime, move |_| {
            captured.set(captured.get() + 1);
            Ok(())
        })
        .unwrap_err();
    assert_eq!(rejected.reason, Error::Stale);
    assert_eq!(calls.get(), 0);
    let mut queue = VecDeque::new();
    (rejected.value)(&mut Commands {
        queue: &mut queue,
        limit: 1,
    })
    .unwrap();
    drain(&mut flow);
    assert_eq!(calls.get(), 1);
}

#[test]
fn duplicate_reentrant_error_panic_cleanup_is_consumed_once_with_bounded_queue() {
    let limits = Limits {
        commands: 1,
        cleanups_per_pump: 1,
        ..Limits::default()
    };
    let (mut flow, root) = Coordinator::new(limits).unwrap();
    let a = flow.create_scope(root.id()).unwrap();
    let b = flow.create_scope(root.id()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, a.id(), 1, &log);
    trace(&mut flow, b.id(), 2, &log);
    flow.register_cleanup(a.id(), CleanupKind::Lifetime, |_| Err(CleanupError))
        .unwrap();
    flow.register_cleanup(a.id(), CleanupKind::Lifetime, |_| {
        panic!("expected unwind probe")
    })
    .unwrap();
    let a_id = a.id();
    let b_id = b.id();
    flow.register_cleanup(a_id, CleanupKind::Lifetime, move |commands| {
        commands.dispose(b_id).unwrap();
        assert_eq!(commands.dispose(a_id), Err(Error::Capacity));
        Ok(())
    })
    .unwrap();
    drop(a);
    assert_eq!(flow.counts().pending_cleanups, 4);
    assert_eq!(flow.pump().pending_cleanups, 4); // one consumed, b's one now pending
    assert_eq!(flow.state(b_id), State::Disposed);
    flow.dispose(a_id).unwrap();
    drain(&mut flow);
    assert_eq!(*log.borrow(), [1, 2]);
    let outcomes: Vec<_> = flow.diagnostics().map(|d| d.outcome).collect();
    assert_eq!(outcomes.iter().filter(|o| **o == Outcome::Error).count(), 1);
    assert_eq!(
        outcomes.iter().filter(|o| **o == Outcome::Panicked).count(),
        1
    );
    assert_eq!(flow.counts().cleanups, 0);
}

#[test]
fn reentrant_ancestor_disposal_revisits_previously_live_children_before_parent() {
    let (mut flow, root) = fixture();
    let parent = flow.create_scope(root.id()).unwrap();
    let first = flow.create_scope(parent.id()).unwrap();
    let last = flow.create_scope(parent.id()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, first.id(), 1, &log);
    trace(&mut flow, parent.id(), 3, &log);
    let parent_id = parent.id();
    let last_log = log.clone();
    flow.register_cleanup(last.id(), CleanupKind::Lifetime, move |commands| {
        last_log.borrow_mut().push(2);
        commands.dispose(parent_id).unwrap();
        Ok(())
    })
    .unwrap();
    flow.dispose(last.id()).unwrap();
    drain(&mut flow);
    assert_eq!(*log.borrow(), [2, 1, 3]);
}

#[test]
fn world_component_removal_explicit_dispose_and_world_drop_share_one_contract() {
    let (mut flow, root) = fixture();
    let window = flow.register_world().unwrap();
    let window_scope = window.scope();
    let child = flow.create_scope(window_scope).unwrap();
    let child_id = child.id();
    let grandchild = flow.create_scope(child_id).unwrap();
    let grandchild_id = grandchild.id();
    let mut world = World::new();
    window.bind(&mut world).unwrap();
    let e = world.spawn(Marker);
    child.bind(&mut world, e).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    trace(&mut flow, child_id, 2, &log);
    trace(&mut flow, grandchild_id, 1, &log);
    world.despawn_entity(e);
    assert_eq!(flow.state(child_id), State::Disposed);
    assert_eq!(flow.state(grandchild_id), State::Disposed);
    assert!(log.borrow().is_empty());
    flow.dispose(child_id).unwrap();
    drain(&mut flow);
    assert_eq!(*log.borrow(), [1, 2]);
    let replacement = flow.create_scope(window_scope).unwrap();
    assert_ne!(replacement.id(), child_id);
    let recycled = world.spawn(Marker);
    replacement.bind(&mut world, recycled).unwrap();
    drop(world);
    assert_eq!(flow.state(window_scope), State::Disposed);
    drain(&mut flow);
    assert_eq!(*log.borrow(), [1, 2]);
    assert_eq!(flow.counts().scopes, 1);
    assert_eq!(flow.state(root.id()), State::Visible);
}

#[test]
fn coordinator_drop_signals_but_never_runs_arbitrary_callbacks() {
    let (mut flow, root) = fixture();
    let live = root.liveness();
    let called = Rc::new(Cell::new(false));
    let captured = called.clone();
    flow.register_cleanup(root.id(), CleanupKind::Lifetime, move |_| {
        captured.set(true);
        Ok(())
    })
    .unwrap();
    drop(flow);
    assert!(!live.is_live());
    assert!(!called.get());
    assert_eq!(Rc::strong_count(&called), 1); // captures physically released
}

#[test]
fn all_identity_counters_refuse_exhaustion_before_replacing_valid_work() {
    let atomic = AtomicU64::new(u64::MAX - 1);
    assert_eq!(allocate_authority(&atomic), Ok(u64::MAX));
    assert_eq!(allocate_authority(&atomic), Err(Error::Exhausted));
    let (mut flow, root) = fixture();
    flow.serial = u64::MAX - 1;
    let last = flow.create_scope(root.id()).unwrap();
    assert_eq!(last.id().serial, u64::MAX);
    let counts = flow.counts();
    assert_eq!(flow.create_scope(root.id()).err(), Some(Error::Exhausted));
    assert_eq!(flow.counts(), counts);
    flow.cleanup_serial = u64::MAX - 1;
    assert_eq!(
        flow.register_cleanup(last.id(), CleanupKind::Lifetime, |_| Ok(()))
            .unwrap()
            .serial,
        u64::MAX
    );
    assert_eq!(
        flow.register_cleanup(last.id(), CleanupKind::Lifetime, |_| Ok(()))
            .err()
            .unwrap()
            .reason,
        Error::Exhausted
    );
    let before = flow.activity(last.id()).unwrap();
    flow.scopes.get_mut(&last.id()).unwrap().generation = u64::MAX;
    let valid = flow.activity(last.id()).unwrap();
    assert_ne!(before, valid);
    assert_eq!(
        flow.suspend(last.id(), Recovery::ReconcileFromDurableState),
        Err(Error::Exhausted)
    );
    assert!(flow.activity_is_current(valid));
    assert_eq!(flow.state(last.id()), State::Visible);
    // Use another coordinator for lease exhaustion, since scope serial is spent.
    let (mut lease_flow, lease_root) = fixture();
    let document = lease_flow.create_document().unwrap();
    lease_flow.lease_serial = u64::MAX - 1;
    let lease = lease_flow.lease(lease_root.id(), document).unwrap();
    assert_eq!(
        lease_flow.lease(lease_root.id(), document).err(),
        Some(Error::Exhausted)
    );
    assert_eq!(lease_flow.counts().active_leases, 1);
    assert_eq!(lease_flow.close_document(document), Err(Error::InUse));
    drop(lease);
    flow.dispose(last.id()).unwrap();
    drain(&mut flow);
    assert_eq!(flow.counts().cleanups, 0);
}

#[test]
fn capacity_diagnostics_and_foreign_stale_identity_are_bounded() {
    let limits = Limits {
        scopes: 2,
        cleanups_per_scope: 1,
        leases: 1,
        diagnostics: 1,
        ..Limits::default()
    };
    let (mut flow, root) = Coordinator::new(limits).unwrap();
    let child = flow.create_scope(root.id()).unwrap();
    assert_eq!(flow.create_scope(root.id()).err(), Some(Error::Capacity));
    flow.register_cleanup(child.id(), CleanupKind::Lifetime, |_| Ok(()))
        .unwrap();
    assert_eq!(
        flow.register_cleanup(child.id(), CleanupKind::Lifetime, |_| Ok(()))
            .err()
            .unwrap()
            .reason,
        Error::Capacity
    );
    let (mut other, other_root) = fixture();
    assert_ne!(root.id(), other_root.id());
    assert_eq!(other.create_scope(root.id()).err(), Some(Error::Stale));
    assert_eq!(other.dispose(child.id()), Err(Error::Stale));
    flow.dispose(child.id()).unwrap();
    drain(&mut flow);
    assert_eq!(flow.diagnostics().count(), 1);
    assert_eq!(flow.dropped_diagnostics(), 1);
    let fresh = flow.create_scope(root.id()).unwrap();
    assert_ne!(fresh.id(), child.id());
    assert_eq!(flow.activity(child.id()), Err(Error::Stale));
    assert_eq!(
        Coordinator::new(Limits {
            scopes: 0,
            ..limits
        })
        .err(),
        Some(Error::Invalid)
    );
    assert_eq!(other.state(other_root.id()), State::Visible);
}

#[test]
fn lease_capacity_does_not_evict_admitted_presentations() {
    let (mut flow, root) = Coordinator::new(Limits {
        leases: 1,
        ..Limits::default()
    })
    .unwrap();
    let document = flow.create_document().unwrap();
    let held = flow.lease(root.id(), document).unwrap();
    assert_eq!(flow.lease(root.id(), document).err(), Some(Error::Capacity));
    assert_eq!(flow.close_document(document), Err(Error::InUse));
    assert_eq!(flow.counts().leases, 1);
    drop(held);
    let replacement = flow.lease(root.id(), document).unwrap();
    assert_eq!(flow.counts().leases, 1);
    drop(replacement);
}

#[test]
fn sim_host_keeps_logical_death_separate_from_explicit_cleanup_pump() {
    for seed in [0, 1, 42] {
        let (mut flow, root) = fixture();
        let mut sim = rndr_core::Sim::with_seed(seed);
        let window = flow.register_world().unwrap();
        window.bind(sim.world()).unwrap();
        let scope = flow.create_scope(root.id()).unwrap();
        let id = scope.id();
        let entity = sim.world().spawn(Marker);
        scope.bind(sim.world(), entity).unwrap();
        let calls = Rc::new(Cell::new(0));
        let captured = calls.clone();
        flow.register_cleanup(id, CleanupKind::Lifetime, move |_| {
            captured.set(captured.get() + 1);
            Ok(())
        })
        .unwrap();
        sim.update();
        assert_eq!(flow.state(id), State::Visible);
        sim.world().despawn_entity(entity);
        assert_eq!(flow.state(id), State::Disposed);
        sim.advance(std::time::Duration::from_millis(16));
        sim.update();
        assert_eq!(calls.get(), 0); // no automatic plugin or callback in Drop
        drain(&mut flow);
        assert_eq!(calls.get(), 1);
        drop(sim);
        drain(&mut flow);
        assert_eq!(flow.counts().scopes, 1);
        drop(root);
        drain(&mut flow);
        assert_eq!(flow.counts(), Counts::default());
    }
}

#[test]
fn one_hundred_lifecycle_cycles_return_every_storage_count_to_baseline() {
    let (mut flow, root) = Coordinator::new(Limits {
        cleanups_per_pump: 1,
        diagnostics: 8,
        ..Limits::default()
    })
    .unwrap();
    let baseline = flow.counts();
    assert_eq!(baseline.scopes, 1);
    let calls = Rc::new(Cell::new(0));
    for cycle in 1..=100 {
        let window = flow.register_world().unwrap();
        let parent = flow.create_scope(window.scope()).unwrap();
        let child = flow.create_scope(parent.id()).unwrap();
        let document = flow.create_document().unwrap();
        let lease = flow.lease(child.id(), document).unwrap();
        for id in [parent.id(), child.id(), document.scope()] {
            let calls = calls.clone();
            flow.register_cleanup(id, CleanupKind::Lifetime, move |_| {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .unwrap();
        }
        flow.set_visibility(child.id(), State::Hidden).unwrap();
        let activity = flow.activity(child.id()).unwrap();
        flow.suspend(child.id(), Recovery::ReconcileFromDurableState)
            .unwrap();
        assert_ne!(flow.resume(child.id(), true).unwrap(), activity);
        drop(parent);
        drop(lease);
        drop(window);
        flow.close_document(document).unwrap();
        drain(&mut flow);
        assert_eq!(flow.counts(), baseline, "cycle {cycle}");
        assert_eq!(calls.get(), cycle * 3);
        assert!(flow.diagnostics().count() <= 8);
        println!("F3 cycle={cycle} scopes=1 cleanups=0 leases=0 active_leases=0 pending_scopes=0 pending_cleanups=0 queued_commands=0 callbacks={}", calls.get());
    }
    drop(root);
    drain(&mut flow);
    assert_eq!(flow.counts(), Counts::default());
}
