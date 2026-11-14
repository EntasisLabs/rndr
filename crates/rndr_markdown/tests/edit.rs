use rndr_markdown::{apply_text, apply_view, parse, Command, EditError};

fn round_trip(src: &str, cmd: Command) -> rndr_markdown::Applied {
    let tree = parse(src);
    let applied = apply_view(src, &tree, &cmd).unwrap();
    let via_text = apply_text(src, &applied.splices);
    assert_eq!(applied.bytes, via_text);
    assert_eq!(
        parse(&via_text),
        applied.tree,
        "incremental diverged for {cmd:?} on {src:?}"
    );
    assert_eq!(applied.tree.concat(), applied.bytes);
    applied
}

#[test]
fn insert_in_a_middle_block_stays_local() {
    let src = "alpha\n\nbeta\n\ngamma\n";
    let applied = round_trip(
        src,
        Command::Insert {
            at: 0,
            text: "X".into(),
        },
    );
    assert!(
        !applied.widened_to_root,
        "a letter in the first paragraph widened to the root"
    );
    assert!(applied.bytes.starts_with("Xalpha\n"));
    assert!(applied.bytes.ends_with("gamma\n"));
    let old = parse(src);
    let new = &applied.tree;
    assert_eq!(
        new.children().last().unwrap().concat(),
        old.children().last().unwrap().concat()
    );
}

#[test]
fn open_without_an_edit_changes_nothing() {
    let src = "alpha\n\nbeta\n";
    let tree = parse(src);
    let applied = apply_view(
        src,
        &tree,
        &Command::Insert {
            at: 0,
            text: String::new(),
        },
    )
    .unwrap();
    assert!(applied.splices.is_empty());
    assert_eq!(applied.bytes, src);
    assert_eq!(applied.tree, tree);
}

#[test]
fn underscore_emphasis_is_toggled_with_underscores() {
    let src = "say _a_ please\n";
    let start = src.find("_a_").unwrap() + 1;
    let end = start + 1;
    let applied = round_trip(src, Command::ToggleEmphasis { start, end });
    assert_eq!(applied.bytes, "say a please\n");
    let again = round_trip(
        "say a please\n",
        Command::ToggleEmphasis { start: 4, end: 5 },
    );
    assert_eq!(again.bytes, "say *a* please\n");
}

#[test]
fn literal_star_is_escaped() {
    let src = "a*\n";
    let applied = round_trip(
        src,
        Command::Insert {
            at: 0,
            text: "*".into(),
        },
    );
    assert_eq!(applied.bytes, "\\*a*\n");
}

#[test]
fn field_value_splice_keeps_the_key() {
    let src = "```callout\ntone: tip\n# keep\nbody: \"Hi\"\n```\n";
    let value = src.find("Hi").unwrap();
    let applied = round_trip(
        src,
        Command::SetField {
            start: value,
            end: value + 2,
            value: "Yo".into(),
        },
    );
    assert_eq!(
        applied.bytes,
        "```callout\ntone: tip\n# keep\nbody: \"Yo\"\n```\n"
    );
}

#[test]
fn hazards_survive_a_local_insert() {
    for src in [
        "Foo\n---\n\nbar\n",
        "- a\nb\n\nnext\n",
        "- a\n\n- b\n\nnext\n",
        "<div>\n\nfoo\n",
        "<script>\nfoo\n\nbar\n</script>\n\nafter\n",
        "[foo]: /url\n\n[foo]\n\nmore\n",
        "```callout\ntone: tip\nbody: Hi\n```\n\nafter\n",
    ] {
        let at = src.find("a").or_else(|| src.find("F")).unwrap_or(0);
        round_trip(
            src,
            Command::Insert {
                at,
                text: "Z".into(),
            },
        );
    }
}

#[test]
fn bad_boundary_is_refused() {
    let src = "é\n";
    let tree = parse(src);
    let err = apply_view(
        src,
        &tree,
        &Command::Insert {
            at: 1,
            text: "x".into(),
        },
    )
    .unwrap_err();
    assert_eq!(err, EditError::InvalidRange);
    assert_eq!(parse(src).concat(), src);
}
