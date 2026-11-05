use rndr_core::editing::*;
use rndr_core::source_projection::*;

const RAW: &str = "\u{feff}A *quiet* line  \r\n- [ ] café مرحبا 👩‍💻\r\n```alien\r\n{ raw:  unknown }  \r\n```\r\n<custom untouched=\"yes\">\r\n";
fn fixture(text: &str) -> (SourceDocuments, DocumentId, Projection) {
    let mut docs = SourceDocuments::default();
    let id = docs.open(text);
    let p = Projection::build(&docs.get(id).unwrap().snapshot());
    (docs, id, p)
}
fn sel(a: usize, b: usize) -> ProjectionSelection {
    ProjectionSelection::inward(a, b)
}
fn point(offset: usize, affinity: Affinity) -> ProjectionPoint {
    ProjectionPoint { offset, affinity }
}

#[test]
fn build_preview_selection_copy_do_not_dirty_and_partition_exact_source() {
    let (mut docs, id, p) = fixture(RAW);
    let d = docs.get_mut(id).unwrap();
    let before = d.snapshot();
    assert!(p.text().starts_with("A quiet line  \r\n[ ] café"));
    assert_eq!(
        p.segments()
            .iter()
            .filter(|s| s.kind == SegmentKind::Opaque)
            .count(),
        2
    );
    let mut at = 0;
    for s in p.segments() {
        assert_eq!(s.source.start, at);
        at = s.source.end;
    }
    assert_eq!(at, RAW.len());
    let a = p.text().find("quiet").unwrap();
    assert_eq!(p.copy(sel(a, a + 5), CopyPolicy::Source).unwrap(), "quiet");
    let whole = ProjectionSelection {
        anchor: point(0, Affinity::Before),
        head: point(p.text().len(), Affinity::After),
    };
    assert_eq!(p.copy(whole, CopyPolicy::Source).unwrap(), RAW);
    assert_eq!(d.snapshot(), before);
    assert_eq!(d.drain_events().count(), 0);
    assert!(d.undo().unwrap().is_none());
}

#[test]
fn exact_changed_literal_and_task_preserve_all_other_bytes_with_shared_undo() {
    let (mut docs, id, p) = fixture(RAW);
    let d = docs.get_mut(id).unwrap();
    let a = p.text().find("quiet").unwrap();
    let e = p
        .replace(d, sel(a, a + 5), "calm", EditOrigin::Keyboard)
        .unwrap()
        .unwrap();
    assert_eq!(d.text(), RAW.replacen("quiet", "calm", 1));
    assert_eq!(
        e.changes,
        vec![SourceChange {
            range: 6..11,
            replacement: "calm".into()
        }]
    );
    let p = Projection::build(&d.snapshot());
    let task = p
        .segments()
        .iter()
        .find(|s| s.kind == SegmentKind::Task)
        .unwrap()
        .display
        .start;
    p.toggle_task(d, task, EditOrigin::Host(42)).unwrap();
    assert_eq!(
        d.text(),
        RAW.replacen("quiet", "calm", 1).replacen("[ ]", "[x]", 1)
    );
    d.undo().unwrap();
    d.undo().unwrap();
    assert_eq!(d.text(), RAW);
    d.redo().unwrap();
    d.redo().unwrap();
    assert_eq!(d.revision(), Revision(6));
}

#[test]
fn hidden_delimiter_boundaries_have_two_affinities_and_reversed_mapping() {
    let (_, _, p) = fixture("a *é* z");
    let a = p.text().find('é').unwrap();
    assert_eq!(p.point_to_source(point(a, Affinity::Before)), Ok(2));
    assert_eq!(p.point_to_source(point(a, Affinity::After)), Ok(3));
    assert_eq!(p.point_to_source(point(a + 2, Affinity::Before)), Ok(5));
    assert_eq!(p.point_to_source(point(a + 2, Affinity::After)), Ok(6));
    assert_eq!(
        p.selection_to_source(sel(a + 2, a)).unwrap(),
        SourceSelection { anchor: 5, head: 3 }
    );
    assert_eq!(p.copy(sel(a + 2, a), CopyPolicy::Source).unwrap(), "é");
    assert_eq!(
        p.point_to_source(point(a + 1, Affinity::After)),
        Err(ProjectionError::InvalidPoint)
    );
    assert_eq!(
        p.point_to_source(point(usize::MAX, Affinity::Before)),
        Err(ProjectionError::InvalidPoint)
    );
}

#[test]
fn inserts_and_deletes_inside_emphasis_leave_delimiters_untouched() {
    for (a, b, replacement, expected) in [
        (0, 0, "Q", "*Qabc*"),
        (3, 3, "Q", "*abcQ*"),
        (0, 1, "", "*bc*"),
        (2, 3, "", "*ab*"),
    ] {
        let (mut docs, id, p) = fixture("*abc*");
        let selection = if a == b {
            ProjectionSelection {
                anchor: point(
                    a,
                    if a == 0 {
                        Affinity::After
                    } else {
                        Affinity::Before
                    },
                ),
                head: point(
                    b,
                    if a == 0 {
                        Affinity::After
                    } else {
                        Affinity::Before
                    },
                ),
            }
        } else {
            sel(a, b)
        };
        p.replace(
            docs.get_mut(id).unwrap(),
            selection,
            replacement,
            EditOrigin::Keyboard,
        )
        .unwrap();
        assert_eq!(docs.get(id).unwrap().text(), expected);
    }
}

#[test]
fn unsupported_crossings_syntax_empty_emphasis_and_newline_edits_reveal_without_mutation() {
    for (text, selection, replacement) in [
        ("a *bc* d", sel(0, 5), "X"),
        ("*bc*", sel(0, 2), ""),
        ("a\r\nb", sel(1, 3), ""),
        ("hello", sel(0, 0), "- "),
        ("hello", sel(2, 2), "*"),
        ("hello", sel(2, 2), "\n"),
    ] {
        let (mut docs, id, p) = fixture(text);
        let d = docs.get_mut(id).unwrap();
        let before = d.snapshot();
        let selection_before = d.selection();
        assert!(
            matches!(
                p.replace(d, selection, replacement, EditOrigin::Keyboard),
                Err(ProjectionError::RevealSource(_))
            ),
            "{text:?}"
        );
        assert_eq!(d.snapshot(), before);
        assert_eq!(d.selection(), selection_before);
        assert_eq!(d.drain_events().count(), 0);
    }
}

#[test]
fn opaque_atoms_require_source_reveal_and_reversed_cross_atom_copy_is_explicit() {
    let (mut docs, id, p) = fixture("before\n```x\nraw\n```\nafter");
    let opaque = p
        .segments()
        .iter()
        .find(|s| s.kind == SegmentKind::Opaque)
        .unwrap();
    let inside = opaque.display.start + 1;
    assert_eq!(
        p.point_to_source(point(inside, Affinity::After)),
        Err(ProjectionError::RevealSource(opaque.source.clone()))
    );
    let whole = sel(p.text().len(), 0);
    assert_eq!(
        p.copy(whole, CopyPolicy::Source).unwrap(),
        "before\n```x\nraw\n```\nafter"
    );
    assert!(p
        .copy(whole, CopyPolicy::Rendered)
        .unwrap()
        .contains("Unsupported source"));
    assert!(matches!(
        p.replace(docs.get_mut(id).unwrap(), whole, "", EditOrigin::Keyboard),
        Err(ProjectionError::RevealSource(_))
    ));
}

#[test]
fn stale_external_undo_and_wrong_identity_refuse_even_when_display_matches() {
    let (mut docs, id, p) = fixture("*same*");
    let other = docs.open("*same*");
    assert_eq!(
        p.replace(
            docs.get_mut(other).unwrap(),
            sel(0, 1),
            "S",
            EditOrigin::Keyboard
        ),
        Err(ProjectionError::Source(EditError::MissingDocument))
    );
    let d = docs.get_mut(id).unwrap();
    d.commit(SourceBatch {
        document: id,
        base: d.revision(),
        origin: EditOrigin::Host(1),
        changes: vec![SourceChange {
            range: 1..5,
            replacement: "different".into(),
        }],
    })
    .unwrap();
    d.undo().unwrap();
    assert_eq!(d.text(), "*same*");
    assert_eq!(
        p.replace(d, sel(0, 1), "S", EditOrigin::Keyboard),
        Err(ProjectionError::Source(EditError::StaleRevision))
    );
    assert_eq!(d.text(), "*same*");
}

#[test]
fn composition_is_local_cancelable_and_commit_is_one_source_history_entry() {
    let (mut docs, id, p) = fixture("A *café* مرحبا");
    let a = p.text().find("café").unwrap();
    let d = docs.get_mut(id).unwrap();
    let before = d.snapshot();
    let mut canceled = p.begin_composition(sel(a, a + 5)).unwrap();
    canceled.set_preedit("に");
    assert_eq!(d.snapshot(), before);
    drop(canceled);
    assert_eq!(d.drain_events().count(), 0);
    let mut ime = p.begin_composition(sel(a, a + 5)).unwrap();
    ime.set_preedit("にほ");
    assert_eq!(ime.preedit(), "にほ");
    ime.commit(d, "日本").unwrap();
    assert_eq!(d.text(), "A *日本* مرحبا");
    assert_eq!(d.drain_events().next().unwrap().origin, EditOrigin::Ime);
    d.undo().unwrap();
    assert_eq!(d.text(), before.text);
}

#[test]
fn composition_external_change_stale_and_readonly_refuse() {
    let (mut docs, id, p) = fixture("hello");
    let d = docs.get_mut(id).unwrap();
    let ime = p.begin_composition(sel(0, 1)).unwrap();
    d.commit(SourceBatch {
        document: id,
        base: d.revision(),
        origin: EditOrigin::Host(1),
        changes: vec![SourceChange {
            range: 0..0,
            replacement: "X".into(),
        }],
    })
    .unwrap();
    assert_eq!(
        ime.commit(d, "Y"),
        Err(ProjectionError::Source(EditError::StaleRevision))
    );
    let p = Projection::build(&d.snapshot());
    d.set_readonly(true).unwrap();
    assert_eq!(
        p.replace(d, sel(0, 1), "Y", EditOrigin::Keyboard),
        Err(ProjectionError::Source(EditError::Readonly))
    );
}

#[test]
fn conservative_grammar_fences_unknown_and_empty_document() {
    for text in [
        "**strong**",
        "escaped \\*x*",
        "[link](url)",
        "_em_",
        "# title",
        "<tag>",
        "  code",
        "1. list",
        "~~~foo\nopaque\n~~~~\n",
        "```open\nno close",
    ] {
        let (_, _, p) = fixture(text);
        assert!(
            p.segments().iter().any(|s| s.kind == SegmentKind::Opaque),
            "{text}"
        );
    }
    let (mut docs, id, p) = fixture("");
    p.replace(
        docs.get_mut(id).unwrap(),
        sel(0, 0),
        "hello",
        EditOrigin::Keyboard,
    )
    .unwrap();
    assert_eq!(docs.get(id).unwrap().text(), "hello");
}

#[test]
fn noop_preserves_redo_and_ambiguous_collapsed_syntax_selection_refuses() {
    let (mut docs, id, p) = fixture("*abc*");
    let d = docs.get_mut(id).unwrap();
    p.replace(d, sel(1, 2), "B", EditOrigin::Keyboard).unwrap();
    d.undo().unwrap();
    let p = Projection::build(&d.snapshot());
    let before = d.snapshot();
    let selection = d.selection();
    assert!(p
        .replace(d, sel(0, 3), "abc", EditOrigin::Keyboard)
        .unwrap()
        .is_none());
    assert_eq!(d.snapshot(), before);
    assert_eq!(d.selection(), selection);
    let opposite = ProjectionSelection {
        anchor: point(0, Affinity::Before),
        head: point(0, Affinity::After),
    };
    assert!(matches!(
        p.replace(d, opposite, "", EditOrigin::Keyboard),
        Err(ProjectionError::RevealSource(_))
    ));
    d.redo().unwrap();
    assert_eq!(d.text(), "*aBc*");
}

#[test]
fn task_atomic_interior_and_bom_only_insertion_are_explicit() {
    let (mut docs, id, p) = fixture("- [X] hello");
    let d = docs.get_mut(id).unwrap();
    assert_eq!(
        p.point_to_source(point(1, Affinity::After)),
        Err(ProjectionError::RevealSource(0..6))
    );
    assert_eq!(
        p.toggle_task(d, usize::MAX, EditOrigin::Keyboard),
        Err(ProjectionError::InvalidPoint)
    );
    assert_eq!(d.revision(), Revision(0));
    let (mut docs, id, p) = fixture("\u{feff}");
    assert_eq!(p.point_to_source(point(0, Affinity::Before)), Ok(0));
    assert_eq!(p.point_to_source(point(0, Affinity::After)), Ok(3));
    p.replace(
        docs.get_mut(id).unwrap(),
        sel(0, 0),
        "é",
        EditOrigin::Keyboard,
    )
    .unwrap();
    assert_eq!(docs.get(id).unwrap().text(), "\u{feff}é");
}

#[test]
fn forged_snapshot_and_reload_do_not_reuse_a_matching_display() {
    let (mut docs, id, p) = fixture("hello");
    let d = docs.get_mut(id).unwrap();
    let mut forged = d.snapshot();
    forged.text = "HELLO".into();
    let bad = Projection::build(&forged);
    assert_eq!(
        bad.replace(d, sel(0, 1), "h", EditOrigin::Keyboard),
        Err(ProjectionError::Source(EditError::StaleRevision))
    );
    d.reload(d.revision(), "hello").unwrap();
    // An identical reload may be a no-op; check a real external replacement.
    d.reload(d.revision(), "other").unwrap();
    assert_eq!(
        p.replace(d, sel(0, 1), "H", EditOrigin::Keyboard),
        Err(ProjectionError::Source(EditError::StaleRevision))
    );
}
