mod f5_support;
use f5_support::*;
use rndr_core::{CloseGuard, CloseState};
use rndr_flow::*;

#[test]
fn incoming_manager_loss_after_decision_preserves_original() {
    let mut f = Fixture::new();
    let old = f.push(1);
    let owner = f.flow.register_world().unwrap();
    let manager = owner.id();
    let mut other = rndr_core::App::new();
    owner.bind(other.world_mut()).unwrap();
    let mut docs = rndr_core::editing::SourceDocuments::default();
    let native = docs.open("incoming manager");
    other.world_mut().insert_resource(docs);
    let target = ProtectedTarget {
        document: f.flow.create_document().unwrap(),
        manager,
        native: NativeDocument::Source(native),
    };
    f.host.register(target, None).unwrap();
    let entry = f.entry_for(f.tab, 2, target);
    let op = f
        .history
        .sender()
        .submit(Navigation::Replace {
            tab: f.tab,
            old,
            entry,
        })
        .unwrap();
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(manager, other.world_mut()).unwrap(),
        ];
        f.history.tick(&mut f.host.pass(&f.flow, &mut worlds));
    }
    assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
    drop(other);
    f.choose(op, Choice::Discard).unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Failed));
    assert_eq!(f.history.entries(f.tab).unwrap()[0].id(), old);
    assert_ne!(f.flow.state(old), State::Disposed);
}

#[test]
fn queued_incoming_loss_and_dead_selection_do_not_mutate_history() {
    let mut f = Fixture::new();
    let old = f.push(1);
    let entry = f.entry(2);
    let incoming = entry.id();
    let block = f.close(vec![old]);
    let queued = f
        .history
        .sender()
        .submit(Navigation::Push {
            tab: f.tab,
            after: Some(old),
            entry,
        })
        .unwrap();
    f.tick();
    assert_eq!(f.history.phase(queued), Some(Phase::Queued));
    f.flow.dispose(incoming).unwrap();
    f.history.cancel(block).unwrap();
    f.tick();
    assert_ne!(f.history.phase(queued), Some(Phase::Committed));
    assert_eq!(f.history.entries(f.tab).unwrap()[0].id(), old);
    assert_ne!(f.flow.state(old), State::Disposed);
    let second = f.push(3);
    f.flow.dispose(old).unwrap();
    let select = f
        .history
        .sender()
        .submit(Navigation::Select {
            tab: f.tab,
            entry: old,
        })
        .unwrap();
    f.tick();
    assert_ne!(f.history.phase(select), Some(Phase::Committed));
    assert_eq!(f.history.selected(f.tab).unwrap(), Some(second));
}

#[test]
fn native_request_change_and_save_panic_never_cancel_freeze() {
    let mut f = Fixture::new();
    f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut w).unwrap();
    }
    let d = f.host.native_decision(f.world).unwrap();
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .invalidate();
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        for c in [Choice::Save, Choice::Discard, Choice::Cancel] {
            assert_eq!(
                f.host.choose_native(&f.flow, &mut w, d, c),
                Err(Error::Stale)
            );
        }
    }
    assert_eq!(f.host.native_decision(f.world), Some(d));
    assert!(f.source().is_readonly());
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut w).unwrap();
    }
    let current = f.host.native_decision(f.world).unwrap();
    assert_ne!(current, d);
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .choose_native(&f.flow, &mut w, current, Choice::Save)
            .unwrap();
    }
    let t = f.host.native_saves()[0];
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .save_native(&f.flow, &mut w, t, |_, _| panic!("technical save failure"))
            .unwrap();
        f.host.sync_native(&f.flow, &mut w).unwrap();
    }
    assert!(f.source().is_readonly());
    assert!(f.host.native_decision(f.world).is_some());
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Failed(..)
    ));
}
