//! Independent deterministic probes: import the actual production-used helper.
//! The seven original in-module tests also execute unchanged. No native GUI run.
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use runner::{CloseDrain, CloseOutcome, CloseParticipant, CloseSlot, CloseWorld};
use rndr_core::{App, CloseGuard, CloseState, Desktop, Input, WindowHub, WindowId, World};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};

struct Node { app: App, slot: CloseSlot, primary: bool, id: WindowId }
impl CloseParticipant for Node {
    fn parts(&mut self) -> (&mut World, &mut CloseSlot, bool, WindowId) {
        (self.app.world_mut(), &mut self.slot, self.primary, self.id)
    }
}
fn node(hub: &WindowHub, primary: bool, guarded: bool) -> Node {
    let desktop=hub.attach(); let id=desktop.id(); let mut app=App::new();
    app.world_mut().insert_resource(desktop);
    if guarded { app.world_mut().insert_resource(CloseGuard::default()); }
    Node { app, slot:CloseSlot::default(), primary, id }
}
fn ready(w: &mut World) {
    if let Some(g)=w.resource_mut::<CloseGuard>() { let r=g.request(); assert!(g.complete(r,Ok(()))); }
}
fn pending(w: &World) -> rndr_core::CloseRequest {
    match w.resource::<CloseGuard>().unwrap().state() {
        CloseState::Pending(r) | CloseState::Ready(r) | CloseState::Failed(r,_) => *r,
        s => panic!("unexpected {s:?}"),
    }
}

#[test]
fn independent_secondary_readiness_error_matrix_preserves_unrelated_primary() {
    // Three simultaneous secondaries: every subset Ready, every subset Failed
    // instead of Pending, validator success/error. Unrelated unguarded primary
    // must be supplied, but never becomes a native closing participant.
    for ready_mask in 0..8 { for failure_mask in 0..8 { for error in [false,true] {
        let hub=WindowHub::new(); let mut ns=vec![node(&hub,true,false)];
        for _ in 0..3 { let mut n=node(&hub,false,true);n.slot.requested=true;ns.push(n); }
        let mut d=CloseDrain::default(); let calls=Arc::new(AtomicUsize::new(0));let observed=calls.clone();
        let mut hook=move |ws: &mut [CloseWorld<'_>]| {
            observed.fetch_add(1,Ordering::SeqCst);assert_eq!(ws.len(),4);assert!(ws[0].primary() && !ws[0].closing());
            assert!(ws[1..].iter().all(|w|w.closing() && !w.primary()));
            for (i,w) in ws[1..].iter_mut().enumerate() {
                let g=w.world_mut().resource_mut::<CloseGuard>().unwrap();let r=g.request();
                if ready_mask & (1<<i)!=0 {assert!(g.complete(r,Ok(())));}
                else if failure_mask & (1<<i)!=0 {assert!(g.complete(r,Err("io refused".into())));}
            }
            if error {Err("global validation refused".into())}else{Ok(())}
        };
        assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut hook)),CloseOutcome::Continue);
        assert_eq!(calls.load(Ordering::SeqCst),1);assert!(!ns[0].slot.closed && !ns[0].slot.active);
        for i in 0..3 {assert_eq!(ns[i+1].slot.closed,!error && ready_mask & (1<<i)!=0);}
    }}}
    eprintln!("INDEPENDENT secondary-matrix coordinates=128 all_worlds=true partial_readiness_only=true global_error_atomic=true");
}

#[test]
fn hook_local_drop_queued_open_after_last_check_forces_new_full_pass() {
    struct QueueOnDrop(Desktop,Arc<AtomicUsize>);
    impl Drop for QueueOnDrop {
        fn drop(&mut self) {let count=self.1.clone();self.0.open(Default::default(),move |a|{
            count.fetch_add(1,Ordering::SeqCst);a.world_mut().insert_resource(CloseGuard::default());
        });}
    }
    let hub=WindowHub::new();let mut ns=vec![node(&hub,true,true)];ns[0].slot.requested=true;
    let desktop=ns[0].app.world().resource::<Desktop>().unwrap().clone();
    let built=Arc::new(AtomicUsize::new(0));let count=built.clone();let mut d=CloseDrain::default();
    let mut hook=move |ws: &mut [CloseWorld<'_>]| {
        let _late=QueueOnDrop(desktop.clone(),count.clone());
        assert_eq!(ws.len(),1);ready(ws[0].world_mut());Ok(())
        // _late drops after the final check but before reconcile returns.
    };
    assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut hook)),CloseOutcome::Exit);
    assert!(!hub.try_commit_exit());assert_eq!(built.load(Ordering::SeqCst),0);
    let opens=hub.take_opens();assert_eq!(opens.len(),1);
    for open in opens {let mut n=node(&hub,false,false);(open.build)(&mut n.app);ns.push(n);}
    let mut check=|ws: &mut [CloseWorld<'_>]|{assert_eq!(ws.len(),2);assert!(ws.iter().all(|w|w.closing()));Ok(())};
    assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut check)),CloseOutcome::Continue);
    assert!(!ns[1].slot.closed);ready(ns[1].app.world_mut());
    assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut check)),CloseOutcome::Exit);
    assert!(hub.try_commit_exit());assert_eq!(built.load(Ordering::SeqCst),1);
    let rejected=Arc::new(AtomicUsize::new(0));let r=rejected.clone();
    ns[0].app.world().resource::<Desktop>().unwrap().open(Default::default(),move |_|{r.fetch_add(1,Ordering::SeqCst);});
    assert!(hub.take_opens().is_empty());assert_eq!(rejected.load(Ordering::SeqCst),0);
}

#[test]
fn error_and_panic_retry_preserves_exact_request_without_repeating_frames() {
    for process in [false,true] {
        let hub=WindowHub::new();let mut ns=vec![node(&hub,process,true),node(&hub,false,true)];
        for n in &mut ns {n.slot.requested=true;}
        let mut d=CloseDrain::default();let mut frames=0;
        let mut no=|_: &mut [CloseWorld<'_>]|Err("again".into());
        assert_eq!(d.reconcile(&mut ns, |_|{frames+=1;},Some(&mut no)),CloseOutcome::Continue);
        assert_eq!(frames,2);let old:Vec<_>=ns.iter().map(|n|pending(n.app.world())).collect();
        for round in 0..12 {
            let old=old.clone();let mut fail=move |ws: &mut [CloseWorld<'_>]| {
                for (w,r) in ws.iter().zip(&old) {assert_eq!(pending(w.world()),*r);}
                if round%2==0 {panic!("independent round {round}");}
                Err("technical failure".into())
            };
            assert_eq!(d.reconcile(&mut ns, |_|panic!("frame repeated"),Some(&mut fail)),CloseOutcome::Continue);
            assert!(ns.iter().all(|n|n.slot.active && !n.slot.closed));
        }
        let mut yes=|ws: &mut [CloseWorld<'_>]|{for w in ws {ready(w.world_mut());}Ok(())};
        assert_eq!(d.reconcile(&mut ns, |_|panic!("frame repeated"),Some(&mut yes))==CloseOutcome::Exit,process);
        assert_eq!(ns.iter().all(|n|n.slot.closed),!process);
    }
}

#[test]
fn cancel_process_then_secondary_quit_uses_fresh_native_requests() {
    let hub=WindowHub::new();let mut ns=vec![node(&hub,true,true),node(&hub,false,true)];
    ns[0].slot.requested=true;let mut d=CloseDrain::default();
    assert_eq!(d.reconcile(&mut ns, |_|{},None),CloseOutcome::Continue);
    let old:Vec<_>=ns.iter().map(|n|pending(n.app.world())).collect();
    ns[1].app.world_mut().resource_mut::<CloseGuard>().unwrap().cancel();
    assert_eq!(d.reconcile(&mut ns, |_|{},None),CloseOutcome::Continue);
    assert!(ns.iter().all(|n|!n.slot.active && !n.slot.closed));
    ns[1].app.world_mut().resource_mut::<Input>().unwrap().quit=true;
    assert_eq!(d.reconcile(&mut ns, |_|{},None),CloseOutcome::Continue);
    for (n,old) in ns.iter_mut().zip(old) {
        assert!(n.slot.active);assert_ne!(pending(n.app.world()),old);
        assert!(!n.app.world_mut().resource_mut::<CloseGuard>().unwrap().complete(old,Ok(())));
        ready(n.app.world_mut());
    }
    assert_eq!(d.reconcile(&mut ns, |_|{},None),CloseOutcome::Exit);
}

#[test]
fn late_primary_quit_in_hook_refuses_entire_secondary_pass_then_revalidates() {
    let hub=WindowHub::new();let mut ns=vec![node(&hub,true,false),node(&hub,false,true),node(&hub,false,true)];
    ns[1].slot.requested=true;ns[2].slot.requested=true;let mut d=CloseDrain::default();
    let mut late=|ws: &mut [CloseWorld<'_>]|{
        assert!(!ws[0].closing());for w in &mut ws[1..] {ready(w.world_mut());}
        ws[0].world_mut().resource_mut::<Input>().unwrap().quit=true;Ok(())
    };
    assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut late)),CloseOutcome::Continue);
    assert!(ns.iter().all(|n|!n.slot.closed));let called=Arc::new(AtomicUsize::new(0));let seen=called.clone();
    let mut all=move |ws: &mut [CloseWorld<'_>]|{seen.fetch_add(1,Ordering::SeqCst);assert!(ws.iter().all(|w|w.closing()));Ok(())};
    assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut all)),CloseOutcome::Exit);assert_eq!(called.load(Ordering::SeqCst),1);
}

#[test]
fn opaque_secondary_payload_leak_is_real_repeatable_and_not_bounded_by_window_count() {
    struct Secondary(Arc<AtomicUsize>);
    impl Drop for Secondary {fn drop(&mut self){self.0.fetch_add(1,Ordering::SeqCst);}}
    struct First(Arc<AtomicUsize>);
    impl Drop for First {fn drop(&mut self){std::panic::panic_any(Secondary(self.0.clone()));}}
    let leaked=Arc::new(AtomicUsize::new(0));let observed=leaked.clone();
    let hub=WindowHub::new();let mut ns=vec![node(&hub,true,true)];ns[0].slot.requested=true;
    let mut d=CloseDrain::default();let mut panic=move |_: &mut [CloseWorld<'_>]|->Result<(),String>{
        std::panic::panic_any(First(observed.clone()));
    };
    for round in 1..=4 {
        assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut panic)),CloseOutcome::Continue);
        assert!(!ns[0].slot.closed);assert_eq!(leaked.load(Ordering::SeqCst),0);
        assert_eq!(Arc::strong_count(&leaked),2+round);
    }
    drop(panic);assert_eq!(Arc::strong_count(&leaked),5);
    eprintln!("INDEPENDENT opaque-secondary-payload repeated=4 destructors=0 leaked_arc_owners=4 same_window=true safety_retained=true RESOURCE_EXCEPTION_NOT_NO_LEAK=true");
}

#[test]
fn secondary_destruction_runs_arbitrary_resources_only_after_validation() {
    struct OnDrop { phase:Arc<AtomicUsize>, desktop:Desktop, count:Arc<AtomicUsize> }
    impl Drop for OnDrop {fn drop(&mut self){assert_eq!(self.phase.load(Ordering::SeqCst),2);
        self.count.fetch_add(1,Ordering::SeqCst);self.desktop.open(Default::default(), |_|{});
    }}
    let hub=WindowHub::new();let mut ns=vec![node(&hub,true,false),node(&hub,false,true),node(&hub,false,true)];
    let phase=Arc::new(AtomicUsize::new(0));let drops=Arc::new(AtomicUsize::new(0));
    for n in &mut ns[1..] {n.slot.requested=true;let desktop=n.app.world().resource::<Desktop>().unwrap().clone();
        n.app.world_mut().insert_resource(OnDrop{phase:phase.clone(),desktop,count:drops.clone()});}
    let p=phase.clone();let mut hook=move |ws: &mut [CloseWorld<'_>]|{
        assert_eq!(p.swap(1,Ordering::SeqCst),0);for w in &mut ws[1..]{ready(w.world_mut());}Ok(())};
    let mut d=CloseDrain::default();assert_eq!(d.reconcile(&mut ns, |_|{},Some(&mut hook)),CloseOutcome::Continue);
    assert_eq!(drops.load(Ordering::SeqCst),0);assert!(ns[1..].iter().all(|n|n.slot.closed));
    phase.store(2,Ordering::SeqCst);ns.retain(|n|!n.slot.closed);
    assert_eq!(ns.len(),1);assert_eq!(drops.load(Ordering::SeqCst),2);
    assert_eq!(hub.take_opens().len(),2);
    eprintln!("INDEPENDENT destruction-boundary callbacks_before_commit=0 resource_destructors_during_removal=2 queued_opens=2 rollback_NOT_promised=true");
}
