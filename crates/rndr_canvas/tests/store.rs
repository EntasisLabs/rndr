use rndr_canvas::*;

fn rect(id: &str, x: f64) -> Node {
    Node::new(
        id,
        Content::Shape {
            shape: ShapeKind::Rectangle,
            fill: "#336699".into(),
            stroke: "transparent".into(),
            stroke_width: 0.0,
            line: None,
        },
        Transform {
            x,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            rotation: 0.0,
        },
    )
}

fn group(id: &str) -> Node {
    Node::new(id, Content::Group, Transform::default())
}

fn create(store: &dyn CanvasStore) -> Document {
    store
        .create(CreateRequest {
            id: "doc".into(),
            title: "Doc".into(),
            scene: Scene::default(),
        })
        .unwrap()
}

fn tx(id: &str, revision: u64, operations: Vec<Operation>) -> Transaction {
    Transaction {
        id: id.into(),
        expected_revision: revision,
        label: "Edit".into(),
        operations,
    }
}

fn undo(id: &str, revision: u64, redo: bool) -> HistoryRequest {
    HistoryRequest {
        id: id.into(),
        expected_revision: revision,
        redo,
    }
}

fn ids(doc: &Document) -> Vec<&str> {
    doc.scene.nodes.iter().map(|n| n.id.as_str()).collect()
}

fn each_store(test: impl Fn(&dyn CanvasStore)) {
    test(&MemoryStore::new());
    let dir = tempfile::tempdir().unwrap();
    test(&FsStore::open(dir.path()).unwrap());
}

#[test]
fn edits_advance_revisions_and_undo_redo_restore_them() {
    each_store(|store| {
        create(store);
        let a = store
            .transact("doc", tx("t1", 0, vec![Operation::put(rect("a", 0.0))]))
            .unwrap();
        assert_eq!(
            (a.document.revision, a.can_undo, a.undo_label.as_deref()),
            (1, true, Some("Edit"))
        );
        let mut moved = rect("a", 50.0);
        moved.name = Some("Moved".into());
        store
            .transact("doc", tx("t2", 1, vec![Operation::put(moved.clone())]))
            .unwrap();
        let back = store.history("doc", undo("u1", 2, false)).unwrap();
        assert_eq!(back.document.revision, 3);
        assert_eq!(back.document.scene.nodes[0], rect("a", 0.0));
        assert!(back.can_redo);
        let again = store.history("doc", undo("r1", 3, true)).unwrap();
        assert_eq!(again.document.scene.nodes[0], moved);
        assert!(!again.can_redo);
        // A new edit clears redo.
        store.history("doc", undo("u2", 4, false)).unwrap();
        let fresh = store
            .transact("doc", tx("t3", 5, vec![Operation::put(rect("b", 0.0))]))
            .unwrap();
        assert!(!fresh.can_redo);
        assert_eq!(store.get("doc").unwrap().document, fresh.document);
    });
}

#[test]
fn stale_revisions_conflict_and_retries_are_idempotent() {
    each_store(|store| {
        create(store);
        let first = tx("t1", 0, vec![Operation::put(rect("a", 0.0))]);
        store.transact("doc", first.clone()).unwrap();
        let retry = store.transact("doc", first).unwrap();
        assert!(retry.duplicate);
        assert_eq!((retry.applied_revision, retry.document.revision), (1, 1));
        let changed = tx("t1", 0, vec![Operation::put(rect("a", 9.0))]);
        assert!(matches!(
            store.transact("doc", changed),
            Err(CanvasError::Invalid(_))
        ));
        let stale = tx("t2", 0, vec![Operation::put(rect("b", 0.0))]);
        assert_eq!(store.transact("doc", stale), Err(CanvasError::Conflict(1)));
        let undo_req = undo("u1", 1, false);
        store.history("doc", undo_req.clone()).unwrap();
        assert!(store.history("doc", undo_req).unwrap().duplicate);
        assert!(store.history("doc", undo("u2", 2, false)).is_err());
    });
}

#[test]
fn invalid_batches_change_nothing() {
    each_store(|store| {
        create(store);
        let mut bad = rect("b", 0.0);
        bad.opacity = 2.0;
        let err = store.transact(
            "doc",
            tx(
                "t1",
                0,
                vec![Operation::put(rect("a", 0.0)), Operation::put(bad)],
            ),
        );
        assert!(matches!(err, Err(CanvasError::Invalid(_))));
        let now = store.get("doc").unwrap();
        assert_eq!((now.document.revision, now.can_undo), (0, false));
        assert!(now.document.scene.nodes.is_empty());
        // The failed id was not consumed.
        store
            .transact("doc", tx("t1", 0, vec![Operation::put(rect("a", 0.0))]))
            .unwrap();
    });
}

#[test]
fn removal_cascades_and_undo_restores_the_subtree() {
    each_store(|store| {
        create(store);
        let ops = vec![
            Operation::put(group("g")),
            Operation::put(rect("a", 0.0).with_parent("g")),
            Operation::put(rect("b", 0.0)),
        ];
        let made = store.transact("doc", tx("t1", 0, ops)).unwrap().document;
        let removed = store
            .transact(
                "doc",
                tx(
                    "t2",
                    1,
                    vec![Operation::remove("g"), Operation::remove("a")],
                ),
            )
            .unwrap();
        assert_eq!(ids(&removed.document), ["b"]);
        let unknown = store.transact("doc", tx("t3", 2, vec![Operation::remove("zzz")]));
        assert!(unknown.is_err());
        let restored = store.history("doc", undo("u1", 2, false)).unwrap().document;
        assert_eq!(restored.scene.nodes, made.scene.nodes);
    });
}

#[test]
fn reorder_needs_every_node_once() {
    each_store(|store| {
        create(store);
        let ops = vec![
            Operation::put(rect("a", 0.0)),
            Operation::put(rect("b", 0.0)),
        ];
        store.transact("doc", tx("t1", 0, ops)).unwrap();
        let swap = |ids: &[&str]| Operation::Reorder {
            ids: ids.iter().map(|s| s.to_string()).collect(),
        };
        assert!(store
            .transact("doc", tx("t2", 1, vec![swap(&["a"])]))
            .is_err());
        assert!(store
            .transact("doc", tx("t3", 1, vec![swap(&["a", "a"])]))
            .is_err());
        let done = store
            .transact("doc", tx("t4", 1, vec![swap(&["b", "a"])]))
            .unwrap();
        assert_eq!(ids(&done.document), ["b", "a"]);
    });
}

#[test]
fn hierarchy_must_be_acyclic_and_parented_by_containers() {
    each_store(|store| {
        create(store);
        let orphan = rect("a", 0.0).with_parent("missing");
        assert!(store
            .transact("doc", tx("t1", 0, vec![Operation::put(orphan)]))
            .is_err());
        let ops = vec![
            Operation::put(rect("a", 0.0)),
            Operation::put(rect("b", 0.0).with_parent("a")),
        ];
        assert!(store.transact("doc", tx("t2", 0, ops)).is_err());
        let ops = vec![
            Operation::put(group("g").with_parent("h")),
            Operation::put(group("h").with_parent("g")),
        ];
        assert!(store.transact("doc", tx("t3", 0, ops)).is_err());
    });
}

#[test]
fn assets_are_fingerprinted_and_owned_by_the_document() {
    each_store(|store| {
        create(store);
        let png = b"\x89PNG\r\n\x1a\nnot really a png";
        let asset = store.import_asset("doc", png).unwrap();
        assert_eq!(asset.mime, "image/png");
        assert_eq!(asset.id, format!("a_{}", asset.sha256));
        assert_eq!(store.import_asset("doc", png).unwrap(), asset);
        assert_eq!(store.get("doc").unwrap().document.revision, 1);
        assert_eq!(store.asset("doc", &asset.id).unwrap().1, png);
        assert!(store.import_asset("doc", b"#!/bin/sh").is_err());
        let image = Node::new(
            "img",
            Content::Image {
                asset_id: asset.id.clone(),
                crop: None,
            },
            Transform {
                width: 10.0,
                height: 10.0,
                ..Transform::default()
            },
        );
        store
            .transact("doc", tx("t1", 1, vec![Operation::put(image)]))
            .unwrap();
        let missing = Node::new(
            "img2",
            Content::Image {
                asset_id: "a_missing".into(),
                crop: None,
            },
            Transform::default(),
        );
        assert!(store
            .transact("doc", tx("t2", 2, vec![Operation::put(missing)]))
            .is_err());
    });
}

#[test]
fn create_retries_list_pages_and_delete_hides_documents() {
    each_store(|store| {
        let doc = create(store);
        assert_eq!(create(store), doc);
        let other = CreateRequest {
            id: "doc".into(),
            title: "Other".into(),
            scene: Scene::default(),
        };
        assert!(store.create(other).is_err());
        for id in ["b", "c", "d"] {
            store
                .create(CreateRequest {
                    id: id.into(),
                    title: id.into(),
                    scene: Scene::medousa(),
                })
                .unwrap();
        }
        store.delete("c", 0).unwrap();
        assert_eq!(store.get("c"), Err(CanvasError::NotFound));
        let first = store
            .list(ListQuery {
                cursor: None,
                limit: Some(2),
            })
            .unwrap();
        let names: Vec<_> = first.documents.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(names, ["b", "d"]);
        assert_eq!(first.next_cursor.as_deref(), Some("d"));
        let rest = store
            .list(ListQuery {
                cursor: first.next_cursor,
                limit: Some(2),
            })
            .unwrap();
        let names: Vec<_> = rest.documents.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(names, ["doc"]);
        assert_eq!(rest.next_cursor, None);
        assert!(store
            .list(ListQuery {
                cursor: None,
                limit: Some(0)
            })
            .is_err());
    });
}

#[test]
fn fs_store_survives_reopen_and_refuses_unknown_schemas() {
    let dir = tempfile::tempdir().unwrap();
    {
        let store = FsStore::open(dir.path()).unwrap();
        create(&store);
        store
            .transact("doc", tx("t1", 0, vec![Operation::put(rect("a", 0.0))]))
            .unwrap();
    }
    let store = FsStore::open(dir.path()).unwrap();
    let got = store.get("doc").unwrap();
    assert_eq!((got.document.revision, got.can_undo), (1, true));
    let path = dir.path().join("documents/doc.json");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\"rndr-canvas\"", "\"future\"");
    std::fs::write(&path, text).unwrap();
    assert_eq!(store.get("doc"), Err(CanvasError::Unsupported));
    assert!(store.get("../escape").is_err());
}

#[test]
fn concurrent_writers_never_lose_an_edit() {
    let dir = tempfile::tempdir().unwrap();
    create(&FsStore::open(dir.path()).unwrap());
    let threads: Vec<_> = (0..8)
        .map(|i| {
            let root = dir.path().to_path_buf();
            std::thread::spawn(move || {
                let store = FsStore::open(root).unwrap();
                loop {
                    let rev = store.get("doc").unwrap().document.revision;
                    let op = Operation::put(rect(&format!("n{i}"), i as f64));
                    match store.transact("doc", tx(&format!("t{i}"), rev, vec![op])) {
                        Ok(_) => break,
                        Err(CanvasError::Conflict(_)) => continue,
                        Err(e) => panic!("{e}"),
                    }
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let doc = FsStore::open(dir.path())
        .unwrap()
        .get("doc")
        .unwrap()
        .document;
    assert_eq!((doc.revision, doc.scene.nodes.len()), (8, 8));
}
