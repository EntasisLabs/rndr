use super::*;

#[test]
fn newly_suspended_child_between_pumps_precedes_waiting_parent_cleanup() {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let parent = flow.create_scope(root.id()).unwrap();
    let child = flow.create_scope(parent.id()).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for (id, value) in [(parent.id(), 2), (child.id(), 1)] {
        let log = log.clone();
        flow.register_cleanup(id, CleanupKind::Activity, move |_| {
            log.borrow_mut().push(value);
            Ok(())
        })
        .unwrap();
    }
    flow.suspend(parent.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    for _ in 0..100 {
        if matches!(flow.traversal, Some(Traversal::Cleanup(id, _)) if id == parent.id()) {
            break;
        }
        step(&mut flow, 1);
    }
    assert!(log.borrow().is_empty());
    assert!(matches!(flow.traversal, Some(Traversal::Cleanup(id, _)) if id == parent.id()));
    flow.suspend(child.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    for _ in 0..100 {
        step(&mut flow, 1);
    }
    assert_eq!(*log.borrow(), [1, 2]);
    drop(root);
}

#[test]
fn unrelated_live_registrations_cannot_starve_an_inflight_dead_subtree() {
    let (mut flow, root) = Coordinator::new(Limits {
        cleanups_per_scope: 1024,
        ..Limits::default()
    })
    .unwrap();
    let dead = flow.create_scope(root.id()).unwrap();
    let id = dead.id();
    drop(dead);
    assert_eq!(step(&mut flow, 1).restarts, 1);
    let mut reclaimed = false;
    for _ in 0..100 {
        flow.register_cleanup(root.id(), CleanupKind::Lifetime, |_| Ok(()))
            .unwrap();
        let work = step(&mut flow, 1);
        assert_eq!(work.restarts, 0);
        if !flow.scopes.contains_key(&id) {
            reclaimed = true;
            break;
        }
    }
    assert!(reclaimed);
    drop(root);
}

#[test]
fn full_default_lease_table_sweeps_with_tiny_budgets_and_no_callbacks() {
    let n = Limits::default().leases;
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let document = flow.create_document().unwrap();
    let mut leases = Vec::new();
    for _ in 0..n {
        leases.push(flow.lease(root.id(), document).unwrap());
    }
    assert_eq!(flow.lease(root.id(), document).err(), Some(Error::Capacity));
    drop(leases);
    let exact = flow.counts();
    assert_eq!(exact.leases, n);
    assert_eq!(exact.active_leases, 0);
    assert_eq!(flow.close_document(document), Ok(()));
    let mut units = 0;
    while !flow.leases.is_empty() || flow.scopes.len() != 1 {
        let work = step(&mut flow, 1);
        units += work.units;
        assert_eq!(work.callbacks, 0);
        assert!(units <= 2 * n + 20);
    }
    assert_eq!(flow.counts().leases, 0);
    println!("budget lease-table leases={n} units={units}");
    drop(root);
}

#[test]
fn queued_unwind_commands_are_charged_fifo_before_next_callback() {
    let (mut flow, root) = Coordinator::new(Limits {
        commands: 2,
        cleanups_per_pump: 1,
        ..Limits::default()
    })
    .unwrap();
    let (mut foreign, other_root) = Coordinator::new(Limits::default()).unwrap();
    let other = foreign.create_scope(other_root.id()).unwrap();
    let targets = [other_root.id(), other.id()];
    flow.register_cleanup(root.id(), CleanupKind::Lifetime, move |q| {
        for target in targets {
            q.dispose(target).unwrap();
        }
        assert_eq!(q.dispose(targets[0]), Err(Error::Capacity));
        panic!("expected incremental queue unwind");
    })
    .unwrap();
    drop(root);
    let mut units = 0;
    loop {
        units += 1;
        assert!(units < 30);
        if step(&mut flow, 1).callbacks == 1 {
            break;
        }
    }
    assert_eq!(flow.counts().queued_commands, 2);
    assert_eq!(step(&mut flow, 0), PumpWork::default());
    assert_eq!(step(&mut flow, 1).commands, 1);
    assert_eq!(flow.counts().queued_commands, 1);
    assert_eq!(step(&mut flow, 1).commands, 1);
    assert_eq!(flow.counts().queued_commands, 0);
    let errors: Vec<_> = flow
        .diagnostics()
        .filter(|d| d.outcome == Outcome::Error)
        .map(|d| d.scope)
        .collect();
    assert_eq!(errors, targets);
    for _ in 0..20 {
        step(&mut flow, 1);
    }
    assert_eq!(flow.counts(), Counts::default());
}

use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn step(flow: &mut Coordinator, budget: usize) -> PumpWork {
    let work = flow.pump_work(budget);
    assert!(work.units <= budget);
    assert_eq!(
        work.units,
        work.metadata_visits + work.lease_visits + work.restarts + work.commands
    );
    assert!(work.callbacks <= flow.limits.cleanups_per_pump);
    work
}

#[test]
fn capacity_wide_and_deep_callback_free_retirement_is_incremental() {
    for deep in [false, true] {
        let n = Limits::default().scopes;
        let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
        let mut parent = root.id();
        let mut owners = Vec::new();
        for _ in 1..n {
            let child = flow.create_scope(parent).unwrap();
            if deep {
                parent = child.id();
            }
            owners.push(child);
        }
        assert_eq!(flow.create_scope(root.id()).err(), Some(Error::Capacity));
        assert_eq!(step(&mut flow, 0), PumpWork::default());
        assert_eq!(flow.scopes.len(), n);
        // Put the cursor inside the tree, then drop an owner BETWEEN pumps.
        step(&mut flow, 17);
        drop(root);
        assert_eq!(flow.counts().pending_scopes, n);
        let first = step(&mut flow, 1);
        assert_eq!(first.restarts, 1);
        assert_eq!(flow.scopes.len(), n); // no hidden eager reclamation
        let mut units = 1;
        while !flow.scopes.is_empty() {
            let work = step(&mut flow, 1);
            assert_eq!(work.callbacks, 0);
            units += work.units;
            assert!(units <= 6 * n + 3, "deep={deep} units={units}");
        }
        assert_eq!(flow.counts(), Counts::default());
        println!("budget callback-free deep={deep} scopes={n} units={units}");
        drop(owners);
    }
}

#[test]
fn saturated_metadata_callbacks_commands_and_leases_all_make_progress() {
    let n = 128;
    let (mut flow, root) = Coordinator::new(Limits {
        scopes: n + 2,
        leases: n,
        cleanups_per_scope: 4,
        cleanups_per_pump: 1,
        commands: 1,
        diagnostics: 1,
    })
    .unwrap();
    let document = flow.create_document().unwrap();
    let mut owners = Vec::new();
    let mut leases = Vec::new();
    let calls = Rc::new(Cell::new(0));
    for _ in 0..n {
        let child = flow.create_scope(root.id()).unwrap();
        leases.push(flow.lease(child.id(), document).unwrap());
        for _ in 0..4 {
            let calls = calls.clone();
            let target = root.id();
            flow.register_cleanup(child.id(), CleanupKind::Lifetime, move |q| {
                calls.set(calls.get() + 1);
                q.dispose(target).unwrap();
                assert_eq!(q.dispose(target), Err(Error::Capacity));
                Ok(())
            })
            .unwrap();
        }
        assert_eq!(
            flow.register_cleanup(child.id(), CleanupKind::Lifetime, |_| Ok(()))
                .unwrap_err()
                .reason,
            Error::Capacity
        );
        owners.push(child);
    }
    assert_eq!(flow.create_scope(root.id()).err(), Some(Error::Capacity));
    assert_eq!(flow.lease(root.id(), document).err(), Some(Error::Capacity));
    drop(root);
    let before = flow.counts();
    assert_eq!(before.scopes, n + 2);
    assert_eq!(before.pending_cleanups, 4 * n);
    assert_eq!(before.active_leases, 0);
    assert_eq!(before.leases, n);
    let mut units = 0;
    while !flow.scopes.is_empty() || !flow.leases.is_empty() || !flow.commands.is_empty() {
        units += step(&mut flow, 1).units;
        assert!(units < 20 * (n + 2) + 8 * n);
    }
    assert_eq!(calls.get(), 4 * n);
    assert_eq!(flow.counts(), Counts::default());
    assert!(flow.dropped_diagnostics() > 0);
    drop((owners, leases));
}

#[test]
fn newly_dead_ancestor_restarts_before_parent_and_keeps_reverse_order() {
    for drop_owner in [false, true] {
        let (mut flow, root) = Coordinator::new(Limits {
            cleanups_per_pump: 1,
            ..Limits::default()
        })
        .unwrap();
        let parent = flow.create_scope(root.id()).unwrap();
        let first = flow.create_scope(parent.id()).unwrap();
        let last = flow.create_scope(parent.id()).unwrap();
        let log = Rc::new(RefCell::new(Vec::new()));
        for (id, value) in [(first.id(), 11), (first.id(), 10), (parent.id(), 30)] {
            let log = log.clone();
            flow.register_cleanup(id, CleanupKind::Lifetime, move |_| {
                log.borrow_mut().push(value);
                Ok(())
            })
            .unwrap();
        }
        let parent_id = parent.id();
        let mut retained = Some(parent);
        let captured = if drop_owner { retained.take() } else { None };
        let out = log.clone();
        flow.register_cleanup(last.id(), CleanupKind::Activity, move |q| {
            out.borrow_mut().push(20);
            if drop_owner {
                drop(captured);
            } else {
                q.dispose(parent_id).unwrap();
            }
            Ok(())
        })
        .unwrap();
        flow.suspend(last.id(), Recovery::ReconcileFromDurableState)
            .unwrap();
        let mut units = 0;
        while flow.scopes.len() != 1 {
            units += step(&mut flow, 1).units;
            assert!(units < 300);
        }
        assert_eq!(*log.borrow(), [20, 10, 11, 30]);
        assert_eq!(flow.state(first.id()), State::Disposed);
        drop(root);
    }
}

#[test]
fn lease_ancestry_is_resumable_and_counts_remain_exact_without_pumping() {
    let (mut flow, root) = Coordinator::new(Limits::default()).unwrap();
    let document = flow.create_document().unwrap();
    let mut owners = Vec::new();
    let mut parent = root.id();
    for _ in 0..256 {
        let owner = flow.create_scope(parent).unwrap();
        parent = owner.id();
        owners.push(owner);
    }
    let lease = flow.lease(parent, document).unwrap();
    // Start a live ancestry validation. Death halfway through must be caught
    // on this or the next sweep without restarting lease progress on every drop.
    step(&mut flow, 41);
    drop(owners.remove(0));
    assert_eq!(flow.counts().active_leases, 0);
    assert_eq!(flow.counts().leases, 1);
    assert_eq!(flow.counts().pending_scopes, 256);
    assert_eq!(step(&mut flow, 0), PumpWork::default());
    let mut units = 0;
    while !flow.leases.is_empty() || flow.scopes.len() != 2 {
        units += step(&mut flow, 1).units;
        assert!(units < 4000);
    }
    assert!(flow.document_is_live(document));
    drop(lease);
    flow.close_document(document).unwrap();
    for _ in 0..30 {
        step(&mut flow, 1);
    }
    assert_eq!(flow.counts().scopes, 1);
    drop(root);
}

#[test]
fn callback_entry_scans_and_revalidation_are_charged_not_bulk() {
    let (mut flow, root) = Coordinator::new(Limits {
        cleanups_per_scope: 4096,
        ..Limits::default()
    })
    .unwrap();
    let calls = Rc::new(Cell::new(0));
    let out = calls.clone();
    flow.register_cleanup(root.id(), CleanupKind::Activity, move |_| {
        out.set(out.get() + 1);
        Ok(())
    })
    .unwrap();
    for _ in 1..4096 {
        flow.register_cleanup(root.id(), CleanupKind::Lifetime, |_| Ok(()))
            .unwrap();
    }
    flow.suspend(root.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    step(&mut flow, 100);
    assert_eq!(calls.get(), 0); // no rposition scanning all 4096 entries
    let mut units = 100;
    while calls.get() == 0 {
        units += step(&mut flow, 1).units;
        assert!(units < 8300);
    }
    assert!(units > 8000);
    assert_eq!(flow.counts().cleanups, 4095);
    flow.resume(root.id(), true).unwrap();
    drop(root);
    for _ in 0..200 {
        step(&mut flow, 100);
    }
    assert_eq!(flow.counts(), Counts::default());
}
