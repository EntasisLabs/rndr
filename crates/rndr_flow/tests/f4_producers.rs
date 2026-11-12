//! Real producer-exit acknowledgments, not a cancelled UI handle or quiet poll.
use rndr_core::{events, FeedConfig, Sim};
use rndr_flow::*;
use std::sync::mpsc;
use std::thread;

#[test]
fn actual_cooperative_and_blocked_producers_ack_only_after_their_owned_closure_exits() {
    for seed in [1, 7, 42] {
        for cooperative in [true, false] {
            let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
            let mut sim = Sim::with_seed(seed);
            let world = flow.register_world().unwrap();
            let owner = flow.create_scope(world.scope()).unwrap();
            world.bind(sim.world()).unwrap();
            let (request, sender, exit, status) = flow
                .replace_feed::<_, u64>(
                    sim.world(),
                    owner.id(),
                    LaneKey(1),
                    Identity {
                        target: 17u64,
                        operation: 9,
                        revision: 0,
                    },
                    DeliveryMode::Ordered { recoverable: true },
                    FeedConfig::queue(1),
                )
                .unwrap();
            let (started, began) = mpsc::channel();
            let (release, blocked) = mpsc::channel();
            // This is the controlled EXTERNAL producer under test, not a new
            // production flow executor/runtime. Handshakes replace sleeps.
            let producer = thread::spawn(move || {
                exit.run(|| {
                    sender.try_send(Some(1), 1).unwrap();
                    started.send(()).unwrap();
                    if cooperative {
                        sender.wait_until_cancelled();
                    } else {
                        blocked.recv().unwrap(); // deliberately ignores cancellation
                        assert!(sender.is_cancelled());
                    }
                    Ok(())
                })
            });
            began.recv().unwrap();
            assert!(!status.observation().exit_acknowledged);
            assert_eq!(status.observation().pending_deliveries, 1);
            drop(owner);
            assert!(!flow.request_is_current(request.token()));
            for _ in 0..200 {
                let work = flow.pump_work(1);
                assert!(work.units <= 1);
            }
            assert!(status.observation().cancellation_signalled);
            if !cooperative {
                assert!(!status.observation().exit_acknowledged);
                assert_eq!(flow.work_counts().charged, 1);
                for _ in 0..1000 {
                    flow.pump_work(1);
                }
                assert!(!status.observation().exit_acknowledged);
                assert_eq!(flow.work_counts().retired_unacknowledged, 1);
                release.send(()).unwrap();
            }
            producer.join().unwrap().unwrap();
            assert!(status.observation().exit_acknowledged);
            assert_eq!(status.observation().terminal, Terminal::Succeeded);
            sim.update();
            assert!(events::<Completion<u64, u64>>(sim.world()).is_empty());
            for _ in 0..200 {
                flow.pump_work(1);
            }
            assert_eq!(flow.work_counts(), WorkCounts::default());
        }
    }
}
