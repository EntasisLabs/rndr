mod f5_support;
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use f5_support::*;
use rndr_core::editing::SourceDocuments;
use rndr_core::{App, CloseGuard, CloseState};
use rndr_flow::*;

#[test]
fn manager_fixed_point_chain_cycle_order_and_withdrawal() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        // 0: chain blocked at its tail; 1: legitimate cycle; 2: cycle withdrawal;
        // 3: cycle blocked by application lease; 4: free chain.
        for mode in 0..5 {
            let mut f = Fixture::new();
            f.push(1);
            let mut nodes = vec![(
                f.world,
                f.app,
                f.target,
                runner::CloseSlot {
                    requested: true,
                    ..Default::default()
                },
            )];
            for _ in 0..2 {
                let owner = f.flow.register_world().unwrap();
                let id = owner.id();
                let mut app = App::new();
                owner.bind(app.world_mut()).unwrap();
                app.world_mut().insert_resource(CloseGuard::default());
                let mut docs = SourceDocuments::default();
                let native = docs.open("manager document");
                app.world_mut().insert_resource(docs);
                let target = ProtectedTarget {
                    document: f.flow.create_document().unwrap(),
                    manager: id,
                    native: NativeDocument::Source(native),
                };
                f.host.register(target, None).unwrap();
                nodes.push((
                    id,
                    app,
                    target,
                    runner::CloseSlot {
                        requested: true,
                        ..Default::default()
                    },
                ));
            }
            let mut leases = vec![];
            // B presents A; D presents B. Cycle adds A presenting D.
            for (presenter, manager) in [(1, 0), (2, 1)]
                .into_iter()
                .chain((mode == 1 || mode == 2 || mode == 3).then_some((0, 2)))
            {
                let scope = nodes[presenter]
                    .1
                    .world()
                    .local::<WorldOwner>()
                    .unwrap()
                    .scope();
                leases.push(f.flow.lease(scope, nodes[manager].2.document).unwrap());
            }
            let application = if mode == 0 || mode == 3 {
                Some(f.flow.lease(f.root.id(), nodes[2].2.document).unwrap())
            } else {
                None
            };
            let primary_owner = f.flow.register_world().unwrap();
            let primary_id = primary_owner.id();
            let mut primary = App::new();
            primary_owner.bind(primary.world_mut()).unwrap();
            primary.world_mut().insert_resource(CloseGuard::default());
            let mut pc = runner::CloseSlot::default();
            // Reorder actual World iteration, not just a synthetic dependency list.
            // Apply the permutation explicitly by stable original indices below.
            let mut indexed: Vec<_> = nodes.into_iter().enumerate().collect();
            indexed.sort_by_key(|(i, _)| order.iter().position(|j| j == i).unwrap());
            let mut nodes: Vec<_> = indexed.into_iter().map(|(_, n)| n).collect();
            let mut drain = runner::CloseDrain::default();
            assert!(!drain.step(
                nodes
                    .iter_mut()
                    .map(|n| (n.1.world_mut(), &mut n.3, false))
                    .chain(std::iter::once((primary.world_mut(), &mut pc, true)))
            ));
            {
                let mut worlds: Vec<_> = nodes
                    .iter_mut()
                    .map(|n| HostWorld::new(n.0, n.1.world_mut()).unwrap())
                    .collect();
                worlds.push(HostWorld::new(primary_id, primary.world_mut()).unwrap());
                f.host.sync_native(&f.flow, &mut worlds).unwrap();
            }
            // Obtain exact decisions by node IDs, then supply all Worlds again.
            let ids: Vec<_> = nodes.iter().map(|n| n.0).collect();
            {
                let mut worlds: Vec<_> = nodes
                    .iter_mut()
                    .map(|n| HostWorld::new(n.0, n.1.world_mut()).unwrap())
                    .collect();
                worlds.push(HostWorld::new(primary_id, primary.world_mut()).unwrap());
                for id in &ids {
                    let d = f.host.native_decision(*id).unwrap();
                    f.host
                        .choose_native(&f.flow, &mut worlds, d, Choice::Discard)
                        .unwrap();
                }
                f.host.sync_native(&f.flow, &mut worlds).unwrap();
            }
            if mode == 2 {
                // Withdraw one member after all guards were Ready. Every manager
                // in the cycle must be revoked before real secondary consumption.
                nodes[0]
                    .1
                    .world_mut()
                    .resource_mut::<CloseGuard>()
                    .unwrap()
                    .cancel();
                let mut worlds: Vec<_> = nodes
                    .iter_mut()
                    .map(|n| HostWorld::new(n.0, n.1.world_mut()).unwrap())
                    .collect();
                worlds.push(HostWorld::new(primary_id, primary.world_mut()).unwrap());
                f.host.sync_native(&f.flow, &mut worlds).unwrap();
            }
            let expected = mode == 1 || mode == 4;
            assert!(!drain.step(
                nodes
                    .iter_mut()
                    .map(|n| (n.1.world_mut(), &mut n.3, false))
                    .chain(std::iter::once((primary.world_mut(), &mut pc, true)))
            ));
            assert!(
                nodes.iter().all(|n| n.3.closed == expected),
                "mode={mode} order={order:?}"
            );
            assert!(!pc.closed);
            drop(application);
            drop(leases);
        }
    }
}

#[test]
fn incoming_push_replace_loss_during_decision_and_save_is_atomic() {
    for push in [false, true] {
        for save in [false, true] {
            let mut f = Fixture::new();
            let first = f.push(1);
            let old = f.push(2);
            let target = f.new_target();
            let entry = f.entry_for(f.tab, 3, target);
            let incoming = entry.id();
            let command = if push {
                Navigation::Push {
                    tab: f.tab,
                    after: Some(first),
                    entry,
                }
            } else {
                Navigation::Replace {
                    tab: f.tab,
                    old,
                    entry,
                }
            };
            let op = f.history.sender().submit(command).unwrap();
            f.tick();
            assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
            if save {
                f.choose(op, Choice::Save).unwrap();
                let tickets = f.history.guard(op).unwrap().saves();
                f.flow.dispose(incoming).unwrap();
                for t in tickets {
                    f.save(op, t, |_, _| Ok(())).unwrap();
                }
            } else {
                f.flow.dispose(incoming).unwrap();
                f.choose(op, Choice::Discard).unwrap();
            }
            f.tick();
            assert_eq!(f.history.phase(op), Some(Phase::Failed));
            assert_eq!(
                f.history
                    .entries(f.tab)
                    .unwrap()
                    .iter()
                    .map(|e| e.id())
                    .collect::<Vec<_>>(),
                vec![first, old]
            );
            assert_eq!(f.history.selected(f.tab).unwrap(), Some(old));
            assert_ne!(f.flow.state(old), State::Disposed);
        }
    }
}

#[test]
fn incoming_exact_lease_document_and_native_authority_are_required() {
    for wrong in 0..4 {
        let mut f = Fixture::new();
        let old = f.push(1);
        let other = f.new_target();
        let mut entry = f.entry_for(f.tab, 2, other);
        match wrong {
            0 => entry.lease = f.flow.lease(f.root.id(), other.document).unwrap(),
            1 => entry.lease = f.flow.lease(entry.id(), f.target.document).unwrap(),
            2 => {
                entry.document.native = f.target.native;
            }
            _ => {
                // Simulate unavoidable loss of the incoming native store.
                f.app
                    .world_mut()
                    .insert_resource(SourceDocuments::default());
            }
        }
        let op = f
            .history
            .sender()
            .submit(Navigation::Replace {
                tab: f.tab,
                old,
                entry,
            })
            .unwrap();
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Failed));
        assert_eq!(f.history.entries(f.tab).unwrap()[0].id(), old);
        assert_ne!(f.flow.state(old), State::Disposed);
    }
}

#[test]
fn native_refusals_and_technical_failure_keep_owned_protection() {
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
    for choice in [Choice::Save, Choice::Discard, Choice::Cancel] {
        assert_eq!(
            f.host.choose_native(&f.flow, &mut [], d, choice),
            Err(Error::Pending)
        );
        assert!(f.source().is_readonly());
        assert_eq!(f.host.native_decision(f.world), Some(d));
    }
    // A valid but unrelated coordinator/World cannot stand in for the missing one.
    let mut foreign = Fixture::new();
    {
        let mut w = [HostWorld::new(foreign.world, foreign.app.world_mut()).unwrap()];
        assert_eq!(
            f.host.choose_native(&f.flow, &mut w, d, Choice::Save),
            Err(Error::Pending)
        );
    }
    assert_eq!(f.host.native_decision(f.world), Some(d));
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        assert_eq!(
            f.host.choose_native(&foreign.flow, &mut w, d, Choice::Save),
            Err(Error::Stale)
        );
        assert_eq!(f.host.native_decision(f.world), Some(d));
        f.host
            .choose_native(&f.flow, &mut w, d, Choice::Save)
            .unwrap();
    }
    let t = f.host.native_saves()[0];
    assert_eq!(
        f.host
            .save_native(&f.flow, &mut [], t, |_, _| panic!("must not run")),
        Err(Error::Pending)
    );
    assert!(f.source().is_readonly());
    {
        let mut w = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        assert_eq!(
            f.host.choose_native(&f.flow, &mut w, d, Choice::Save),
            Err(Error::Stale)
        );
        f.host
            .save_native(&f.flow, &mut w, t, |_, _| Err(SaveFailure::Failed))
            .unwrap();
        f.host.sync_native(&f.flow, &mut w).unwrap();
    }
    assert!(f.source().is_readonly());
    let retry = f.host.native_decision(f.world).unwrap();
    assert_eq!(
        f.host.choose_native(&f.flow, &mut [], retry, Choice::Save),
        Err(Error::Pending)
    );
    assert_eq!(f.host.native_decision(f.world), Some(retry));
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Failed(..)
    ));
}
