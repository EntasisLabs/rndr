mod f5_support;
use f5_support::*;
use rndr_core::editing::SourceDocuments;
use rndr_core::{CloseGuard, CloseState};
use rndr_flow::*;
fn sync(f: &mut Fixture) -> Result<(), Error> {
    let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
    f.host.sync_native(&f.flow, &mut worlds)
}
fn choose(f: &mut Fixture, c: Choice) {
    let d = f.host.native_decision(f.world).unwrap();
    let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
    f.host.choose_native(&f.flow, &mut worlds, d, c).unwrap();
}
#[test]
fn retained_disposal_conflicts_with_native_ready_and_resumes_after_cancel() {
    let mut f = Fixture::new();
    let a = f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    sync(&mut f).unwrap();
    choose(&mut f, Choice::Discard);
    sync(&mut f).unwrap();
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Ready(_)
    ));
    let op = f.close(vec![a]);
    assert_eq!(f.history.phase(op), Some(Phase::Queued));
    assert_ne!(f.flow.state(a), State::Disposed);
    for _ in 0..8 {
        f.tick();
    }
    assert_eq!(f.history.phase(op), Some(Phase::Queued));
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .cancel();
    sync(&mut f).unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
    f.choose(op, Choice::Discard).unwrap();
    f.tick();
    assert_eq!(f.history.phase(op), Some(Phase::Committed));
}
#[test]
fn unavoidable_new_document_while_ready_invalidates_and_requires_explicit_restart() {
    let mut f = Fixture::new();
    let a = f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    sync(&mut f).unwrap();
    choose(&mut f, Choice::Discard);
    sync(&mut f).unwrap();
    let native = f
        .app
        .world_mut()
        .resource_mut::<SourceDocuments>()
        .unwrap()
        .open("new unsaved unleased manager document");
    assert_eq!(sync(&mut f), Err(Error::Pending));
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Pending(_)
    ));
    assert!(f.source().is_readonly());
    let target = ProtectedTarget {
        document: f.flow.create_document().unwrap(),
        manager: f.world,
        native: NativeDocument::Source(native),
    };
    f.host.register(target, None).unwrap();
    sync(&mut f).unwrap();
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Failed(_, _)
    ));
    assert_ne!(f.flow.state(a), State::Disposed);
    choose(&mut f, Choice::Cancel);
    sync(&mut f).unwrap();
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    sync(&mut f).unwrap();
    choose(&mut f, Choice::Save);
    let tickets = f.host.native_saves();
    assert_eq!(tickets.len(), 2);
    for ticket in tickets {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .save_native(&f.flow, &mut worlds, ticket, |_, _| Ok(()))
            .unwrap();
    }
    sync(&mut f).unwrap();
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Ready(_)
    ));
    assert!(f
        .app
        .world()
        .resource::<SourceDocuments>()
        .unwrap()
        .get(native)
        .unwrap()
        .is_readonly());
}
