//! Promotion gates for the supported staged projection contract.
use rndr_core::editing::*;
use rndr_core::source_projection::*;
use std::error::Error;

fn fixture(raw: &str) -> (SourceDocuments, DocumentId, Projection) {
    let mut docs = SourceDocuments::default();
    let id = docs.open(raw);
    let projection = Projection::build(&docs.get(id).unwrap().snapshot());
    (docs, id, projection)
}
const CASES: &[&str] = &[
    "",
    "\u{feff}",
    "hello",
    "*é*",
    "A *bc* z  \r\n",
    "a\rb",
    "a\r\r\nb",
    "- [X] *字*\r\n",
    "- [ ] ",
    "\u{feff}- [x] café مرحبا 👩‍💻\r\n",
    "```alien\r\n\u{feff}raw\t \r\n```\r\nend",
    "~~~open\nraw",
    "<unknown>\nplain",
    "\n\r\n",
    "a\u{301} z",
    "**unsupported**",
];

#[test]
fn inverse_maps_every_source_scalar_exactly_or_explicitly_reveals_an_atom() {
    for raw in CASES {
        let (docs, id, p) = fixture(raw);
        let before = docs.get(id).unwrap().snapshot();
        for at in 0..=raw.len() {
            let result = p.point_from_source(at);
            if !raw.is_char_boundary(at) {
                assert_eq!(result, Err(ProjectionError::InvalidPoint));
                continue;
            }
            match result {
                Ok(point) => {
                    assert_eq!(p.point_to_source(point), Ok(at), "{raw:?} at {at}");
                    let source = SourceSelection {
                        anchor: at,
                        head: 0,
                    };
                    let display = p.selection_from_source(source).unwrap();
                    assert_eq!(p.selection_to_source(display).unwrap(), source);
                }
                Err(ProjectionError::RevealSource(range)) => {
                    assert!(range.start < at && at < range.end);
                    assert!(p.segments().iter().any(|s| s.source == range
                        && matches!(s.kind, SegmentKind::Task | SegmentKind::Opaque)));
                }
                error => panic!("unexpected {error:?} in {raw:?} at {at}"),
            }
        }
        assert_eq!(
            p.point_from_source(usize::MAX),
            Err(ProjectionError::InvalidPoint)
        );
        assert_eq!(docs.get(id).unwrap().snapshot(), before);
    }
}

#[test]
fn captured_target_lifecycle_is_checked_and_noop_does_not_consume_redo() {
    let (mut docs, id, p) = fixture("A *quiet* line\r\n");
    let other = docs.open("A *quiet* line\r\n");
    let d = docs.get_mut(id).unwrap();
    let target = p.capture(d, ProjectionSelection::inward(7, 2)).unwrap();
    assert_eq!(target.document(), id);
    assert_eq!(target.revision(), d.revision());
    assert_eq!(target.original_text(), "quiet");
    assert_eq!(target.selection(), ProjectionSelection::inward(7, 2));
    assert_eq!(d.drain_events().count(), 0);
    target.replace(d, "calm", EditOrigin::Host(1)).unwrap();
    assert_eq!(target.validate(d), Err(EditError::StaleRevision.into()));
    d.undo().unwrap();
    assert_eq!(target.validate(d), Err(EditError::StaleRevision.into()));
    let p = Projection::build(&d.snapshot());
    let current = p.capture(d, ProjectionSelection::inward(2, 7)).unwrap();
    assert!(current
        .replace(d, current.original_text(), EditOrigin::Host(1))
        .unwrap()
        .is_none());
    current.validate(d).unwrap();
    d.redo().unwrap();
    assert_eq!(d.text(), "A *calm* line\r\n");
    assert_eq!(current.validate(d), Err(EditError::StaleRevision.into()));
    assert_eq!(
        target.replace(docs.get_mut(other).unwrap(), "other", EditOrigin::Host(1)),
        Err(EditError::MissingDocument.into())
    );
}

#[test]
fn checked_live_copy_and_capture_reject_forged_stale_and_nonliteral_targets() {
    let (mut docs, id, p) = fixture("*quiet*\n<opaque>");
    let d = docs.get_mut(id).unwrap();
    let selection = ProjectionSelection::inward(0, 5);
    let mut forged = d.snapshot();
    forged.text = "*other*\n<opaque>".into();
    let forged = Projection::build(&forged);
    assert!(matches!(
        forged.capture(d, selection),
        Err(ProjectionError::Source(EditError::StaleRevision))
    ));
    assert_eq!(
        forged.copy_live(d, selection, CopyPolicy::Rendered),
        Err(EditError::StaleRevision.into())
    );
    let opaque = p
        .segments()
        .iter()
        .find(|s| s.kind == SegmentKind::Opaque)
        .unwrap();
    let before = d.snapshot();
    assert!(matches!(
        p.capture(
            d,
            ProjectionSelection::inward(opaque.display.start + 1, opaque.display.end)
        ),
        Err(ProjectionError::RevealSource(_))
    ));
    assert_eq!(d.snapshot(), before);
    assert_eq!(
        p.copy_live(d, selection, CopyPolicy::Source).unwrap(),
        "quiet"
    );
    p.replace(d, selection, "calm", EditOrigin::Host(1))
        .unwrap();
    assert!(matches!(
        p.capture(d, selection),
        Err(ProjectionError::Source(EditError::StaleRevision))
    ));
    for policy in [CopyPolicy::Rendered, CopyPolicy::Source] {
        assert_eq!(
            p.copy_live(d, selection, policy),
            Err(EditError::StaleRevision.into())
        );
    }
    // Snapshot copy is explicitly historical, unlike checked live copy.
    assert_eq!(p.copy(selection, CopyPolicy::Source).unwrap(), "quiet");
}

#[test]
fn target_apply_obeys_current_readonly_and_event_backpressure_without_losing_draft() {
    let (mut docs, id, p) = fixture("hello");
    let d = docs.get_mut(id).unwrap();
    let target = p.capture(d, ProjectionSelection::inward(0, 5)).unwrap();
    d.set_readonly(true).unwrap();
    assert_eq!(
        target.replace(d, "calm", EditOrigin::Host(1)),
        Err(EditError::Readonly.into())
    );
    assert_eq!(d.text(), "hello");
    d.set_readonly(false).unwrap();
    target.replace(d, "calm", EditOrigin::Host(1)).unwrap();
    d.drain_events().for_each(drop);
    for n in 0..EVENT_LIMIT {
        let p = Projection::build(&d.snapshot());
        p.replace(
            d,
            ProjectionSelection::inward(0, d.text().len()),
            if n % 2 == 0 { "one" } else { "two" },
            EditOrigin::Host(1),
        )
        .unwrap();
    }
    let target = Projection::build(&d.snapshot())
        .capture(d, ProjectionSelection::inward(0, 3))
        .unwrap();
    let before = d.snapshot();
    assert_eq!(
        target.replace(d, "three", EditOrigin::Host(1)),
        Err(EditError::EventBackpressure.into())
    );
    assert_eq!(d.snapshot(), before);
    assert_eq!(target.original_text(), "two");
    assert_eq!(d.drain_events().count(), EVENT_LIMIT);
    target.replace(d, "three", EditOrigin::Host(1)).unwrap();
    assert_eq!(d.text(), "three");
}

#[test]
fn exhaustive_endpoint_matrix_splices_exactly_or_refuses_without_side_effects() {
    let mut admitted = 0;
    let mut refused = 0;
    for raw in CASES {
        let (_, _, template) = fixture(raw);
        let endpoints: Vec<_> = (0..=template.text().len())
            .filter(|at| template.text().is_char_boundary(*at))
            .collect();
        for &a in &endpoints {
            for &b in &endpoints {
                for replacement in ["", "X", "é", "日本", "- ", "\n", "*", "\u{0}"] {
                    let (mut docs, id, p) = fixture(raw);
                    let d = docs.get_mut(id).unwrap();
                    let before = d.snapshot();
                    let before_selection = d.selection();
                    let selection = ProjectionSelection::inward(a, b);
                    let planned = p.replacement_batch(selection, replacement, EditOrigin::Host(5));
                    let result = p.replace(d, selection, replacement, EditOrigin::Host(5));
                    match planned {
                        Ok(batch) => {
                            admitted += 1;
                            let change = &batch.changes[0];
                            let mut expected = raw.to_string();
                            expected.replace_range(change.range.clone(), replacement);
                            result.unwrap();
                            assert_eq!(d.text(), expected);
                            if expected != *raw {
                                let events: Vec<_> = d.drain_events().collect();
                                assert_eq!(events.len(), 1);
                                assert_eq!(events[0].changes, batch.changes);
                                d.undo().unwrap();
                                assert_eq!(d.text(), *raw);
                                d.redo().unwrap();
                                assert_eq!(d.text(), expected);
                            } else {
                                assert_eq!(d.snapshot(), before);
                                assert_eq!(d.drain_events().count(), 0);
                            }
                        }
                        Err(error) => {
                            refused += 1;
                            assert_eq!(result, Err(error));
                            assert_eq!(d.snapshot(), before);
                            assert_eq!(d.selection(), before_selection);
                            assert_eq!(d.drain_events().count(), 0);
                            assert!(d.undo().unwrap().is_none());
                        }
                    }
                }
            }
        }
    }
    assert!(admitted > 100);
    assert!(refused > 1000);
    eprintln!("endpoint matrix: {admitted} admitted, {refused} refused");
}

#[test]
fn projection_errors_integrate_with_standard_error_reporting() {
    let source = ProjectionError::Source(EditError::Readonly);
    assert_eq!(source.source().unwrap().to_string(), "Readonly");
    assert!(source.to_string().contains("source admission"));
    assert_eq!(
        ProjectionError::RevealSource(3..9).to_string(),
        "edit source bytes 3..9 instead"
    );
    assert!(ProjectionError::InvalidPoint.source().is_none());
}
