mod f5_support;
use f5_support::*;
use rndr_core::{CloseGuard, CloseState};
use rndr_flow::*;
#[test]
fn actual_owned_durable_save_progresses_while_native_quit_waits() {
    let mut f = Fixture::new();
    let a = f.push(1);
    f.app
        .world_mut()
        .resource_mut::<CloseGuard>()
        .unwrap()
        .request();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    let decision = f.host.native_decision(f.world).unwrap();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .choose_native(&f.flow, &mut worlds, decision, Choice::Save)
            .unwrap();
    }
    let ticket = f.host.native_saves()[0];
    let job = f.host.own_native_save(ticket).unwrap();
    let bytes = f.source().text().as_bytes().to_vec();
    let path = std::env::temp_dir().join(format!("rndr-f5-native-save-{}.txt", std::process::id()));
    let output = path.clone();
    let (go, wait) = std::sync::mpsc::sync_channel(0);
    let worker = std::thread::spawn(move || {
        job.run(|| {
            use std::io::Write;
            wait.recv().unwrap();
            let mut file = std::fs::File::create(output).map_err(|_| SaveFailure::Failed)?;
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| SaveFailure::Failed)
        })
    });
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Pending(_)
    ));
    assert!(f.source().is_readonly());
    let batch = f.batch();
    assert!(f.source_mut().commit(batch).is_err());
    assert_ne!(f.flow.state(a), State::Disposed);
    go.send(()).unwrap();
    let result = worker.join().unwrap();
    {
        let mut worlds = [HostWorld::new(f.world, f.app.world_mut()).unwrap()];
        f.host
            .complete_native_owned_save(&f.flow, &mut worlds, result)
            .unwrap();
        f.host.sync_native(&f.flow, &mut worlds).unwrap();
    }
    assert!(matches!(
        f.app.world().resource::<CloseGuard>().unwrap().state(),
        CloseState::Ready(_)
    ));
    assert!(f.source().is_readonly());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), f.source().text());
    std::fs::remove_file(path).unwrap();
}
