//! Desired safety assertions for the documented missing Session seam, NOT in-scope correction regressions.
mod f5_support;
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use f5_support::*;
use rndr_core::{App, CloseGuard, CloseState};
use rndr_core::editing::SourceDocuments;
use rndr_flow::*;

#[test]
fn fabricated_guard_request_is_not_actual_runner_participation() {
    let mut f=Fixture::new(); f.push(1);
    let owner=f.flow.register_world().unwrap();let wid=owner.id();let scope=owner.scope();
    let mut other=App::new();owner.bind(other.world_mut()).unwrap();
    other.world_mut().insert_resource(CloseGuard::default());
    let mut docs=SourceDocuments::default();let native=docs.open("B manager");other.world_mut().insert_resource(docs);
    let target=ProtectedTarget{document:f.flow.create_document().unwrap(),manager:wid,native:NativeDocument::Source(native)};
    f.host.register(target,None).unwrap();
    let _lease=f.flow.lease(scope,f.target.document).unwrap();
    let mut a=runner::CloseSlot{requested:true,..Default::default()};let mut b=runner::CloseSlot::default();
    let mut drain=runner::CloseDrain::default();
    assert!(!drain.step([(f.app.world_mut(),&mut a,false),(other.world_mut(),&mut b,false)]));
    assert!(a.active && !b.active);
    // Public CloseGuard request alone does NOT activate the private runner slot.
    other.world_mut().resource_mut::<CloseGuard>().unwrap().request();
    {let mut worlds=[HostWorld::new(f.world,f.app.world_mut()).unwrap(),HostWorld::new(wid,other.world_mut()).unwrap()];
     f.host.sync_native(&f.flow,&mut worlds).unwrap();
     for id in [f.world,wid] {let d=f.host.native_decision(id).unwrap();f.host.choose_native(&f.flow,&mut worlds,d,Choice::Discard).unwrap();}
     f.host.sync_native(&f.flow,&mut worlds).unwrap();}
    assert!(!drain.step([(f.app.world_mut(),&mut a,false),(other.world_mut(),&mut b,false)]));
    eprintln!("SEAM participation A_closed={} B_active={} B_closed={}",a.closed,b.active,b.closed);
    assert!(!a.closed,"manager A must survive non-closing B's held A presentation; Session seam must supply actual participation");
}

#[test]
fn secondary_tentative_close_must_be_withdrawable_after_callback() {
    let mut app=App::new();app.world_mut().insert_resource(CloseGuard::default());
    let mut c=runner::CloseSlot{requested:true,..Default::default()};let mut drain=runner::CloseDrain::default();
    assert!(!drain.step([(app.world_mut(),&mut c,false)]));
    let r=match app.world().resource::<CloseGuard>().unwrap().state(){CloseState::Pending(r)=>*r,s=>panic!("{s:?}")};
    app.world_mut().resource_mut::<CloseGuard>().unwrap().complete(r,Ok(()));
    // Newly requested hidden frames can run after this step in real reconcile_close.
    assert!(!drain.step([(app.world_mut(),&mut c,false)]));assert!(c.closed);
    app.world_mut().resource_mut::<CloseGuard>().unwrap().invalidate();
    assert!(!drain.step([(app.world_mut(),&mut c,false)]));
    eprintln!("SEAM post-callback closed={} guard={:?}",c.closed,app.world().resource::<CloseGuard>().unwrap().state());
    assert!(!c.closed,"a later guard recheck alone does not clear the sticky secondary destruction flag");
}
