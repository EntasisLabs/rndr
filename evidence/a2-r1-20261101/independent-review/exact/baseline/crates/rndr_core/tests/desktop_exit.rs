use rndr_core::{App, WindowConfig, WindowHub};
use std::sync::{Arc, Barrier};

#[test]
fn pending_and_recursive_build_opens_block_commitment() {
    let hub = WindowHub::new();
    let desktop = hub.attach();
    desktop.open(WindowConfig::default(), |app| {
        rndr_core::desktop(app.world())
            .unwrap()
            .open(WindowConfig::default(), |_| {});
    });
    assert!(!hub.try_commit_exit());
    let first = hub.take_opens();
    let mut app = App::new();
    app.world_mut().insert_resource(hub.attach());
    (first.into_iter().next().unwrap().build)(&mut app);
    assert!(!hub.try_commit_exit());
    assert_eq!(hub.take_opens().len(), 1);
    assert!(hub.try_commit_exit());
    desktop.open(WindowConfig::default(), |_| panic!("post-commit build"));
    assert!(hub.take_opens().is_empty());
}

#[test]
fn concurrent_open_is_either_admitted_or_after_commit_never_stranded() {
    for _ in 0..100 {
        let hub = WindowHub::new();
        let desktop = hub.attach();
        let barrier = Arc::new(Barrier::new(2));
        let peer = barrier.clone();
        let thread = std::thread::spawn(move || {
            peer.wait();
            desktop.open(WindowConfig::default(), |_| {});
        });
        barrier.wait();
        let committed = hub.try_commit_exit();
        thread.join().unwrap();
        let opens = hub.take_opens();
        assert_eq!(opens.len(), usize::from(!committed));
    }
}
