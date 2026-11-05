//! E7 source-custody byte goldens, independent of any Markdown serializer.
use editing::{EditOrigin, SourceDocuments};
use rndr_core::editing;
use rndr_core::source_projection::{CopyPolicy, Projection, ProjectionSelection};

#[test]
fn changed_literal_preserves_bom_crlf_delimiters_and_unknown_block_with_exact_undo() {
    let raw = "\u{feff}*old*  \r\n\r\n__keep__\t  \r\n\r\n~~~mystery\r\n  @raw  \t\r\n~~~\r\n";
    let mut docs = SourceDocuments::default();
    let id = docs.open(raw);
    let session = docs.get_mut(id).unwrap();
    let projection = Projection::build(&session.snapshot());
    let a = projection.text().find("old").unwrap();
    let selection = ProjectionSelection::inward(a, a + 3);
    assert_eq!(
        projection.copy(selection, CopyPolicy::Rendered).unwrap(),
        "old"
    );
    projection
        .replace(session, selection, "new", EditOrigin::Host(71))
        .unwrap();
    assert_eq!(session.text(), raw.replacen("old", "new", 1));
    assert!(projection
        .replace(session, selection, "bad", EditOrigin::Host(71))
        .is_err());
    session.undo().unwrap();
    assert_eq!(session.text().as_bytes(), raw.as_bytes());
    // Undo advances the authority revision; old projections do not revive.
    assert!(projection.validate(session).is_err());
}

#[test]
fn task_toggle_and_reverse_selection_do_not_normalize_untouched_bytes() {
    let raw = "\u{feff}- [X] *hello*  \r\n~~~custom\r\n @raw \t\r\n~~~\r\n";
    let mut docs = SourceDocuments::default();
    let id = docs.open(raw);
    let session = docs.get_mut(id).unwrap();
    let projection = Projection::build(&session.snapshot());
    let a = projection.text().find("hello").unwrap();
    let selection = ProjectionSelection::inward(a + 5, a);
    let mapped = projection.selection_to_source(selection).unwrap();
    assert!(mapped.anchor > mapped.head);
    assert_eq!(
        projection.copy(selection, CopyPolicy::Source).unwrap(),
        "hello"
    );
    projection
        .toggle_task(session, 0, EditOrigin::Host(72))
        .unwrap();
    assert_eq!(session.text(), raw.replacen("[X]", "[ ]", 1));
    session.undo().unwrap();
    assert_eq!(session.text(), raw);
}
