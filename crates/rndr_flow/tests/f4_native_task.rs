use rndr_core::editing::{EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments};
use rndr_core::{events, Sim};
use rndr_flow::*;

type Message = Completion<rndr_core::editing::DocumentId, Result<SourceBatch, TaskFailure>>;

#[test]
fn actual_owned_task_transposes_into_exact_native_commit_without_losing_receipt_or_failure() {
    let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
    let mut sim = Sim::new();
    let world = flow.register_world().unwrap();
    let owner = flow.create_scope(world.scope()).unwrap();
    world.bind(sim.world()).unwrap();
    let mut docs = SourceDocuments::default();
    let document = docs.open("a");
    sim.world().insert_resource(docs);
    let batch = SourceBatch {
        document,
        base: Revision(0),
        origin: EditOrigin::Host(11),
        changes: vec![SourceChange {
            range: 0..1,
            replacement: "X".into(),
        }],
    };
    let (_, success) = flow
        .replace_background(
            sim.world(),
            owner.id(),
            LaneKey(1),
            Identity {
                target: document,
                operation: 11,
                revision: 0,
            },
            async move { Ok::<_, TaskFailure>(batch) },
        )
        .ok()
        .unwrap();
    assert_eq!(success.observation().pending_deliveries, 1);
    assert!(!success.observation().exit_acknowledged);
    sim.update();
    let ready = events::<Message>(sim.world()).to_vec();
    assert_eq!(ready.len(), 1);
    assert!(success.observation().exit_acknowledged);
    assert_eq!(success.observation().pending_deliveries, 1);
    let applied = flow
        .commit_source_completion(sim.world(), ready[0].clone().transpose().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(applied.after, Revision(1));
    let before = {
        let d = sim
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap();
        (
            d.text().to_owned(),
            d.revision(),
            d.history_retained_bytes(),
        )
    };
    assert_eq!(before.0, "X");
    assert!(flow
        .commit_source_completion(sim.world(), ready[0].clone().transpose().unwrap())
        .is_err());
    drop(ready);
    let (_, failure) = flow
        .replace_background(
            sim.world(),
            owner.id(),
            LaneKey(2),
            Identity {
                target: document,
                operation: 12,
                revision: 1,
            },
            async { Err::<SourceBatch, _>(TaskFailure::Failed) },
        )
        .ok()
        .unwrap();
    sim.update();
    let errors = events::<Message>(sim.world()).to_vec();
    assert_eq!(errors.len(), 1);
    for envelope in errors {
        assert_eq!(envelope.transpose().unwrap_err(), TaskFailure::Failed);
    }
    let after = {
        let d = sim
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(document)
            .unwrap();
        (
            d.text().to_owned(),
            d.revision(),
            d.history_retained_bytes(),
        )
    };
    assert_eq!(after, before);
    assert_eq!(success.observation().terminal, Terminal::Succeeded);
    assert_eq!(failure.observation().terminal, Terminal::Failed);
    drop(owner);
    for _ in 0..200 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts().charged, 1);
    assert_eq!(flow.work_counts().pending_deliveries, 1);
    sim.update();
    for _ in 0..200 {
        flow.pump_work(1);
    }
    assert_eq!(flow.work_counts(), WorkCounts::default());
}
