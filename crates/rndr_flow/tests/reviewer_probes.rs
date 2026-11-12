// Original public reviewer probes from target/f3-review/probes.rs.
// Cases and assertions retained; formatting normalized by rustfmt.
use rndr_flow::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
#[test]
fn queued_fifo_survives_callback_unwind_and_full_queue() {
    let (mut f, root) = Coordinator::new(Limits {
        commands: 2,
        cleanups_per_pump: 1,
        ..Limits::default()
    })
    .unwrap();
    let (other, foreign) = Coordinator::new(Limits::default()).unwrap();
    let a = foreign.id();
    let b = other.root();
    let foreign_child = {
        let mut other = other;
        let c = other.create_scope(b).unwrap();
        let id = c.id();
        drop(c);
        id
    };
    f.register_cleanup(root.id(), CleanupKind::Lifetime, move |q| {
        q.dispose(a).unwrap();
        q.dispose(foreign_child).unwrap();
        assert_eq!(q.dispose(a), Err(Error::Capacity));
        panic!("queued unwind");
    })
    .unwrap();
    drop(root);
    f.pump();
    f.pump();
    let errors: Vec<_> = f
        .diagnostics()
        .filter(|d| d.outcome == Outcome::Error)
        .map(|d| d.scope)
        .collect();
    assert_eq!(errors, [a, foreign_child]);
    assert_eq!(f.counts(), Counts::default());
    println!("FIFO/unwind: foreign requests rejected in submitted order; queue empty; counts zero");
}
#[test]
fn dropping_ancestor_inside_suspended_cleanup_reorders_pending_children() {
    let (mut f, root) = Coordinator::new(Limits {
        cleanups_per_pump: 1,
        ..Limits::default()
    })
    .unwrap();
    let parent = f.create_scope(root.id()).unwrap();
    let p = parent.id();
    let a = f.create_scope(p).unwrap();
    let b = f.create_scope(p).unwrap();
    let log = Rc::new(RefCell::new(Vec::new()));
    for (id, n) in [(a.id(), 1), (p, 3)] {
        let l = log.clone();
        f.register_cleanup(id, CleanupKind::Lifetime, move |_| {
            l.borrow_mut().push(n);
            Ok(())
        })
        .unwrap();
    }
    let l = log.clone();
    f.register_cleanup(b.id(), CleanupKind::Activity, move |_| {
        drop(parent);
        l.borrow_mut().push(2);
        Ok(())
    })
    .unwrap();
    f.suspend(b.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    for _ in 0..5 {
        f.pump();
    }
    assert_eq!(*log.borrow(), [2, 1, 3]);
    assert_eq!(f.counts().scopes, 1);
    assert_eq!(f.activity(a.id()), Err(Error::Stale));
    println!("owner-drop reentry: trace [2,1,3], descendants stale immediately, root baseline");
}
#[test]
fn bounded_suspend_drain_requires_all_activity_attempts_and_retains_lifetime() {
    let (mut f, root) = Coordinator::new(Limits {
        cleanups_per_pump: 1,
        ..Limits::default()
    })
    .unwrap();
    let n = Rc::new(Cell::new(0));
    for _ in 0..3 {
        let n = n.clone();
        f.register_cleanup(root.id(), CleanupKind::Activity, move |_| {
            n.set(n.get() + 1);
            Err(CleanupError)
        })
        .unwrap();
    }
    f.register_cleanup(root.id(), CleanupKind::Lifetime, |_| Ok(()))
        .unwrap();
    let old = f.activity(root.id()).unwrap();
    f.suspend(root.id(), Recovery::ReconcileFromDurableState)
        .unwrap();
    for _ in 0..2 {
        f.pump();
        assert_eq!(f.resume(root.id(), true), Err(Error::Pending));
    }
    f.pump();
    let new = f.resume(root.id(), true).unwrap();
    assert_ne!(old, new);
    assert_eq!(n.get(), 3);
    assert_eq!(f.counts().cleanups, 1);
    println!("suspension: 3 attempted errors, resume pending until final activity consumed, lifetime retained");
}
