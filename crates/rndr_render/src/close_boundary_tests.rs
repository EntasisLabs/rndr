//! Headless tests of the same reconcile helper used by Running on both paths.
//! No native event loop, surface, OS input or GUI certification is implied.
use super::close::{CloseDrain, CloseOutcome, CloseParticipant, CloseSlot};
use super::{CloseWorld, Session};
use rndr_core::{App, CloseGuard, CloseState, Input, WindowHub, WindowId, World};
use std::{cell::RefCell, rc::Rc};

struct Slot {
    app: App,
    close: CloseSlot,
    primary: bool,
    id: WindowId,
}
impl CloseParticipant for Slot {
    fn parts(&mut self) -> (&mut World, &mut CloseSlot, bool, WindowId) {
        (self.app.world_mut(), &mut self.close, self.primary, self.id)
    }
}
fn slot(hub: &WindowHub, primary: bool, guarded: bool) -> Slot {
    let desktop = hub.attach();
    let id = desktop.id();
    let mut app = App::new();
    app.world_mut().insert_resource(desktop);
    if guarded {
        app.world_mut().insert_resource(CloseGuard::default());
    }
    Slot {
        app,
        close: CloseSlot::default(),
        primary,
        id,
    }
}
fn ready(world: &mut World) {
    if let Some(g) = world.resource_mut::<CloseGuard>() {
        let r = g.request();
        assert!(g.complete(r, Ok(())));
    }
}

#[test]
fn public_session_hook_covers_every_live_world_and_actual_intent() {
    for mode in 0..3 {
        // primary close, secondary close, quit from secondary
        let hub = WindowHub::new();
        let mut slots = vec![
            slot(&hub, true, true),
            slot(&hub, false, true),
            slot(&hub, false, false),
        ];
        if mode == 2 {
            slots[1]
                .app
                .world_mut()
                .resource_mut::<Input>()
                .unwrap()
                .quit = true;
        } else {
            slots[usize::from(mode == 1)].close.requested = true;
        }
        let ids: Vec<_> = slots.iter().map(|s| s.id).collect();
        let seen = Rc::new(RefCell::new(Vec::new()));
        let calls = seen.clone();
        // This is the actual public Session builder, not a flow-only adapter.
        let mut session =
            Session::new(App::new())
                .window(App::new())
                .close_validator(move |worlds| {
                    assert_eq!(worlds.len(), 3);
                    for (i, w) in worlds.iter_mut().enumerate() {
                        assert_eq!(w.id(), ids[i]);
                        assert_eq!(w.primary(), i == 0);
                        assert_eq!(w.closing(), mode != 1 || i == 1);
                        if w.closing() {
                            ready(w.world_mut());
                        }
                        calls.borrow_mut().push(i);
                    }
                    Ok(())
                });
        let mut d = CloseDrain::default();
        let outcome = d.reconcile(&mut slots, |_| {}, session.close_validator.as_deref_mut());
        assert_eq!(*seen.borrow(), vec![0, 1, 2]);
        assert_eq!(
            outcome,
            if mode == 1 {
                CloseOutcome::Continue
            } else {
                CloseOutcome::Exit
            }
        );
        assert_eq!(slots[1].close.closed, mode == 1);
        assert!(!slots[0].close.closed && !slots[2].close.closed);
    }
}

#[test]
fn hidden_callback_precedes_final_validation_and_never_follows_consumption() {
    for primary in [false, true] {
        for invalidate in [false, true] {
            let hub = WindowHub::new();
            let mut slots = vec![slot(&hub, primary, true)];
            slots[0].close.requested = true;
            let order = Rc::new(RefCell::new(Vec::new()));
            let observe = order.clone();
            let mut session = Session::new(App::new()).close_validator(move |worlds| {
                observe.borrow_mut().push("validate");
                assert_eq!(
                    worlds[0].world().resource::<String>().unwrap(),
                    "hidden mutation"
                );
                let g = worlds[0].world().resource::<CloseGuard>().unwrap();
                if invalidate {
                    assert!(matches!(g.state(), CloseState::Pending(_)));
                } else {
                    assert!(matches!(g.state(), CloseState::Ready(_)));
                }
                Ok(())
            });
            let frame = order.clone();
            let mut d = CloseDrain::default();
            let outcome = d.reconcile(
                &mut slots,
                |s| {
                    frame.borrow_mut().push("hidden");
                    ready(s.app.world_mut());
                    s.app
                        .world_mut()
                        .insert_resource("hidden mutation".to_string());
                    if invalidate {
                        s.app
                            .world_mut()
                            .resource_mut::<CloseGuard>()
                            .unwrap()
                            .invalidate();
                    }
                },
                session.close_validator.as_deref_mut(),
            );
            assert_eq!(*order.borrow(), vec!["hidden", "validate"]);
            assert_eq!(outcome == CloseOutcome::Exit, primary && !invalidate);
            assert_eq!(slots[0].close.closed, !primary && !invalidate);
            // No application callback is used after this point by the helper.
            if !primary && !invalidate {
                slots.retain(|s| !s.close.closed);
                assert!(slots.is_empty());
            }
        }
    }
}

#[test]
fn errors_and_panics_refuse_whole_pass_without_cancelling_or_losing_text() {
    for primary in [false, true] {
        for panic_hook in [false, true] {
            let hub = WindowHub::new();
            let mut slots = vec![slot(&hub, primary, true), slot(&hub, false, true)];
            for s in &mut slots {
                s.close.requested = true;
                s.app
                    .world_mut()
                    .insert_resource("exact draft α".to_string());
            }
            let mut session = Session::new(App::new()).close_validator(move |worlds| {
                for w in worlds {
                    ready(w.world_mut());
                }
                if panic_hook {
                    panic!("validator injected failure");
                }
                Err("validation failed".into())
            });
            let mut d = CloseDrain::default();
            assert_eq!(
                d.reconcile(&mut slots, |_| {}, session.close_validator.as_deref_mut()),
                CloseOutcome::Continue
            );
            for s in &slots {
                assert!(!s.close.closed && s.close.active);
                assert_eq!(s.app.world().resource::<String>().unwrap(), "exact draft α");
                assert!(matches!(
                    s.app.world().resource::<CloseGuard>().unwrap().state(),
                    CloseState::Ready(_)
                ));
            }
            // Retry the same owned requests, no implied cancel or discard.
            let mut success = |_: &mut [CloseWorld<'_>]| Ok(());
            let result = d.reconcile(
                &mut slots,
                |_| panic!("no repeated frames"),
                Some(&mut success),
            );
            assert_eq!(result == CloseOutcome::Exit, primary);
            assert_eq!(slots.iter().all(|s| s.close.closed), !primary);
        }
    }
}

#[test]
fn fabricated_participation_is_rejected_before_and_after_hook() {
    for inside in [false, true] {
        let hub = WindowHub::new();
        let mut slots = vec![slot(&hub, false, true), slot(&hub, false, true)];
        slots[0].close.requested = true;
        if !inside {
            ready(slots[1].app.world_mut());
        }
        let called = Rc::new(RefCell::new(0));
        let seen = called.clone();
        let mut session = Session::new(App::new()).close_validator(move |worlds| {
            *seen.borrow_mut() += 1;
            assert!(worlds[0].closing() && !worlds[1].closing());
            ready(worlds[0].world_mut());
            ready(worlds[1].world_mut()); // cannot manufacture native participation
            Ok(())
        });
        let mut d = CloseDrain::default();
        assert_eq!(
            d.reconcile(&mut slots, |_| {}, session.close_validator.as_deref_mut()),
            CloseOutcome::Continue
        );
        assert_eq!(*called.borrow(), usize::from(inside));
        assert!(slots.iter().all(|s| !s.close.closed));
        assert!(!slots[1].close.active);
    }
}

#[test]
fn new_quit_or_cancel_in_callback_settles_or_defers_before_validation() {
    for quit in [false, true] {
        let hub = WindowHub::new();
        let mut slots = vec![slot(&hub, true, true), slot(&hub, false, true)];
        slots[1].close.requested = true;
        let mut d = CloseDrain::default();
        let mut never = |_: &mut [CloseWorld<'_>]| -> Result<(), String> {
            panic!("must defer new cohort");
        };
        let outcome = d.reconcile(
            &mut slots,
            |s| {
                if quit {
                    s.app.world_mut().resource_mut::<Input>().unwrap().quit = true;
                } else {
                    s.app
                        .world_mut()
                        .resource_mut::<CloseGuard>()
                        .unwrap()
                        .cancel();
                }
            },
            if quit { Some(&mut never) } else { None },
        );
        assert!(slots.iter().all(|s| !s.close.closed));
        if quit {
            assert_eq!(outcome, CloseOutcome::Deferred);
            assert!(slots.iter().all(|s| s.close.active));
        } else {
            assert_eq!(outcome, CloseOutcome::Continue);
            assert!(!slots[1].close.active);
        }
    }
}

#[test]
fn no_hook_compatibility_and_secondary_withdrawal_are_not_sticky() {
    for primary in [false, true] {
        for guarded in [false, true] {
            let hub = WindowHub::new();
            let mut slots = vec![slot(&hub, primary, guarded)];
            slots[0].close.requested = true;
            let mut d = CloseDrain::default();
            assert_eq!(
                d.reconcile(&mut slots, |_| {}, None) == CloseOutcome::Exit,
                primary && !guarded
            );
            assert_eq!(slots[0].close.closed, !primary && !guarded);
            if guarded {
                ready(slots[0].app.world_mut());
                assert_eq!(
                    d.reconcile(&mut slots, |_| {}, None) == CloseOutcome::Exit,
                    primary
                );
                assert_eq!(slots[0].close.closed, !primary);
                slots[0]
                    .app
                    .world_mut()
                    .resource_mut::<CloseGuard>()
                    .unwrap()
                    .invalidate();
                assert_eq!(
                    d.reconcile(&mut slots, |_| {}, None),
                    CloseOutcome::Continue
                );
                assert!(!slots[0].close.closed);
            }
        }
    }
}

#[test]
fn validator_panic_payload_destructor_is_also_contained() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Payload(Arc<AtomicUsize>);
    impl Drop for Payload {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
            panic!("payload destructor panic");
        }
    }
    for primary in [false, true] {
        let hub = WindowHub::new();
        let mut slots = vec![slot(&hub, primary, true)];
        slots[0].close.requested = true;
        let drops = Arc::new(AtomicUsize::new(0));
        let observed = drops.clone();
        let mut session = Session::new(App::new()).close_validator(move |worlds| {
            ready(worlds[0].world_mut());
            std::panic::panic_any(Payload(observed.clone()));
        });
        let mut drain = CloseDrain::default();
        assert_eq!(
            drain.reconcile(&mut slots, |_| {}, session.close_validator.as_deref_mut()),
            CloseOutcome::Continue
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(slots[0].close.active && !slots[0].close.closed);
    }
}

#[test]
fn participation_changes_inside_validator_refuse_all_consumption() {
    for primary in [false, true] {
        for quit in [false, true] {
            let hub = WindowHub::new();
            let mut slots = vec![slot(&hub, primary, true), slot(&hub, false, true)];
            for s in &mut slots {
                s.close.requested = true;
            }
            let mut session = Session::new(App::new()).close_validator(move |worlds| {
                for w in &mut *worlds {
                    ready(w.world_mut());
                }
                if quit {
                    worlds[1].world_mut().resource_mut::<Input>().unwrap().quit = true;
                } else {
                    worlds[1]
                        .world_mut()
                        .resource_mut::<CloseGuard>()
                        .unwrap()
                        .cancel();
                }
                Ok(())
            });
            let mut drain = CloseDrain::default();
            assert_eq!(
                drain.reconcile(&mut slots, |_| {}, session.close_validator.as_deref_mut()),
                CloseOutcome::Continue
            );
            assert!(slots.iter().all(|s| !s.close.closed));
        }
    }
}

#[test]
fn dynamically_admitted_window_is_in_next_validation_and_exit_queue_fence() {
    let hub = WindowHub::new();
    let mut slots = vec![slot(&hub, true, false)];
    slots[0].close.requested = true;
    let desktop = slots[0]
        .app
        .world()
        .resource::<rndr_core::Desktop>()
        .unwrap()
        .clone();
    let mut d = CloseDrain::default();
    let mut hook = move |worlds: &mut [CloseWorld<'_>]| {
        assert_eq!(worlds.len(), 1);
        desktop.open(Default::default(), |_| {});
        Ok(())
    };
    assert_eq!(
        d.reconcile(&mut slots, |_| {}, Some(&mut hook)),
        CloseOutcome::Exit
    );
    assert!(!hub.try_commit_exit());
    assert_eq!(hub.take_opens().len(), 1);
    slots.push(slot(&hub, false, true));
    let mut check = |worlds: &mut [CloseWorld<'_>]| {
        assert_eq!(worlds.len(), 2);
        assert!(worlds.iter().all(|w| w.closing()));
        Ok(())
    };
    assert_eq!(
        d.reconcile(&mut slots, |_| {}, Some(&mut check)),
        CloseOutcome::Continue
    );
    ready(slots[1].app.world_mut());
    assert_eq!(
        d.reconcile(&mut slots, |_| {}, Some(&mut check)),
        CloseOutcome::Exit
    );
    assert!(hub.try_commit_exit());
}
