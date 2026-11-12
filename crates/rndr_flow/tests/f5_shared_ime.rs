mod f5_support;
use f5_support::*;
use rndr_core::{App, Input};
use rndr_flow::*;
#[test]
fn shared_hidden_participant_pending_ime_is_never_silently_discarded_or_omitted() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let owner = f.flow.register_world().unwrap();
    let bid = owner.id();
    let parent = owner.scope();
    let mut b = App::new();
    owner.bind(b.world_mut()).unwrap();
    let hidden = f.flow.create_scope(parent).unwrap();
    f.flow.set_visibility(hidden.id(), State::Hidden).unwrap();
    let _lease = f.flow.lease(hidden.id(), f.target.document).unwrap();
    b.world_mut().resource_mut::<Input>().unwrap().ime_preedit = "共有の未確定".into();
    let before = f.snapshot();
    let op = f.close(vec![a]);
    assert_eq!(f.history.phase(op), Some(Phase::Queued));
    assert_eq!(f.snapshot(), before);
    assert!(!f.source().is_readonly());
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(bid, b.world_mut()).unwrap(),
        ];
        f.history.tick(&mut f.host.pass(&f.flow, &mut worlds));
    }
    assert_eq!(f.history.phase(op), Some(Phase::Queued));
    assert_eq!(
        b.world().resource::<Input>().unwrap().ime_preedit,
        "共有の未確定"
    );
    assert_eq!(f.snapshot(), before);
    // Only the editor/user, not a timeout/observer/close guard, ends preedit.
    b.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .ime_preedit
        .clear();
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(bid, b.world_mut()).unwrap(),
        ];
        f.history.tick(&mut f.host.pass(&f.flow, &mut worlds));
    }
    assert_eq!(f.history.phase(op), Some(Phase::AwaitingDecision));
    assert!(f.source().is_readonly());
    f.history.cancel(op).unwrap();
    assert!(!f.source().is_readonly());
}
