//! Host-adapter multiworld qualification. Includes the unchanged runner close
//! coordinator directly; no OS event loop/GPU/native GUI certification is claimed.
mod f5_support;
#[path = "../../rndr_render/src/close.rs"]
mod runner;
use f5_support::*;
use rndr_core::editing::SourceDocuments;
use rndr_core::{App, CloseGuard, CloseState, World};
use rndr_flow::*;
fn sync(f: &mut Fixture, b: &mut App, bid: WorldId) {
    let mut worlds = [
        HostWorld::new(f.world, f.app.world_mut()).unwrap(),
        HostWorld::new(bid, b.world_mut()).unwrap(),
    ];
    f.host.sync_native(&f.flow, &mut worlds).unwrap();
}
fn choose(f: &mut Fixture, b: &mut App, bid: WorldId, world: WorldId, choice: Choice) {
    let d = f.host.native_decision(world).unwrap();
    let mut worlds = [
        HostWorld::new(f.world, f.app.world_mut()).unwrap(),
        HostWorld::new(bid, b.world_mut()).unwrap(),
    ];
    f.host
        .choose_native(&f.flow, &mut worlds, d, choice)
        .unwrap();
}
fn guard(world: &World) -> &CloseState {
    world.resource::<CloseGuard>().unwrap().state()
}
#[test]
fn multiworld_shared_hidden_leases_ready_is_not_disposal_cancel_preserves_everything() {
    let mut f = Fixture::new();
    let a = f.push(1);
    f.flow.set_visibility(a, State::Hidden).unwrap();
    let owner = f.flow.register_world().unwrap();
    let bid = owner.id();
    let bscope = owner.scope();
    let mut b = App::new();
    owner.bind(b.world_mut()).unwrap();
    b.world_mut().insert_resource(CloseGuard::default());
    let presentation = f.flow.create_scope(bscope).unwrap();
    let lease = f.flow.lease(presentation.id(), f.target.document).unwrap();
    let before = f.snapshot();
    let mut drain = runner::CloseDrain::default();
    let mut ca = runner::CloseSlot {
        requested: true,
        ..Default::default()
    };
    let mut cb = runner::CloseSlot::default();
    assert!(!drain.step([
        (f.app.world_mut(), &mut ca, true),
        (b.world_mut(), &mut cb, false)
    ]));
    sync(&mut f, &mut b, bid);
    let aw = f.world;
    choose(&mut f, &mut b, bid, aw, Choice::Discard);
    sync(&mut f, &mut b, bid);
    assert!(matches!(guard(f.app.world()), CloseState::Ready(_)));
    assert!(!drain.step([
        (f.app.world_mut(), &mut ca, true),
        (b.world_mut(), &mut cb, false)
    ]));
    assert_eq!(f.snapshot(), before);
    assert_eq!(f.flow.state(a), State::Hidden);
    assert_eq!(f.flow.register_world().err(), Some(Error::Pending));
    assert_eq!(
        f.flow.lease(presentation.id(), f.target.document).err(),
        Some(Error::Pending)
    );
    choose(&mut f, &mut b, bid, bid, Choice::Cancel);
    assert!(!drain.step([
        (f.app.world_mut(), &mut ca, true),
        (b.world_mut(), &mut cb, false)
    ]));
    sync(&mut f, &mut b, bid);
    assert_eq!(f.snapshot(), before);
    assert!(!f.source().is_readonly());
    drop(lease);
    drop(presentation);
}
#[test]
fn quit_save_lane_failure_retry_stale_request_and_late_window_admission() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let owner = f.flow.register_world().unwrap();
    let bid = owner.id();
    let mut b = App::new();
    owner.bind(b.world_mut()).unwrap();
    b.world_mut().insert_resource(CloseGuard::default());
    let mut drain = runner::CloseDrain::default();
    let mut ca = runner::CloseSlot::default();
    let mut cb = runner::CloseSlot::default();
    b.world_mut()
        .resource_mut::<rndr_core::Input>()
        .unwrap()
        .quit = true;
    assert!(!drain.step([
        (f.app.world_mut(), &mut ca, true),
        (b.world_mut(), &mut cb, false)
    ]));
    sync(&mut f, &mut b, bid);
    let old = match guard(f.app.world()) {
        CloseState::Pending(r) => *r,
        _ => panic!(),
    };
    let aw = f.world;
    choose(&mut f, &mut b, bid, aw, Choice::Save);
    let ticket = f.host.native_saves()[0];
    // Saving is allowed while the process-exit transition awaits it. Simulated IO
    // panic fails closed; actual durability is separately exercised by f5.rs.
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(bid, b.world_mut()).unwrap(),
        ];
        f.host
            .save_native(&f.flow, &mut worlds, ticket, |_, _| panic!("save panic"))
            .unwrap();
    }
    sync(&mut f, &mut b, bid);
    assert!(matches!(guard(f.app.world()), CloseState::Failed(_, _)));
    assert!(f.source().is_readonly());
    assert_eq!(f.flow.state(a), State::Visible);
    choose(&mut f, &mut b, bid, aw, Choice::Save);
    let retry = f.host.native_saves()[0];
    assert_ne!(retry, ticket);
    assert!(!f
        .app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .complete(old, Ok(())));
    {
        let mut worlds = [
            HostWorld::new(f.world, f.app.world_mut()).unwrap(),
            HostWorld::new(bid, b.world_mut()).unwrap(),
        ];
        assert_eq!(
            f.host
                .save_native(&f.flow, &mut worlds, ticket, |_, _| panic!("stale save")),
            Err(Error::Stale)
        );
        f.host
            .save_native(&f.flow, &mut worlds, retry, |w, n| {
                let NativeDocument::Source(id) = n else {
                    panic!()
                };
                assert_eq!(
                    w.resource::<SourceDocuments>()
                        .unwrap()
                        .get(id)
                        .unwrap()
                        .text(),
                    "unsaved α\nexact bytes"
                );
                Ok(())
            })
            .unwrap();
    }
    sync(&mut f, &mut b, bid);
    assert!(matches!(guard(f.app.world()), CloseState::Ready(_)));
    assert!(matches!(guard(b.world()), CloseState::Ready(_)));
    assert!(f.source().is_readonly());
    assert_eq!(f.flow.register_world().err(), Some(Error::Pending));
    assert!(drain.step([
        (f.app.world_mut(), &mut ca, true),
        (b.world_mut(), &mut cb, false)
    ]));
    assert_ne!(f.flow.state(a), State::Disposed);
    // Runner-consumed shutdown drops real WorldOwner resources. It is the only
    // event permitting this native attempt to release Ready protection.
    drop(std::mem::replace(&mut f.app, App::new()));
    drop(b);
    f.host.reconcile_world_loss(&f.flow);
    assert_eq!(f.flow.state(a), State::Disposed);
    assert!(f.flow.register_world().is_ok());
}
#[test]
fn missing_world_unregistered_document_and_lost_shared_lease_never_bless_ready() {
    let mut f = Fixture::new();
    let a = f.push(1);
    let owner = f.flow.register_world().unwrap();
    let bid = owner.id();
    let scope = owner.scope();
    let mut b = App::new();
    owner.bind(b.world_mut()).unwrap();
    b.world_mut().insert_resource(CloseGuard::default());
    let p = f.flow.create_scope(scope).unwrap();
    let lease = f.flow.lease(p.id(), f.target.document).unwrap();
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    b.world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    {
        let mut missing = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        assert_eq!(
            f.host.sync_native(&f.flow, &mut missing),
            Err(Error::Pending)
        );
    }
    assert!(matches!(guard(f.app.world()), CloseState::Pending(_)));
    sync(&mut f, &mut b, bid);
    let aw = f.world;
    choose(&mut f, &mut b, bid, aw, Choice::Discard);
    sync(&mut f, &mut b, bid);
    assert!(matches!(guard(f.app.world()), CloseState::Ready(_)));
    drop(lease);
    sync(&mut f, &mut b, bid);
    assert!(matches!(guard(f.app.world()), CloseState::Failed(_, _)));
    assert!(f.source().is_readonly());
    assert_ne!(f.flow.state(a), State::Disposed);
}
