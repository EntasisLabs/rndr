//! All-World native protection through the *actual renderer-used* boundary.
//! This is deterministic headless coverage, not a native event-loop run.
mod f5_support;
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use f5_support::Fixture;
use rndr_core::{editing::SourceDocuments, App, CloseGuard, Input, WindowHub, World};
use rndr_flow::*;
use runner::{CloseDrain, CloseOutcome, CloseParticipant, CloseSlot, CloseWorld};
use std::{cell::RefCell, rc::Rc};
struct Node {
    app: App,
    close: CloseSlot,
    primary: bool,
    id: rndr_core::WindowId,
    index: usize,
}
impl CloseParticipant for Node {
    fn parts(&mut self) -> (&mut World, &mut CloseSlot, bool, rndr_core::WindowId) {
        (self.app.world_mut(), &mut self.close, self.primary, self.id)
    }
}
fn node(hub: &WindowHub, app: App, index: usize, primary: bool, requested: bool) -> Node {
    Node {
        app,
        close: CloseSlot {
            requested,
            ..Default::default()
        },
        primary,
        id: hub.attach().id(),
        index,
    }
}
fn bridge<'a>(worlds: &'a mut [CloseWorld<'_>]) -> Vec<HostWorld<'a>> {
    worlds
        .iter_mut()
        .map(|w| {
            let id = w.world().local::<WorldOwner>().unwrap().id();
            HostWorld::new(id, w.world_mut()).unwrap()
        })
        .collect()
}
#[test]
fn real_boundary_native_fixed_point_matrix() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for mode in 0..5 {
            for exit_path in 0..3 {
                // mode 0 free chain, 1 blocked chain, 2 free cycle, 3 blocked
                // cycle, 4 participant withdrawal in a hidden callback.
                // exit_path 0 independent secondary, 1 primary, 2 secondary quit.
                let mut f = Fixture::new();
                f.push(1);
                let hub = WindowHub::new();
                let mut nodes = vec![node(&hub, f.app, 0, false, true)];
                let mut targets = vec![f.target];
                for i in 1..3 {
                    let owner = f.flow.register_world().unwrap();
                    let wid = owner.id();
                    let mut app = App::new();
                    owner.bind(app.world_mut()).unwrap();
                    app.world_mut().insert_resource(CloseGuard::default());
                    let mut docs = SourceDocuments::default();
                    let native = docs.open("other manager");
                    app.world_mut().insert_resource(docs);
                    let target = ProtectedTarget {
                        document: f.flow.create_document().unwrap(),
                        manager: wid,
                        native: NativeDocument::Source(native),
                    };
                    f.host.register(target, None).unwrap();
                    targets.push(target);
                    nodes.push(node(&hub, app, i, false, true));
                }
                let mut leases = Vec::new();
                for (p, m) in [(1, 0), (2, 1)]
                    .into_iter()
                    .chain((mode == 2 || mode == 3).then_some((0, 2)))
                {
                    let scope = nodes[p].app.world().local::<WorldOwner>().unwrap().scope();
                    leases.push(f.flow.lease(scope, targets[m].document).unwrap());
                }
                let _outside = if mode == 1 || mode == 3 {
                    Some(f.flow.lease(f.root.id(), targets[2].document).unwrap())
                } else {
                    None
                };
                let owner = f.flow.register_world().unwrap();
                let mut primary = App::new();
                owner.bind(primary.world_mut()).unwrap();
                primary.world_mut().insert_resource(CloseGuard::default());
                nodes.sort_by_key(|n| order.iter().position(|i| *i == n.index).unwrap());
                nodes.push(node(&hub, primary, 3, true, exit_path == 1));
                if exit_path == 2 {
                    nodes[0]
                        .app
                        .world_mut()
                        .resource_mut::<Input>()
                        .unwrap()
                        .quit = true;
                }
                let state = Rc::new(RefCell::new((f.flow, f.host)));
                let retained = state.clone();
                let mut hook = move |worlds: &mut [CloseWorld<'_>]| {
                    assert_eq!(worlds.len(), 4);
                    for w in worlds.iter() {
                        assert_eq!(w.closing(), exit_path != 0 || !w.primary());
                    }
                    let ids: Vec<_> = worlds
                        .iter()
                        .map(|w| w.world().local::<WorldOwner>().unwrap().id())
                        .collect();
                    let mut worlds = bridge(worlds);
                    let mut state = retained.borrow_mut();
                    let (flow, host) = &mut *state;
                    host.sync_native(flow, &mut worlds)
                        .map_err(|e| format!("{e:?}"))?;
                    for id in ids {
                        if let Some(d) = host.native_decision(id) {
                            host.choose_native(flow, &mut worlds, d, Choice::Discard)
                                .map_err(|e| format!("{e:?}"))?;
                        }
                    }
                    host.sync_native(flow, &mut worlds)
                        .map_err(|e| format!("{e:?}"))
                };
                let mut drain = CloseDrain::default();
                // mode4 cancels last member before final validation, so the
                // expected actual membership has changed. Use separate hook below.
                if mode == 4 {
                    let retained = state.clone();
                    let mut withdrawn = move |worlds: &mut [CloseWorld<'_>]| {
                        let ids: Vec<_> = worlds
                            .iter()
                            .map(|w| w.world().local::<WorldOwner>().unwrap().id())
                            .collect();
                        let mut worlds = bridge(worlds);
                        let mut s = retained.borrow_mut();
                        let (flow, host) = &mut *s;
                        host.sync_native(flow, &mut worlds)
                            .map_err(|e| format!("{e:?}"))?;
                        for id in ids {
                            if let Some(d) = host.native_decision(id) {
                                host.choose_native(flow, &mut worlds, d, Choice::Discard)
                                    .map_err(|e| format!("{e:?}"))?;
                            }
                        }
                        host.sync_native(flow, &mut worlds)
                            .map_err(|e| format!("{e:?}"))
                    };
                    let outcome = drain.reconcile(
                        &mut nodes,
                        |n| {
                            if n.index == 2 {
                                n.app
                                    .world_mut()
                                    .resource_mut::<CloseGuard>()
                                    .unwrap()
                                    .cancel();
                            }
                        },
                        Some(&mut withdrawn),
                    );
                    assert_ne!(outcome, CloseOutcome::Exit);
                    assert!(nodes.iter().all(|n| !n.close.closed));
                } else {
                    let outcome = drain.reconcile(&mut nodes, |_| {}, Some(&mut hook));
                    let safe = mode == 0 || mode == 2;
                    assert_eq!(outcome == CloseOutcome::Exit, exit_path != 0 && safe);
                    for n in &nodes {
                        assert_eq!(n.close.closed, exit_path == 0 && safe && n.index != 3);
                    }
                }
                eprintln!(
                    "F5-SEAM matrix order={order:?} mode={mode} exit_path={exit_path} safety=true"
                );
                drop(leases);
            }
        }
    }
}
#[test]
fn hook_error_or_panic_keeps_real_document_freeze_and_exact_decision() {
    for panic_hook in [false, true] {
        let mut f = Fixture::new();
        f.push(1);
        let hub = WindowHub::new();
        let mut nodes = vec![node(&hub, f.app, 0, false, true)];
        let state = Rc::new(RefCell::new((f.flow, f.host)));
        let retained = state.clone();
        let mut hook = move |worlds: &mut [CloseWorld<'_>]| {
            let mut worlds = bridge(worlds);
            let mut s = retained.borrow_mut();
            let (flow, host) = &mut *s;
            host.sync_native(flow, &mut worlds).unwrap();
            if panic_hook {
                panic!("after real freeze");
            }
            Err("after real freeze".into())
        };
        let mut drain = CloseDrain::default();
        assert_eq!(
            drain.reconcile(&mut nodes, |_| {}, Some(&mut hook)),
            CloseOutcome::Continue
        );
        let decision = state.borrow().1.native_decision(f.world).unwrap();
        assert!(!nodes[0].close.closed);
        let docs = nodes[0].app.world().resource::<SourceDocuments>().unwrap();
        let NativeDocument::Source(native) = f.target.native else {
            unreachable!()
        };
        assert!(docs.get(native).unwrap().is_readonly());
        let mut fail = |_: &mut [CloseWorld<'_>]| Err("again".into());
        assert_eq!(
            drain.reconcile(&mut nodes, |_| panic!("no new frame"), Some(&mut fail)),
            CloseOutcome::Continue
        );
        assert_eq!(state.borrow().1.native_decision(f.world), Some(decision));
    }
}
