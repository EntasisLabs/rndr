use rndr_core::editing::DocumentId;
use rndr_core::editing::*;
use rndr_core::{Buffer, Editor, Focus, Input, Sim};
use rndr_flow::*;

fn batch(id: DocumentId, revision: u64, operation: u64) -> SourceBatch {
    SourceBatch {
        document: id,
        base: Revision(revision),
        origin: EditOrigin::Host(operation),
        changes: vec![SourceChange {
            range: 0..1,
            replacement: "X".into(),
        }],
    }
}
fn snapshot(world: &rndr_core::World, ids: &[DocumentId]) -> Vec<(String, Revision, usize)> {
    let docs = world.resource::<SourceDocuments>().unwrap();
    ids.iter()
        .map(|&id| {
            let d = docs.get(id).unwrap();
            (d.text().into(), d.revision(), d.history_retained_bytes())
        })
        .collect()
}
#[test]
fn ordered_source_chunks_advance_only_owned_native_receipts_without_replacement_or_rebase() {
    for seed in [1, 7, 42] {
        let mut sim = Sim::with_seed(seed);
        let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
        let world = flow.register_world().unwrap();
        let owner = flow.create_scope(world.scope()).unwrap();
        world.bind(sim.world()).unwrap();
        let mut docs = SourceDocuments::default();
        let document = docs.open("a");
        sim.world().insert_resource(docs);
        let request = flow
            .replace_request(
                sim.world(),
                owner.id(),
                LaneKey(1),
                Identity {
                    target: document,
                    operation: 5,
                    revision: 0,
                },
                DeliveryMode::Ordered { recoverable: true },
            )
            .unwrap();
        let token = request.token();
        for (sequence, base, text) in [(1, 0, "x"), (2, 1, "y")] {
            let mut payload = batch(document, base, 5);
            payload.changes[0].replacement = text.into();
            let committed = flow
                .commit_source_completion(sim.world(), request.completion(Some(sequence), payload))
                .unwrap()
                .unwrap();
            assert_eq!(committed.after, Revision(base + 1));
            assert_eq!(request.token(), token);
            assert!(flow.request_is_current(token));
        }
        let before = snapshot(sim.world(), &[document]);
        let mut same = batch(document, 2, 5);
        same.changes[0].replacement = "y".into();
        assert_eq!(
            flow.commit_source_completion(sim.world(), request.completion(Some(3), same.clone())),
            Ok(None)
        );
        assert_eq!(snapshot(sim.world(), &[document]), before);
        for sequence in [1, 2, 3] {
            assert!(flow
                .commit_source_completion(
                    sim.world(),
                    request.completion(Some(sequence), same.clone())
                )
                .is_err());
            assert_eq!(snapshot(sim.world(), &[document]), before);
        }
        assert_eq!(
            sim.world()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(document)
                .unwrap()
                .drain_events()
                .count(),
            2
        );
        let mut external = batch(document, 2, 9);
        external.changes[0].replacement = "external".into();
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(document)
            .unwrap()
            .commit(external)
            .unwrap();
        assert_eq!(
            sim.world()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(document)
                .unwrap()
                .drain_events()
                .count(),
            1
        );
        let before = snapshot(sim.world(), &[document]);
        // Neither an old native base nor a producer's attempt to bless the
        // outside revision may continue this operation. No automatic rebasing.
        for base in [2, 3] {
            let mut payload = batch(document, base, 5);
            payload.changes[0].replacement = "must not land".into();
            assert_eq!(
                flow.commit_source_completion(sim.world(), request.completion(Some(4), payload)),
                Err(CompletionError::Native(SourceCompletionError::Native(
                    EditError::StaleRevision
                )))
            );
            assert_eq!(snapshot(sim.world(), &[document]), before);
            assert_eq!(
                sim.world()
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(document)
                    .unwrap()
                    .drain_events()
                    .count(),
                0
            );
        }
        assert_eq!(request.token(), token);
        assert_eq!(request.identity().revision, 0);
    }
}
#[test]
fn native_source_zero_mutation_revision_readonly_composition_operation_and_target_refusals() {
    for seed in [1, 7, 42] {
        let mut sim = Sim::with_seed(seed);
        let (mut flow, _root) = Coordinator::new(Limits::default()).unwrap();
        let world = flow.register_world().unwrap();
        let owner = flow.create_scope(world.scope()).unwrap();
        world.bind(sim.world()).unwrap();
        let mut docs = SourceDocuments::default();
        let a = docs.open("alpha");
        let b = docs.open("bravo");
        sim.world().insert_resource(docs);
        let view = sim.world().spawn((Buffer::from_str(""), Editor::default()));
        attach_source(sim.world(), view, a).unwrap();
        sim.world().insert_resource(Focus { entity: Some(view) });
        let ids = [a, b];
        let id = Identity {
            target: a,
            operation: 7,
            revision: 0,
        };
        let request = flow
            .replace_request(sim.world(), owner.id(), LaneKey(1), id, DeliveryMode::Once)
            .unwrap();
        for (payload, expected) in [
            (
                batch(b, 0, 7),
                SourceCompletionError::Native(EditError::MissingDocument),
            ),
            (
                batch(a, 1, 7),
                SourceCompletionError::Native(EditError::StaleRevision),
            ),
            (batch(a, 0, 8), SourceCompletionError::Operation),
        ] {
            let before = snapshot(sim.world(), &ids);
            assert_eq!(
                flow.commit_source_completion(sim.world(), request.completion(None, payload)),
                Err(CompletionError::Native(expected))
            );
            assert_eq!(snapshot(sim.world(), &ids), before);
            for &id in &ids {
                assert_eq!(
                    sim.world()
                        .resource_mut::<SourceDocuments>()
                        .unwrap()
                        .get_mut(id)
                        .unwrap()
                        .drain_events()
                        .count(),
                    0
                );
            }
        }
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap()
            .set_readonly(true)
            .unwrap();
        let before = snapshot(sim.world(), &ids);
        assert_eq!(
            flow.commit_source_completion(sim.world(), request.completion(None, batch(a, 0, 7))),
            Err(CompletionError::Native(SourceCompletionError::Native(
                EditError::Readonly
            )))
        );
        assert_eq!(snapshot(sim.world(), &ids), before);
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap()
            .set_readonly(false)
            .unwrap();
        // No normal update/preparation: test earliest Input policy, not just
        // already observed SourceSession::composition.
        sim.world().resource_mut::<Input>().unwrap().ime_preedit = "に".into();
        let before = snapshot(sim.world(), &ids);
        assert_eq!(
            flow.commit_source_completion(sim.world(), request.completion(None, batch(a, 0, 7))),
            Err(CompletionError::Native(SourceCompletionError::Native(
                EditError::Composing
            )))
        );
        assert_eq!(snapshot(sim.world(), &ids), before);
        // Session-native composition remains authoritative even after pending
        // Input was cleared, until the native composition owner observes it.
        prepare_source_input(sim.world());
        assert!(sim
            .world()
            .resource::<SourceDocuments>()
            .unwrap()
            .get(a)
            .unwrap()
            .is_composing());
        sim.world()
            .resource_mut::<Input>()
            .unwrap()
            .ime_preedit
            .clear();
        let before = snapshot(sim.world(), &ids);
        assert_eq!(
            flow.commit_source_completion(sim.world(), request.completion(None, batch(a, 0, 7))),
            Err(CompletionError::Native(SourceCompletionError::Native(
                EditError::Composing
            )))
        );
        assert_eq!(snapshot(sim.world(), &ids), before);
        prepare_source_input(sim.world());
        // An unrelated writer wins after request start. No implicit rebase.
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap()
            .commit(batch(a, 0, 9))
            .unwrap();
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap()
            .drain_events()
            .for_each(drop);
        let before = snapshot(sim.world(), &ids);
        assert_eq!(
            flow.commit_source_completion(sim.world(), request.completion(None, batch(a, 0, 7))),
            Err(CompletionError::Native(SourceCompletionError::Native(
                EditError::StaleRevision
            )))
        );
        assert_eq!(snapshot(sim.world(), &ids), before);
        for &id in &ids {
            assert_eq!(
                sim.world()
                    .resource_mut::<SourceDocuments>()
                    .unwrap()
                    .get_mut(id)
                    .unwrap()
                    .drain_events()
                    .count(),
                0
            );
        }
        // An exact fresh request can commit once, never to the selected view's
        // target B; changing focus is not target authority.
        let view_b = sim.world().spawn((Buffer::from_str(""), Editor::default()));
        attach_source(sim.world(), view_b, b).unwrap();
        sim.world().resource_mut::<Focus>().unwrap().entity = Some(view_b);
        let fresh = flow
            .replace_request(
                sim.world(),
                owner.id(),
                LaneKey(1),
                Identity {
                    target: a,
                    operation: 10,
                    revision: 1,
                },
                DeliveryMode::Once,
            )
            .unwrap();
        let mut payload = batch(a, 1, 10);
        payload.changes[0].replacement = "Y".into();
        flow.commit_source_completion(sim.world(), fresh.completion(None, payload.clone()))
            .unwrap();
        let before = snapshot(sim.world(), &ids);
        assert!(flow
            .commit_source_completion(sim.world(), fresh.completion(None, payload))
            .is_err());
        assert_eq!(snapshot(sim.world(), &ids), before);
        assert_eq!(
            sim.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(b)
                .unwrap()
                .text(),
            "bravo"
        );
        assert_eq!(
            sim.world()
                .resource_mut::<SourceDocuments>()
                .unwrap()
                .get_mut(a)
                .unwrap()
                .drain_events()
                .count(),
            1
        );
        sim.world()
            .resource_mut::<SourceDocuments>()
            .unwrap()
            .get_mut(a)
            .unwrap()
            .undo()
            .unwrap();
        assert_eq!(
            sim.world()
                .resource::<SourceDocuments>()
                .unwrap()
                .get(a)
                .unwrap()
                .text(),
            "Xlpha"
        );
    }
}
