//! Independent F5 review reproductions; desired safety assertions, no production edits.
mod f5_support;
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use f5_support::*;
use rndr_core::editing::SourceDocuments;
use rndr_core::{App, CloseGuard};
use rndr_flow::*;

#[test]
fn replacement_owner_loss_must_refuse_before_commit() {
    let mut f = Fixture::new();
    let old = f.push(1);
    let other = f.new_target();
    let replacement = f.entry_for(f.tab, 2, other);
    let new_id = replacement.id();
    let op = f
        .history
        .sender()
        .submit(Navigation::Replace {
            tab: f.tab,
            old,
            entry: replacement,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
    f.flow.dispose(new_id).unwrap();
    f.choose(op, Choice::Discard).unwrap();
    f.tick();
    eprintln!(
        "replacement-loss phase={:?} selected={:?} new_state={:?} old_state={:?}",
        f.history.phase(op),
        f.history.selected(f.tab),
        f.flow.state(new_id),
        f.flow.state(old)
    );
    assert_ne!(
        f.history.phase(op),
        Some(Phase::Committed),
        "dead replacement cannot replace live original"
    );
    assert_eq!(f.history.entries(f.tab).unwrap()[0].id(), old);
}

#[test]
fn manager_cohort_must_be_transitively_consumable() {
    let mut f = Fixture::new();
    f.push(1);
    let bo = f.flow.register_world().unwrap();
    let bid = bo.id();
    let bs = bo.scope();
    let mut b = App::new();
    bo.bind(b.world_mut()).unwrap();
    b.world_mut().insert_resource(CloseGuard::default());
    let bp = f.flow.create_scope(bs).unwrap();
    let _a_in_b = f.flow.lease(bp.id(), f.target.document).unwrap();
    let mut bd = SourceDocuments::default();
    let bn = bd.open("B document held outside every World");
    b.world_mut().insert_resource(bd);
    let bdoc = f.flow.create_document().unwrap();
    let bt = ProtectedTarget {
        document: bdoc,
        manager: bid,
        native: NativeDocument::Source(bn),
    };
    f.host.register(bt, None).unwrap();
    let _application_lease = f.flow.lease(f.root.id(), bdoc).unwrap();
    let co = f.flow.register_world().unwrap();
    let cid = co.id();
    let mut c = App::new();
    co.bind(c.world_mut()).unwrap();
    c.world_mut().insert_resource(CloseGuard::default());
    let mut ca = runner::CloseSlot {
        requested: true,
        ..Default::default()
    };
    let mut cb = runner::CloseSlot {
        requested: true,
        ..Default::default()
    };
    let mut cc = runner::CloseSlot::default();
    let mut drain = runner::CloseDrain::default();
    assert!(!drain.step([
        (c.world_mut(), &mut cc, true),
        (f.app.world_mut(), &mut ca, false),
        (b.world_mut(), &mut cb, false)
    ]));
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(bid, b.world_mut()).unwrap(),
            HostWorld::new(cid, c.world_mut()).unwrap(),
        ];
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
        for wid in [f.world, bid] {
            let d = f.host.native_decision(wid).unwrap();
            f.host
                .choose_native(&f.flow, &mut worlds, d, Choice::Discard)
                .unwrap();
        }
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    eprintln!(
        "cohort guards A={:?} B={:?}",
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        b.world().resource::<CloseGuard>().unwrap().state()
    );
    assert!(!drain.step([
        (c.world_mut(), &mut cc, true),
        (f.app.world_mut(), &mut ca, false),
        (b.world_mut(), &mut cb, false)
    ]));
    eprintln!(
        "cohort runner closed A={} B={} unrelated_primary={}",
        ca.closed, cb.closed, cc.closed
    );
    assert!(
        !ca.closed,
        "A's native authority must survive while B's shared presentation cannot close"
    );
    assert!(!cb.closed);
}

#[test]
fn omitted_world_on_native_save_choice_must_retain_freeze() {
    let mut f = Fixture::new();
    f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    assert!(f.source().is_readonly());
    let decision = f.host.native_decision(f.world).unwrap();
    let result = f
        .host
        .choose_native(&f.flow, &mut [], decision, Choice::Save);
    eprintln!(
        "omitted-world result={:?} freeze={} decision={:?} native={:?}",
        result,
        f.source().is_readonly(),
        f.host.native_decision(f.world),
        f.app.world().resource::<CloseGuard>().unwrap().state()
    );
    assert_eq!(result, Err(Error::Pending));
    assert!(
        f.source().is_readonly(),
        "failed admission must not drop the existing attempt"
    );
    assert_eq!(f.host.native_decision(f.world), Some(decision));
}

#[test]
fn same_document_replacement_loss_positive_control() {
    let mut f = Fixture::new();
    let old = f.push(1);
    let replacement = f.entry(2);
    let new_id = replacement.id();
    let op = f
        .history
        .sender()
        .submit(Navigation::Replace {
            tab: f.tab,
            old,
            entry: replacement,
        })
        .unwrap();
    f.tick();
    f.flow.dispose(new_id).unwrap();
    assert_eq!(f.choose(op, Choice::Discard), Err(Error::Stale));
    f.tick();
    assert_ne!(f.history.phase(op), Some(Phase::Committed));
    assert_ne!(f.flow.state(old), State::Disposed);
    assert_eq!(f.history.entries(f.tab).unwrap()[0].id(), old);
}

#[test]
fn omitted_world_sync_positive_control() {
    let mut f = Fixture::new();
    f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    let d = f.host.native_decision(f.world).unwrap();
    assert_eq!(f.host.sync_native(&f.flow, &mut []), Err(Error::Pending));
    assert!(f.source().is_readonly());
    assert_eq!(f.host.native_decision(f.world), Some(d));
}

struct DropRoute(Option<Box<dyn FnOnce()>>);
impl Drop for DropRoute {
    fn drop(&mut self) {
        if let Some(callback) = self.0.take() {
            callback();
        }
    }
}
#[test]
fn route_destructor_submission_is_outside_inbox_borrow_and_revalidates() {
    use std::{cell::Cell, rc::Rc};
    let mut f = Fixture::new();
    let tab_owner = f.flow.create_scope(f.world_scope()).unwrap();
    let mut history: Retained<DropRoute, ProtectedTarget, FrozenDocuments> =
        Retained::new(HistoryLimits::default()).unwrap();
    let tab = history.add_tab(&f.flow, f.world, tab_owner).unwrap();
    let owner = f.flow.create_scope(tab).unwrap();
    let old = owner.id();
    let sender = history.sender();
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let submission = Rc::new(Cell::new(None));
    let observed = submission.clone();
    let route = DropRoute(Some(Box::new(move || {
        seen.set(seen.get() + 1);
        observed.set(Some(
            sender
                .submit(Navigation::Close { entries: vec![old] })
                .unwrap(),
        ));
    })));
    history
        .sender()
        .submit(Navigation::Push {
            tab,
            after: None,
            entry: RetainedEntry {
                route,
                document: f.target,
                lease: f.flow.lease(old, f.target.document).unwrap(),
                owner,
            },
        })
        .unwrap();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        history.tick(&mut f.host.pass(&f.flow, &mut worlds));
    }
    let owner = f.flow.create_scope(tab).unwrap();
    let op = history
        .sender()
        .submit(Navigation::Replace {
            tab,
            old,
            entry: RetainedEntry {
                route: DropRoute(None),
                document: f.target,
                lease: f.flow.lease(owner.id(), f.target.document).unwrap(),
                owner,
            },
        })
        .unwrap();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        let mut pass = f.host.pass(&f.flow, &mut worlds);
        history.tick(&mut pass);
        history
            .choose(&mut pass, history.decision(op).unwrap(), Choice::Discard)
            .unwrap();
        history.tick(&mut pass);
        assert_eq!(history.phase(op), Some(Phase::Committed));
        assert_eq!(count.get(), 1);
        let nested = submission.get().unwrap();
        assert_eq!(history.phase(nested), Some(Phase::Queued));
        history.tick(&mut pass);
        assert_eq!(history.phase(nested), Some(Phase::Failed));
        assert_eq!(count.get(), 1);
    }
}
