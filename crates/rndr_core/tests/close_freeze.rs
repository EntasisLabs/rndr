//! Opt-in F5 writer freeze compatibility, no native event-loop certification.
use rndr_core::editing::{
    EditError, EditOrigin, Revision, SourceBatch, SourceChange, SourceDocuments,
};
use rndr_core::structured::{StructuredDocument, StructuredError};
use rndr_core::structured_native::StructuredDocuments;
#[test]
fn source_freeze_is_owned_nested_and_not_readonly_policy_or_history_mutation() {
    let mut docs = SourceDocuments::default();
    let id = docs.open("original\r\nα");
    let d = docs.get_mut(id).unwrap();
    let a = d.freeze().unwrap();
    let b = d.freeze().unwrap();
    let before = (
        d.text().to_owned(),
        d.revision(),
        d.history_retained_bytes(),
        d.selection(),
    );
    d.set_readonly(false).unwrap();
    assert_eq!(
        d.commit(SourceBatch {
            document: id,
            base: Revision(0),
            origin: EditOrigin::Host(1),
            changes: vec![SourceChange {
                range: 0..0,
                replacement: "X".into()
            }]
        }),
        Err(EditError::Readonly)
    );
    assert_eq!(d.undo(), Err(EditError::Readonly));
    assert_eq!(d.redo(), Err(EditError::Readonly));
    assert_eq!(d.reload(Revision(0), "reload"), Err(EditError::Readonly));
    assert_eq!(
        (
            d.text().to_owned(),
            d.revision(),
            d.history_retained_bytes(),
            d.selection()
        ),
        before
    );
    drop(a);
    assert!(d.is_readonly());
    drop(b);
    assert!(!d.is_readonly());
    d.reload(Revision(0), "allowed").unwrap();
}
#[test]
fn structured_freeze_blocks_all_writer_paths_and_manager_removal_preserving_state() {
    let mut docs = StructuredDocuments::default();
    let id = docs.insert(StructuredDocument::new("first\nsecond"));
    let d = docs.get_mut(id).unwrap();
    d.replace_selection("changed", EditOrigin::Host(1)).unwrap();
    d.undo().unwrap();
    let before = (d.id(), d.revision(), d.plain_text(), d.selection());
    let a = d.freeze().unwrap();
    let b = d.freeze().unwrap();
    d.set_readonly(false);
    assert!(d.is_readonly());
    assert_eq!(
        d.replace_selection("agent", EditOrigin::Host(2)),
        Err(StructuredError::Readonly)
    );
    assert_eq!(d.undo(), Err(StructuredError::Readonly));
    assert_eq!(d.redo(), Err(StructuredError::Readonly));
    assert_eq!(d.begin_composition(), Err(StructuredError::Readonly));
    assert_eq!(
        (d.id(), d.revision(), d.plain_text(), d.selection()),
        before
    );
    assert!(docs.remove(id).is_none());
    drop(a);
    assert!(docs.get(id).unwrap().is_readonly());
    drop(b);
    assert!(!docs.get(id).unwrap().is_readonly());
    assert!(docs.remove(id).is_some());
}
#[test]
fn structured_composition_is_not_silently_discarded_by_freeze() {
    let mut d = StructuredDocument::new("IME");
    d.begin_composition().unwrap();
    assert!(d.freeze().is_err());
    assert!(d.is_composing());
    d.commit_composition("確定").unwrap();
    let f = d.freeze().unwrap();
    assert!(d.is_readonly());
    drop(f);
    assert!(!d.is_readonly());
}
