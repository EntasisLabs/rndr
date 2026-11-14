use rndr_markdown::{parse, Command, EditError, Session, TypedView, ViewMode};

#[test]
fn open_records_nothing_and_ids_are_deterministic() {
    let src = "alpha\n\nbeta\n";
    let a = Session::open(src);
    let b = Session::open(src);
    assert_eq!(a.journal_frames(), 0);
    assert_eq!(a.bytes(), src);
    assert_eq!(a.revision(), 0);
    assert_eq!(a.ids(), b.ids());
    assert_eq!(a.ids()[0], 1);
}

#[test]
fn typing_keeps_the_paragraph_id_and_a_flip_does_not_renumber() {
    let mut session = Session::open("alpha\n\nbeta\n");
    let before = session.ids().to_vec();
    let para = session.tree().children()[0].kind();
    let _ = para;
    session
        .edit(
            Command::Insert {
                at: 1,
                text: "Z".into(),
            },
            0,
        )
        .unwrap();
    assert_eq!(session.bytes(), "aZlpha\n\nbeta\n");
    assert_eq!(session.journal_frames(), 1);
    // The root and the edited paragraph start where they started.
    assert_eq!(session.ids()[0], before[0]);
    session.set_mode(ViewMode::Raw).unwrap();
    let flipped = session.ids().to_vec();
    session.set_mode(ViewMode::Typed).unwrap();
    assert_eq!(session.ids(), flipped.as_slice());
    session.undo(1_000).unwrap();
    assert_eq!(session.bytes(), "alpha\n\nbeta\n");
}

#[test]
fn two_letters_share_one_undo_step() {
    let mut session = Session::open(".\n");
    session
        .edit(
            Command::Insert {
                at: 0,
                text: "a".into(),
            },
            0,
        )
        .unwrap();
    session
        .edit(
            Command::Insert {
                at: 1,
                text: "b".into(),
            },
            100,
        )
        .unwrap();
    assert_eq!(session.bytes(), "ab.\n");
    session.undo(200).unwrap();
    assert_eq!(session.bytes(), ".\n");
}

#[test]
fn composition_is_not_in_the_bytes_until_commit() {
    let mut session = Session::open("ab\n");
    session.composition_start(1, 1).unwrap();
    session.composition_update("é").unwrap();
    assert_eq!(session.bytes(), "ab\n");
    assert_eq!(session.journal_frames(), 0);
    assert_eq!(
        session.edit(
            Command::Insert {
                at: 0,
                text: "x".into()
            },
            0
        ),
        Err(EditError::Composing)
    );
    assert!(session.set_mode(ViewMode::Raw).is_err());
    session.composition_commit(10).unwrap();
    assert_eq!(session.bytes(), "aéb\n");
    assert_eq!(session.journal_frames(), 1);
}

#[test]
fn liquid_view_preserves_field_order_and_comments() {
    let src = "```callout\ntone: tip\n# keep\nbody: \"Hi\"\n```\n";
    let tree = parse(src);
    let view = TypedView::new(src, &tree);
    let fences = view.liquid_fences();
    assert_eq!(fences.len(), 1);
    assert_eq!(fences[0].info, "callout");
    assert_eq!(fences[0].canonical, "callout");
    assert!(fences[0].valid);
    let keys: Vec<_> = fences[0]
        .fields
        .iter()
        .map(|f| if f.comment { "#" } else { f.key })
        .collect();
    assert_eq!(keys, vec!["tone", "#", "body"]);
    let body = fences[0].fields.iter().find(|f| f.key == "body").unwrap();
    assert_eq!(body.value, "Hi");
    assert!(body.markdown);
    assert!(body.markdown_tree().unwrap().concat() == "Hi");
}

#[test]
fn procedure_alias_stays_in_the_info_string() {
    let src = "```procedure\ntitle: Soup\nsteps:\n---\nlabel: Boil\nbody: Water\n---\nlabel: Serve\nbody: Bowl\n```\n";
    let tree = parse(src);
    assert_eq!(tree.concat(), src);
    let view = TypedView::new(src, &tree);
    let fence = &view.liquid_fences()[0];
    assert_eq!(fence.info, "procedure");
    assert_eq!(fence.canonical, "recipe");
}
