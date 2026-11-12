use super::*;
use crate::{Coordinator, DocumentId, Limits};
struct Host;
impl TransitionHost<DocumentId> for Host {
    type Guard = bool;
    fn protect(&mut self, _: OperationId, t: &[(ScopeId, DocumentId)]) -> Result<bool, Error> {
        Ok(!t.is_empty())
    }
    fn dirty(&self, g: &bool) -> bool {
        *g
    }
    fn choose(&mut self, _: &mut bool, _: DecisionId, c: Choice) -> Result<Phase, Error> {
        Ok(if c == Choice::Discard {
            Phase::Ready
        } else {
            Phase::Saving
        })
    }
    fn poll(&mut self, _: &mut bool) -> Phase {
        Phase::Saving
    }
    fn validate(&self, _: &bool) -> Result<(), Error> {
        Ok(())
    }
}
#[test]
fn counter_max_capacity_and_total_pending_refuse_atomically() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let world = flow.register_world().unwrap();
    let tab_owner = flow.create_scope(world.scope()).unwrap();
    let mut h: Retained<u8, DocumentId, bool> = Retained::new(HistoryLimits {
        tabs: 1,
        entries: 1,
        operations: 1,
        outcomes: 1,
    })
    .unwrap();
    let tab = h.add_tab(&flow, world.id(), tab_owner).unwrap();
    let doc = flow.create_document().unwrap();
    let owner = flow.create_scope(tab).unwrap();
    let lease = flow.lease(owner.id(), doc).unwrap();
    let entry = RetainedEntry {
        route: 1,
        document: doc,
        owner,
        lease,
    };
    let id = entry.id();
    let sender = h.sender();
    let queued = sender
        .submit(Navigation::Push {
            tab,
            after: None,
            entry,
        })
        .unwrap();
    assert_eq!(h.phase(queued), Some(Phase::Queued));
    h.tick(&mut Host);
    let before = h.counts();
    sender.0.serial.set(u64::MAX);
    assert_eq!(
        sender
            .submit(Navigation::Select { tab, entry: id })
            .unwrap_err()
            .reason,
        Error::Exhausted
    );
    assert_eq!(h.counts(), before);
    sender.0.serial.set(1);
    let close = sender
        .submit(Navigation::Close { entries: vec![id] })
        .unwrap();
    h.tick(&mut Host);
    assert_eq!(h.phase(close), Some(Phase::AwaitingDecision));
    let before = h.counts();
    let serial = sender.0.serial.get();
    assert_eq!(
        sender
            .submit(Navigation::Select { tab, entry: id })
            .unwrap_err()
            .reason,
        Error::Capacity
    );
    assert_eq!(sender.0.serial.get(), serial);
    assert_eq!(h.counts(), before);
    assert_eq!(
        sender
            .submit(Navigation::Close { entries: vec![id] })
            .unwrap(),
        close
    );
    h.cancel(close).unwrap();
    h.tick(&mut Host);
    let before = h.entries(tab).unwrap()[0].id();
    let owner = flow.create_scope(tab).unwrap();
    let lease = flow.lease(owner.id(), doc).unwrap();
    let rejected = sender
        .submit(Navigation::Push {
            tab,
            after: Some(id),
            entry: RetainedEntry {
                route: 2,
                document: doc,
                owner,
                lease,
            },
        })
        .unwrap();
    h.tick(&mut Host);
    assert_eq!(h.phase(rejected), Some(Phase::Failed));
    assert_eq!(h.entries(tab).unwrap().len(), 1);
    assert_eq!(h.entries(tab).unwrap()[0].id(), before);
    assert_eq!(h.selected(tab).unwrap(), Some(id));
    assert_eq!(h.counts().outcomes, 1);
    drop(h);
    assert_eq!(
        sender
            .submit(Navigation::Select { tab, entry: id })
            .unwrap_err()
            .reason,
        Error::Stale
    );
}
#[test]
fn decision_round_max_preserves_owned_guard_and_current_choice() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let world = flow.register_world().unwrap();
    let tab_owner = flow.create_scope(world.scope()).unwrap();
    let mut h: Retained<u8, DocumentId, bool> = Retained::new(HistoryLimits::default()).unwrap();
    let tab = h.add_tab(&flow, world.id(), tab_owner).unwrap();
    let doc = flow.create_document().unwrap();
    let owner = flow.create_scope(tab).unwrap();
    let lease = flow.lease(owner.id(), doc).unwrap();
    let id = owner.id();
    h.sender()
        .submit(Navigation::Push {
            tab,
            after: None,
            entry: RetainedEntry {
                route: 1,
                document: doc,
                owner,
                lease,
            },
        })
        .unwrap();
    h.tick(&mut Host);
    let op = h
        .sender()
        .submit(Navigation::Close { entries: vec![id] })
        .unwrap();
    h.tick(&mut Host);
    h.pending[0].round = u64::MAX;
    let d = h.decision(op).unwrap();
    let before = h.counts();
    assert_eq!(
        h.choose(&mut Host, d, Choice::Discard),
        Err(Error::Exhausted)
    );
    assert_eq!(h.phase(op), Some(Phase::AwaitingDecision));
    assert_eq!(h.decision(op), Some(d));
    assert_eq!(h.counts(), before);
    h.cancel(op).unwrap();
    h.tick(&mut Host);
    assert_eq!(h.phase(op), Some(Phase::Cancelled));
}
