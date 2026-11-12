//! The UI-specific seam stays in its consumer, not rndr-flow or the facade.
use rndr_core::World;
use rndr_flow::*;
use rndr_liquid_ui::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

const A: &str = "```accordion\ntitle: FAQ\n---\nlabel: One\nbody: Body\n```\n";
struct ComponentWriter;
impl WorldCompletionTarget<LiquidBlockHandle, LiquidComponentState> for ComponentWriter {
    type Output = LiquidComponentRecord;
    type Error = LiquidF2Error;
    fn commit(
        &mut self,
        world: &mut World,
        id: &Identity<LiquidBlockHandle>,
        value: LiquidComponentState,
    ) -> Result<Self::Output, Self::Error> {
        // The existing F2 API checks exact surface/block incarnation and record
        // revision, synchronously. No source-byte/entity/selected-tab fallback.
        put_liquid_component_state_for(world, id.target, id.revision, value)
    }
}
#[test]
fn consumer_native_f2_gate_refuses_retired_revision_and_cross_world_without_changes() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut world = World::new();
    let owner = flow.register_world().unwrap();
    let scope = flow.create_scope(owner.scope()).unwrap();
    owner.bind(&mut world).unwrap();
    let a = liquid_view(&mut world, A);
    let b = liquid_view(&mut world, A);
    let surface_a = liquid_surface_handle(&world, a).unwrap();
    let surface_b = liquid_surface_handle(&world, b).unwrap();
    let handle_a = liquid_component_handle(&world, surface_a, 0).unwrap();
    let handle_b = liquid_component_handle(&world, surface_b, 0).unwrap();
    let request = flow
        .replace_request(
            &world,
            scope.id(),
            LaneKey(1),
            Identity {
                target: handle_a,
                operation: 1,
                revision: 0,
            },
            DeliveryMode::Once,
        )
        .unwrap();
    let state = LiquidComponentState::Accordion { open: vec![0] };
    let mut other = World::new();
    flow.register_world().unwrap().bind(&mut other).unwrap();
    let before = (
        liquid_component_state_for(&world, handle_a).unwrap(),
        liquid_component_state_for(&world, handle_b).unwrap(),
    );
    assert!(flow
        .commit_world_completion(
            &mut other,
            request.completion(None, state.clone()),
            &mut ComponentWriter
        )
        .is_err());
    assert_eq!(
        (
            liquid_component_state_for(&world, handle_a).unwrap(),
            liquid_component_state_for(&world, handle_b).unwrap()
        ),
        before
    );
    // Another authoritative native writer wins after request start.
    put_liquid_component_state_for(&mut world, handle_a, 0, state.clone()).unwrap();
    let before = (
        liquid_component_state_for(&world, handle_a).unwrap(),
        liquid_component_state_for(&world, handle_b).unwrap(),
    );
    let refused = flow.commit_world_completion(
        &mut world,
        request.completion(None, state.clone()),
        &mut ComponentWriter,
    );
    assert_eq!(
        refused,
        Err(CompletionError::Native(LiquidF2Error::Revision {
            expected: 0,
            actual: 1
        }))
    );
    assert_eq!(
        (
            liquid_component_state_for(&world, handle_a).unwrap(),
            liquid_component_state_for(&world, handle_b).unwrap()
        ),
        before
    );
    // Reset physically retires the handle while its flow lane is still valid.
    reset_liquid_block(&mut world, handle_a, 1).unwrap();
    let before = liquid_component_states(&world, b);
    assert_eq!(
        flow.commit_world_completion(
            &mut world,
            request.completion(None, state),
            &mut ComponentWriter
        ),
        Err(CompletionError::Native(LiquidF2Error::StaleBlock))
    );
    assert_eq!(liquid_component_states(&world, b), before);
}
struct Listener(Arc<AtomicUsize>);
impl LiquidInteractionHandlerFor for Listener {
    fn on_interaction(&mut self, _: &mut World, _: &LiquidInteractionEnvelopeFor) {}
}
impl Drop for Listener {
    fn drop(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn consumer_explicit_unsubscribe_is_world_local_and_precedes_custom_cleanup() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut world = World::new();
    let owner = flow.register_world().unwrap();
    let scope = flow.create_scope(owner.scope()).unwrap();
    owner.bind(&mut world).unwrap();
    let request = flow
        .replace_request(
            &world,
            scope.id(),
            LaneKey(1),
            Identity {
                target: 1u64,
                operation: 1,
                revision: 0,
            },
            DeliveryMode::Once,
        )
        .unwrap();
    let drops = Arc::new(AtomicUsize::new(0));
    let subscription = subscribe_liquid_interactions_owned_for(
        &mut world,
        LiquidListenerOwner::World,
        Listener(drops.clone()),
    )
    .ok()
    .unwrap();
    let status = flow
        .own_world_effect(&mut world, &request, move |world| {
            unsubscribe_liquid_interactions(world, subscription)
        })
        .unwrap();
    let observed = drops.clone();
    flow.register_cleanup(scope.id(), CleanupKind::Lifetime, move |_| {
        assert_eq!(observed.load(Ordering::SeqCst), 1);
        Ok(())
    })
    .unwrap();
    drop(scope);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    assert!(!status.observation().exit_acknowledged);
    let mut other = World::new();
    drain_world_effects(&mut other, 10);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drain_world_effects(&mut world, 2);
    for _ in 0..100 {
        flow.pump_work(1);
    }
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    assert!(status.observation().cleanup_attempted);
    assert!(status.observation().exit_acknowledged);
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
