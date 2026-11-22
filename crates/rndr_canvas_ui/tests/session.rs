//! A remote editor saves through a host. Here the host is a `MemoryStore`
//! answered by hand, so every request really goes through a store.
use rndr_canvas::*;
use rndr_canvas_ui::editor::*;

fn store() -> MemoryStore {
    let store = MemoryStore::new();
    store
        .create(CreateRequest {
            id: "doc".into(),
            title: "Doc".into(),
            scene: Scene::default(),
        })
        .unwrap();
    store
}

fn note(id: &str, x: f64) -> Operation {
    Operation::put(Node::new(
        id,
        Content::Shape {
            shape: ShapeKind::Rectangle,
            fill: "#ffd166".into(),
            stroke: "transparent".into(),
            stroke_width: 0.0,
            line: None,
        },
        Transform {
            x,
            y: 0.0,
            width: 40.0,
            height: 40.0,
            rotation: 0.0,
        },
    ))
}

fn ids(editor: &CanvasEditor) -> Vec<String> {
    editor.nodes().into_iter().map(|n| n.id).collect()
}

/// Sends one outgoing request to `store` and reports the answer.
fn answer(editor: &mut CanvasEditor, store: &MemoryStore, request: Outgoing) {
    match request {
        Outgoing::Get => {
            editor.replace(store.get("doc").unwrap());
        }
        Outgoing::Transact(t) => {
            let id = t.id.clone();
            match store.transact("doc", t) {
                Ok(m) => assert!(editor.confirm(&id, m)),
                Err(e) => assert!(editor.fail(&id, e)),
            }
        }
        Outgoing::History(h) => {
            let id = h.id.clone();
            match store.history("doc", h) {
                Ok(m) => assert!(editor.confirm(&id, m)),
                Err(e) => assert!(editor.fail(&id, e)),
            }
        }
    }
}

fn one(editor: &mut CanvasEditor) -> Outgoing {
    let mut out = editor.take_outgoing();
    assert_eq!(out.len(), 1, "{out:?}");
    out.remove(0)
}

#[test]
fn edits_show_at_once_and_save_one_at_a_time_in_order() {
    let store = store();
    let mut editor = CanvasEditor::remote(store.get("doc").unwrap());
    assert!(editor.commit("Add a", vec![note("a", 0.0)]));
    assert!(editor.commit("Add b", vec![note("b", 60.0)]));
    // Both show before the store has seen either.
    assert_eq!(ids(&editor), ["a", "b"]);
    assert!(editor.is_saving());
    assert!(editor.mutation().can_undo);

    let first = one(&mut editor);
    let Outgoing::Transact(t) = &first else {
        panic!("{first:?}")
    };
    assert_eq!(t.expected_revision, 0);
    // Nothing else leaves while the first is unanswered.
    assert!(editor.take_outgoing().is_empty());
    answer(&mut editor, &store, first);
    assert_eq!(ids(&editor), ["a", "b"]);

    // The second takes its revision from the first one's answer.
    let second = one(&mut editor);
    let Outgoing::Transact(t) = &second else {
        panic!("{second:?}")
    };
    assert_eq!(t.expected_revision, 1);
    answer(&mut editor, &store, second);
    assert!(!editor.is_saving());
    assert_eq!(editor.mutation().document.revision, 2);
    assert_eq!(ids(&editor), ["a", "b"]);
}

#[test]
fn an_unreachable_store_stalls_until_retried_with_the_same_id() {
    let store = store();
    let mut editor = CanvasEditor::remote(store.get("doc").unwrap());
    editor.commit("Add a", vec![note("a", 0.0)]);
    let request = one(&mut editor);
    let id = request.id().unwrap().to_string();
    assert!(editor.fail(&id, CanvasError::Storage("offline".into())));
    assert!(editor.is_stalled());
    assert_eq!(ids(&editor), ["a"], "the edit stays on screen");
    assert!(editor.take_outgoing().is_empty(), "not resent every frame");

    editor.retry();
    let again = one(&mut editor);
    assert_eq!(again, request, "same id, same base revision");
    answer(&mut editor, &store, again);
    assert!(!editor.is_stalled());
    assert_eq!(editor.mutation().document.revision, 1);
}

#[test]
fn a_stale_revision_drops_local_edits_and_reads_the_document() {
    let store = store();
    let mut editor = CanvasEditor::remote(store.get("doc").unwrap());
    // Another window saves first.
    store
        .transact(
            "doc",
            Transaction {
                id: "elsewhere".into(),
                expected_revision: 0,
                label: "Add theirs".into(),
                operations: vec![note("theirs", 0.0)],
            },
        )
        .unwrap();
    editor.commit("Add mine", vec![note("mine", 60.0)]);
    editor.commit("Add more", vec![note("more", 120.0)]);
    let request = one(&mut editor);
    answer(&mut editor, &store, request);
    assert!(matches!(
        editor.last_error(),
        Some(CanvasError::Conflict(1))
    ));

    assert_eq!(one(&mut editor), Outgoing::Get);
    answer(&mut editor, &store, Outgoing::Get);
    assert_eq!(ids(&editor), ["theirs"]);
    assert!(!editor.is_saving());
}

#[test]
fn undo_waits_for_confirmed_edits_and_goes_through_the_store() {
    let store = store();
    let mut editor = CanvasEditor::remote(store.get("doc").unwrap());
    editor.commit("Add a", vec![note("a", 0.0)]);
    assert!(!editor.undo(), "history waits while an edit is unconfirmed");
    let request = one(&mut editor);
    answer(&mut editor, &store, request);

    assert!(editor.undo());
    assert!(!editor.mutation().can_undo, "one history request at a time");
    let request = one(&mut editor);
    assert!(matches!(request, Outgoing::History(ref h) if !h.redo));
    answer(&mut editor, &store, request);
    assert!(ids(&editor).is_empty());
    assert!(editor.mutation().can_redo);

    assert!(editor.redo());
    let request = one(&mut editor);
    answer(&mut editor, &store, request);
    assert_eq!(ids(&editor), ["a"]);
}

#[test]
fn a_newer_document_from_elsewhere_lands_under_unconfirmed_edits() {
    let store = store();
    let mut editor = CanvasEditor::remote(store.get("doc").unwrap());
    store
        .transact(
            "doc",
            Transaction {
                id: "elsewhere".into(),
                expected_revision: 0,
                label: "Add theirs".into(),
                operations: vec![note("theirs", 0.0)],
            },
        )
        .unwrap();
    editor.commit("Add mine", vec![note("mine", 60.0)]);
    assert!(editor.replace(store.get("doc").unwrap()));
    assert_eq!(ids(&editor), ["theirs", "mine"]);
    // The queued edit now bases on the newer revision and saves cleanly.
    let request = one(&mut editor);
    answer(&mut editor, &store, request);
    assert_eq!(editor.mutation().document.revision, 2);
    assert!(editor.last_error().is_none());
    // An older read never rolls the document back.
    let mut stale = store.get("doc").unwrap();
    stale.document.revision = 1;
    assert!(!editor.replace(stale));
}

#[test]
fn a_store_backed_editor_never_queues() {
    let store = std::sync::Arc::new(store());
    let mut editor = CanvasEditor::open(store, "doc").unwrap();
    editor.commit("Add a", vec![note("a", 0.0)]);
    assert!(editor.take_outgoing().is_empty());
    assert!(!editor.is_saving());
    assert_eq!(editor.mutation().document.revision, 1);
}
