use super::*;
use std::cell::Cell;
use std::rc::Rc;

fn fixture(capacity: usize) -> (Coordinator, ScopeOwner, DocumentId, Rc<Cell<usize>>) {
    let (mut flow, root) = Coordinator::new(Limits {
        leases: capacity,
        ..Limits::default()
    })
    .unwrap();
    let document = flow.create_document().unwrap();
    let calls = Rc::new(Cell::new(0));
    let observed = calls.clone();
    flow.register_cleanup(root.id(), CleanupKind::Lifetime, move |_| {
        observed.set(observed.get() + 1);
        Ok(())
    })
    .unwrap();
    (flow, root, document, calls)
}

fn refusal(
    flow: &mut Coordinator,
    scope: ScopeId,
    document: DocumentId,
    expected: Error,
    calls: &Cell<usize>,
) {
    let counts = flow.counts();
    let counters = (flow.serial, flow.cleanup_serial, flow.lease_serial);
    let leases: Vec<_> = flow
        .leases
        .iter()
        .map(|(&id, lease)| (id, lease.scope, lease.document, lease.held.clone()))
        .collect();
    let diagnostics = flow.diagnostics.clone();
    let dropped = flow.dropped_diagnostics();
    let commands = flow.commands.clone();
    let dirty = flow.dirty.load(Ordering::Acquire);
    let cursors = (
        flow.rescan,
        flow.lease_after,
        flow.lease_check,
        flow.lease_turn,
    );
    // Avoid changing production traits solely for test snapshot comparison.
    let traversal = |cursor: Option<Traversal>| match cursor {
        None => None,
        Some(Traversal::Enter(id)) => Some((0, id, None)),
        Some(Traversal::Cleanup(id, after)) => Some((1, id, after)),
        Some(Traversal::Exit(id)) => Some((2, id, None)),
    };
    let before_traversal = traversal(flow.traversal);
    assert_eq!(calls.get(), 0);
    assert_eq!(flow.lease(scope, document).err(), Some(expected));
    assert_eq!(flow.counts(), counts);
    assert_eq!(
        (flow.serial, flow.cleanup_serial, flow.lease_serial),
        counters
    );
    assert_eq!(flow.diagnostics, diagnostics);
    assert_eq!(flow.dropped_diagnostics(), dropped);
    assert_eq!(flow.commands, commands);
    assert_eq!(flow.dirty.load(Ordering::Acquire), dirty);
    assert_eq!(
        (
            flow.rescan,
            flow.lease_after,
            flow.lease_check,
            flow.lease_turn
        ),
        cursors
    );
    assert_eq!(traversal(flow.traversal), before_traversal);
    assert_eq!(flow.leases.len(), leases.len());
    for (id, scope, document, held) in leases {
        let retained = &flow.leases[&id];
        assert_eq!((retained.scope, retained.document), (scope, document));
        assert!(Arc::ptr_eq(&retained.held, &held));
    }
    assert_eq!(calls.get(), 0);
}

#[test]
fn exhausted_inactive_slot_is_not_reclaimed() {
    let (mut flow, root, document, calls) = fixture(1);
    flow.lease_serial = u64::MAX - 1;
    drop(flow.lease(root.id(), document).unwrap());
    assert_eq!((flow.counts().leases, flow.counts().active_leases), (1, 0));
    refusal(&mut flow, root.id(), document, Error::Exhausted, &calls);
}

#[test]
fn exhausted_active_slot_remains_valid() {
    let (mut flow, root, document, calls) = fixture(1);
    flow.lease_serial = u64::MAX - 1;
    let active = flow.lease(root.id(), document).unwrap();
    refusal(&mut flow, root.id(), document, Error::Exhausted, &calls);
    assert!(active.0.load(Ordering::Acquire));
    assert!(flow.held(&flow.leases[&u64::MAX]));
    assert_eq!(flow.close_document(document), Err(Error::InUse));
    assert_eq!(flow.counts().active_leases, 1);
}

#[test]
fn exhausted_mixed_storage_preserves_inactive_and_active_leases() {
    let (mut flow, root, document, calls) = fixture(2);
    flow.lease_serial = u64::MAX - 2;
    drop(flow.lease(root.id(), document).unwrap());
    let active = flow.lease(root.id(), document).unwrap();
    assert_eq!((flow.counts().leases, flow.counts().active_leases), (2, 1));
    refusal(&mut flow, root.id(), document, Error::Exhausted, &calls);
    assert!(active.0.load(Ordering::Acquire));
    assert!(flow.held(&flow.leases[&u64::MAX]));
    assert_eq!(flow.close_document(document), Err(Error::InUse));
}

#[test]
fn capacity_refusal_does_not_consume_final_serial() {
    let (mut flow, root, document, calls) = fixture(1);
    flow.lease_serial = u64::MAX - 2;
    let active = flow.lease(root.id(), document).unwrap();
    refusal(&mut flow, root.id(), document, Error::Capacity, &calls);
    assert!(flow.held(&flow.leases[&(u64::MAX - 1)]));
    assert_eq!(flow.close_document(document), Err(Error::InUse));
    drop(active);
    let replacement = flow.lease(root.id(), document).unwrap();
    assert_eq!(flow.lease_serial, u64::MAX);
    assert_eq!(flow.counts().leases, 1);
    assert!(flow.held(&flow.leases[&u64::MAX]));
    drop(replacement);
    assert_eq!(calls.get(), 0);
}

#[test]
fn exhausted_lease_admission_preserves_stale_invalid_precedence() {
    let (mut flow, root, document, calls) = fixture(1);
    let dead = flow.create_scope(root.id()).unwrap();
    let dead_id = dead.id();
    drop(dead);
    let stale_document = flow.create_document().unwrap();
    flow.close_document(stale_document).unwrap();
    flow.lease_serial = u64::MAX;
    refusal(&mut flow, dead_id, document, Error::Stale, &calls);
    refusal(&mut flow, root.id(), stale_document, Error::Stale, &calls);
    refusal(
        &mut flow,
        document.scope(),
        document,
        Error::Invalid,
        &calls,
    );
    // Foreign scopes/documents are stale even with no serials available.
    let (_foreign_flow, foreign_root, foreign_document, _) = fixture(1);
    refusal(&mut flow, foreign_root.id(), document, Error::Stale, &calls);
    refusal(&mut flow, root.id(), foreign_document, Error::Stale, &calls);
}
