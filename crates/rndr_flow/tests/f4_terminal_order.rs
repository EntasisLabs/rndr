//! Real producers, channel-selected orderings: aggregate failure is not exit.
use rndr_core::{FeedConfig, Sim};
use rndr_flow::*;
use std::future::Future;
use std::pin::pin;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::task::{Context, Poll, Wake, Waker};

struct PanicDrop(Arc<AtomicBool>);
impl Drop for PanicDrop {
    fn drop(&mut self) {
        if self.0.swap(false, Ordering::SeqCst) {
            panic!("single queue cleanup panic");
        }
    }
}
fn identity() -> Identity<u64> {
    Identity {
        target: 1,
        revision: 0,
        operation: 1,
    }
}
fn pump(flow: &mut Coordinator) {
    for _ in 0..400 {
        flow.pump_work(1);
    }
}
struct Noop;
impl Wake for Noop {
    fn wake(self: Arc<Self>) {}
}
fn result(kind: usize) -> Result<(), TaskFailure> {
    match kind % 5 {
        0 => Ok(()),
        1 => Err(TaskFailure::Failed),
        2 => Err(TaskFailure::Panicked),
        3 => panic!("actual producer poll/run panic"),
        _ => unreachable!(),
    }
}
#[test]
fn real_producer_cleanup_panic_and_exit_both_orderings_run_track_and_abandon() {
    // 0..4 closure/guard; 5..9 tracked future. Drop variants retain the guard
    // or future on the actual producing thread until its real exit.
    for kind in 0..10 {
        for cleanup_first in [false, true] {
            let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
            let mut sim = Sim::new();
            let world = flow.register_world().unwrap();
            let owner = flow.create_scope(world.scope()).unwrap();
            let independent = flow.create_scope(world.scope()).unwrap();
            world.bind(sim.world()).unwrap();
            let (_, sender, exit, status) = flow
                .replace_feed::<_, PanicDrop>(
                    sim.world(),
                    owner.id(),
                    LaneKey(1),
                    identity(),
                    DeliveryMode::Once,
                    FeedConfig::queue(1),
                )
                .unwrap();
            let (ready_tx, ready_rx) = mpsc::sync_channel(0);
            let (finish_tx, finish_rx) = mpsc::sync_channel(0);
            let worker = std::thread::spawn(move || {
                if kind < 4 {
                    Some(exit.run(|| {
                        sender
                            .send(None, PanicDrop(Arc::new(AtomicBool::new(true))))
                            .unwrap();
                        ready_tx.send(()).unwrap();
                        finish_rx.recv().unwrap();
                        result(kind)
                    }))
                } else if kind == 4 {
                    sender
                        .send(None, PanicDrop(Arc::new(AtomicBool::new(true))))
                        .unwrap();
                    ready_tx.send(()).unwrap();
                    finish_rx.recv().unwrap();
                    drop(sender);
                    drop(exit);
                    None
                } else {
                    let future = exit.track(async move {
                        sender
                            .send(None, PanicDrop(Arc::new(AtomicBool::new(true))))
                            .unwrap();
                        ready_tx.send(()).unwrap();
                        finish_rx.recv().unwrap();
                        if kind == 9 {
                            std::future::pending::<()>().await;
                            Ok(())
                        } else {
                            result(kind)
                        }
                    });
                    let mut future = pin!(future);
                    let waker = Waker::from(Arc::new(Noop));
                    let mut cx = Context::from_waker(&waker);
                    match future.as_mut().poll(&mut cx) {
                        Poll::Ready(r) => Some(r),
                        Poll::Pending => {
                            assert_eq!(kind, 9);
                            None
                        }
                    }
                    // The thread's pinned tracked future is destroyed here.
                }
            });
            ready_rx.recv().unwrap();
            let mut owner = Some(owner);
            if cleanup_first {
                drop(owner.take());
                pump(&mut flow);
                assert_eq!(status.observation().terminal, Terminal::Panicked);
                assert!(!status.observation().exit_acknowledged);
                assert_eq!(flow.work_counts().charged, 1);
                // Quiet pumps never manufacture an acknowledgment for this
                // channel-blocked actual producer, even after cleanup panics.
                pump(&mut flow);
                assert!(!status.observation().exit_acknowledged);
            }
            finish_tx.send(()).unwrap();
            let returned = worker.join().unwrap();
            assert!(status.observation().exit_acknowledged);
            if !cleanup_first {
                let expected = match kind % 5 {
                    0 => Terminal::Succeeded,
                    1 => Terminal::Failed,
                    2 | 3 => Terminal::Panicked,
                    _ => Terminal::DroppedUnknown,
                };
                assert_eq!(status.observation().terminal, expected);
                assert_eq!(status.observation().pending_deliveries, 1);
                drop(owner.take());
                pump(&mut flow);
            }
            assert_eq!(
                status.observation().terminal,
                Terminal::Panicked,
                "kind={kind} cleanup_first={cleanup_first}"
            );
            // run returns the closure result; track also normalizes aggregate
            // panic at poll completion, as on local/background task paths.
            let expected_return = if kind % 5 == 4 {
                None
            } else if kind >= 5 && cleanup_first {
                Some(Err(TaskFailure::Panicked))
            } else {
                Some(match kind % 5 {
                    0 => Ok(()),
                    1 => Err(TaskFailure::Failed),
                    _ => Err(TaskFailure::Panicked),
                })
            };
            assert_eq!(returned, expected_return);
            // Another live lane and actual task still progress after either
            // producer or queued-payload panic. No host update unwind escapes.
            let (_, independent_status) = flow
                .replace_local(
                    sim.world(),
                    independent.id(),
                    LaneKey(2),
                    identity(),
                    DeliveryMode::Once,
                    async { Ok::<_, TaskFailure>(1u64) },
                )
                .unwrap();
            sim.update();
            assert_eq!(
                independent_status.observation().terminal,
                Terminal::Succeeded
            );
            drop(independent);
            pump(&mut flow);
            sim.update();
            sim.update();
            pump(&mut flow);
            assert_eq!(flow.work_counts(), WorkCounts::default());
            println!("terminal-order kind={kind} cleanup_first={cleanup_first} panic_retained=true actual_exit=true independent_progress=true");
        }
    }
}
