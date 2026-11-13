mod f5_support;
use f5_support::*;
use rndr_core::{App, CloseGuard, CloseState};
use rndr_core::editing::SourceDocuments;
use rndr_flow::*;

#[test]
fn decoy_matching_lease_cannot_bless_wrong_owned_allocation() {
    for push in [false, true] {
        let mut f = Fixture::new();
        let old = f.push(1);
        let other = f.new_target();
        let mut entry = f.entry_for(f.tab, 2, other);
        let decoy = f.flow.lease(entry.id(), other.document).unwrap();
        entry.lease = f.flow.lease(entry.id(), f.target.document).unwrap();
        let command = if push { Navigation::Push { tab:f.tab, after:Some(old), entry } }
            else { Navigation::Replace { tab:f.tab, old, entry } };
        let op = f.history.sender().submit(command).unwrap();
        f.tick();
        assert_eq!(f.history.phase(op), Some(Phase::Failed));
        assert_eq!(f.history.entries(f.tab).unwrap().iter().map(|e|e.id()).collect::<Vec<_>>(),vec![old]);
        assert_eq!(f.history.selected(f.tab).unwrap(),Some(old));
        assert_ne!(f.flow.state(old),State::Disposed);
        drop(decoy);
    }
}

#[test]
fn late_incoming_native_identity_loss_is_atomic_across_save_and_discard() {
    for push in [false,true] { for save in [false,true] {
        let mut f=Fixture::new();
        let first=f.push(1); let old=f.push(2);
        let owner=f.flow.register_world().unwrap(); let wid=owner.id();
        let mut other=App::new(); owner.bind(other.world_mut()).unwrap();
        let mut docs=SourceDocuments::default(); let native=docs.open("incoming only");
        other.world_mut().insert_resource(docs);
        let target=ProtectedTarget { document:f.flow.create_document().unwrap(), manager:wid, native:NativeDocument::Source(native) };
        f.host.register(target,None).unwrap();
        let entry=f.entry_for(f.tab,3,target);
        let command=if push { Navigation::Push {tab:f.tab,after:Some(first),entry} }
            else {Navigation::Replace{tab:f.tab,old,entry}};
        let op=f.history.sender().submit(command).unwrap();
        { let mut worlds=[HostWorld::new(f.world,f.app.world_mut()).unwrap(),HostWorld::new(wid,other.world_mut()).unwrap()];
          f.history.tick(&mut f.host.pass(&f.flow,&mut worlds)); }
        assert_eq!(f.history.phase(op),Some(Phase::AwaitingDecision));
        let before=f.snapshot();
        // Manager remains live and registered; only the exact incoming native ID vanishes.
        other.world_mut().insert_resource(SourceDocuments::default());
        f.choose(op,if save {Choice::Save} else {Choice::Discard}).unwrap();
        if save { for t in f.history.guard(op).unwrap().saves() {f.save(op,t,|_,_|Ok(())).unwrap();} }
        { let mut worlds=[HostWorld::new(f.world,f.app.world_mut()).unwrap(),HostWorld::new(wid,other.world_mut()).unwrap()];
          f.history.tick(&mut f.host.pass(&f.flow,&mut worlds)); }
        assert_eq!(f.history.phase(op),Some(Phase::Failed));
        assert!(f.flow.world_is_live(wid));
        assert_eq!(f.snapshot(),before,"original bytes/revision/history/selection/resources changed");
    }}
}

#[test]
fn omitted_owned_result_is_technical_loss_not_cancel() {
    let mut f=Fixture::new(); f.push(1);
    f.app.world_mut().resource_mut::<CloseGuard>().unwrap().request();
    {let mut w=[HostWorld::new(f.world,f.app.world_mut()).unwrap()]; f.host.sync_native(&f.flow,&mut w).unwrap();}
    let d=f.host.native_decision(f.world).unwrap();
    {let mut w=[HostWorld::new(f.world,f.app.world_mut()).unwrap()]; f.host.choose_native(&f.flow,&mut w,d,Choice::Save).unwrap();}
    let t=f.host.native_saves()[0];
    let result=f.host.own_native_save(t).unwrap().run(||Ok(()));
    let state=f.app.world().resource::<CloseGuard>().unwrap().state().clone();
    assert_eq!(f.host.complete_native_owned_save(&f.flow,&mut [],result),Err(Error::Pending));
    assert_eq!(*f.app.world().resource::<CloseGuard>().unwrap().state(),state);
    assert!(f.source().is_readonly());
    {let mut w=[HostWorld::new(f.world,f.app.world_mut()).unwrap()]; f.host.sync_native(&f.flow,&mut w).unwrap();}
    assert!(matches!(f.app.world().resource::<CloseGuard>().unwrap().state(),CloseState::Failed(..)));
    let retry=f.host.native_decision(f.world).unwrap();
    for c in [Choice::Save,Choice::Discard,Choice::Cancel] {
        assert_eq!(f.host.choose_native(&f.flow,&mut [],retry,c),Err(Error::Pending));
        assert_eq!(f.host.native_decision(f.world),Some(retry));
        assert!(f.source().is_readonly());
    }
}

#[test]
fn queued_select_lost_destination_keeps_current_selection() {
    let mut f=Fixture::new(); let first=f.push(1);let current=f.push(2);
    let blocker=f.close(vec![current]);
    let select=f.history.sender().submit(Navigation::Select{tab:f.tab,entry:first}).unwrap();
    f.tick();assert_eq!(f.history.phase(select),Some(Phase::Queued));
    f.flow.dispose(first).unwrap();f.history.cancel(blocker).unwrap();f.tick();
    assert_ne!(f.history.phase(select),Some(Phase::Committed));
    assert_eq!(f.history.selected(f.tab).unwrap(),Some(current));
    assert_ne!(f.flow.state(current),State::Disposed);
}
