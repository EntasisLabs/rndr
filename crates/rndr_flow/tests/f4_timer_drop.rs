//! Timer-owned destruction boundary versus caller-retained delivery receipts.
use rndr_core::{events, Sim};
use rndr_flow::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Default)]
struct Shared {
    status: Option<WorkStatus>,
    during: Vec<Observation>,
}
struct Payload {
    shared: Arc<Mutex<Shared>>,
    original: bool,
    panic: bool,
}
impl Clone for Payload {
    fn clone(&self) -> Self {
        // Caller-owned clones have no destructor panic; the single intentional
        // panic belongs to the timer boundary under test, not arbitrary callers.
        Self {
            shared: self.shared.clone(),
            original: false,
            panic: false,
        }
    }
}
impl Drop for Payload {
    fn drop(&mut self) {
        if self.original {
            let mut shared = self.shared.lock().unwrap();
            let observation = shared.status.as_ref().unwrap().observation();
            shared.during.push(observation);
            drop(shared);
            if self.panic {
                panic!("single owned timer payload drop panic");
            }
        }
    }
}
fn id() -> Identity<u64> {
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
#[test]
fn timer_cancel_publish_world_drop_and_retained_clone_orders_contain_single_panic() {
    for mode in 0..6 {
        for panic in [false, true] {
            let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
            let mut sim = Some(Sim::new());
            let world = flow.register_world().unwrap();
            let scope = flow.create_scope(world.scope()).unwrap();
            let sibling = flow.create_scope(world.scope()).unwrap();
            world.bind(sim.as_mut().unwrap().world()).unwrap();
            let mut other = Sim::new();
            let other_world = flow.register_world().unwrap();
            let independent = flow.create_scope(other_world.scope()).unwrap();
            other_world.bind(other.world()).unwrap();
            let shared = Arc::new(Mutex::new(Shared::default()));
            let (_, status) = flow
                .replace_timer(
                    sim.as_mut().unwrap().world(),
                    scope.id(),
                    LaneKey(1),
                    id(),
                    Duration::from_millis(1),
                    Payload {
                        shared: shared.clone(),
                        original: true,
                        panic,
                    },
                )
                .ok()
                .unwrap();
            shared.lock().unwrap().status = Some(status.clone());
            let mut retained = None;
            if matches!(mode, 1 | 2 | 3 | 5) {
                let sim = sim.as_mut().unwrap();
                sim.advance(Duration::from_millis(1));
                sim.update();
                let published = events::<TimerCompletion<u64, Payload>>(sim.world());
                assert_eq!(published.len(), 1);
                assert!(!status.observation().exit_acknowledged);
                if matches!(mode, 2 | 3 | 5) {
                    retained = Some(published[0].completion());
                }
            }
            if mode == 2 {
                drop(retained.take());
            }
            drop(scope);
            pump(&mut flow);
            assert!(status.observation().cleanup_attempted);
            assert!(!status.observation().exit_acknowledged);
            // Cancellation alone does not destroy a future/published event.
            // Exercise both runtime buffer retirement and actual World loss.
            let same_world = if mode < 4 {
                Some(
                    flow.replace_local(
                        sim.as_mut().unwrap().world(),
                        sibling.id(),
                        LaneKey(3),
                        id(),
                        DeliveryMode::Once,
                        async { Ok::<_, TaskFailure>(1u64) },
                    )
                    .unwrap()
                    .1,
                )
            } else {
                None
            };
            let destroyed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if mode >= 4 {
                    drop(sim.take());
                } else {
                    let sim = sim.as_mut().unwrap();
                    sim.update();
                    sim.update();
                }
            }));
            assert!(destroyed.is_ok(), "owned panic escaped mode={mode}");
            if let Some(same_world) = same_world {
                assert_eq!(same_world.observation().terminal, Terminal::Succeeded);
                drop(sibling);
                pump(&mut flow);
                let sim = sim.as_mut().unwrap();
                sim.update();
                sim.update();
            } else {
                drop(sibling);
            }
            let during = &shared.lock().unwrap().during;
            assert_eq!(during.len(), 1);
            assert!(!during[0].exit_acknowledged, "mode={mode}");
            assert_eq!(during[0].pending_deliveries, 1);
            assert!(status.observation().exit_acknowledged);
            assert_eq!(
                status.observation().terminal,
                if panic {
                    Terminal::Panicked
                } else {
                    Terminal::DroppedUnknown
                }
            );
            pump(&mut flow);
            if retained.is_some() {
                assert_eq!(status.observation().pending_deliveries, 1);
                assert_eq!(flow.work_counts().charged, 1);
                // Logical disposal/acknowledged event exit cannot release the
                // final caller-retained receipt. Drop it last, then reclaim.
                drop(retained.take());
            }
            assert_eq!(status.observation().pending_deliveries, 0);
            pump(&mut flow);
            assert_eq!(flow.work_counts(), WorkCounts::default());
            let (_, next) = flow
                .replace_local(
                    other.world(),
                    independent.id(),
                    LaneKey(2),
                    id(),
                    DeliveryMode::Once,
                    async { Ok::<_, TaskFailure>(42u64) },
                )
                .unwrap();
            other.update();
            assert_eq!(next.observation().terminal, Terminal::Succeeded);
            assert!(next.observation().exit_acknowledged);
            drop(independent);
            pump(&mut flow);
            other.update();
            other.update();
            pump(&mut flow);
            assert_eq!(flow.work_counts(), WorkCounts::default());
            println!("timer-order mode={mode} panic={panic} no_early_ack=true receipt_preserved=true independent_progress=true");
        }
    }
}
