//! Inline marks and atomic mention pills in the structured model.
use rndr_core::editing::{Affinity, EditOrigin};
use rndr_core::structured::{
    MarkKind, Position, StructuredDocument, StructuredError, StructuredSelection, TextSelection,
};

fn select(doc: &mut StructuredDocument, anchor: usize, head: usize) {
    let at = |doc: &StructuredDocument, n| doc.position_at(n, Affinity::After).unwrap();
    let selection = TextSelection {
        anchor: at(doc, anchor),
        head: at(doc, head),
    };
    doc.set_selection(StructuredSelection::Text(selection))
        .unwrap();
}

fn caret(doc: &StructuredDocument) -> usize {
    let selection = doc.text_selection().unwrap();
    doc.display_offset(selection.head).unwrap()
}

fn mention(doc: &mut StructuredDocument) {
    select(doc, 3, 3);
    assert!(doc.insert_pill("@alice", "user:1", EditOrigin::Host(0)).unwrap());
}

const API: EditOrigin = EditOrigin::Host(0);

#[test]
fn toggle_marks_keep_other_marks_per_run() {
    let mut doc = StructuredDocument::new("one two three");
    select(&mut doc, 0, 7);
    doc.toggle_mark(MarkKind::Strong, API).unwrap();
    select(&mut doc, 4, 13);
    doc.toggle_mark(MarkKind::Strike, API).unwrap();
    let runs = &doc.blocks()[0].runs;
    let text: Vec<_> = runs
        .iter()
        .map(|r| (r.text.as_str(), r.marks.strong, r.marks.strike))
        .collect();
    assert_eq!(
        text,
        [("one ", true, false), ("two", true, true), (" three", false, true)]
    );
    select(&mut doc, 4, 13);
    doc.toggle_mark(MarkKind::Strike, API).unwrap();
    assert!(doc.blocks()[0].runs.iter().all(|r| !r.marks.strike));
    select(&mut doc, 0, 3);
    doc.toggle_mark(MarkKind::Code, API).unwrap();
    doc.toggle_mark(MarkKind::Underline, API).unwrap();
    let first = &doc.blocks()[0].runs[0];
    assert!(first.marks.code && first.marks.underline && first.marks.strong);
}

#[test]
fn pills_are_atomic_for_positions_and_typing() {
    let mut doc = StructuredDocument::new("hi  there");
    mention(&mut doc);
    assert_eq!(doc.plain_text(), "hi @alice there");
    assert_eq!(caret(&doc), 9);
    let pill = &doc.blocks()[0].runs[1];
    assert_eq!(pill.text, "@alice");
    assert_eq!(pill.marks.pill.as_deref(), Some("user:1"));
    for inside in 4..9 {
        assert_eq!(
            doc.position_at(inside, Affinity::After),
            Err(StructuredError::InvalidPosition)
        );
    }
    doc.replace_selection("!", API).unwrap();
    assert_eq!(doc.plain_text(), "hi @alice! there");
    let runs = &doc.blocks()[0].runs;
    assert_eq!(runs[1].text, "@alice");
    assert!(runs[2].marks.pill.is_none());
}

#[test]
fn backspace_and_delete_remove_a_whole_pill() {
    let mut doc = StructuredDocument::new("hi  there");
    mention(&mut doc);
    doc.backspace(API).unwrap();
    assert_eq!(doc.plain_text(), "hi  there");
    assert_eq!(caret(&doc), 3);
    doc.undo().unwrap();
    select(&mut doc, 3, 3);
    doc.delete_forward(API).unwrap();
    assert_eq!(doc.plain_text(), "hi  there");
}

#[test]
fn adjacent_equal_pills_stay_separate() {
    let mut doc = StructuredDocument::new("ab");
    select(&mut doc, 1, 1);
    doc.insert_pill("@x", "x", API).unwrap();
    doc.insert_pill("@x", "x", API).unwrap();
    let pills: Vec<_> = doc.blocks()[0]
        .runs
        .iter()
        .filter(|r| r.marks.pill.is_some())
        .map(|r| r.text.as_str())
        .collect();
    assert_eq!(pills, ["@x", "@x"]);
    doc.backspace(API).unwrap();
    assert_eq!(doc.plain_text(), "a@xb");
}

#[test]
fn invalid_pill_ids_and_labels_are_refused() {
    let mut doc = StructuredDocument::new("ab");
    select(&mut doc, 1, 1);
    let revision = doc.revision();
    assert_eq!(
        doc.insert_pill("@x", "<script>", API),
        Err(StructuredError::InvalidSchema)
    );
    assert_eq!(
        doc.insert_pill("a\nb", "x", API),
        Err(StructuredError::InvalidSchema)
    );
    assert_eq!(doc.insert_pill("@x", "", API), Err(StructuredError::InvalidSchema));
    assert_eq!(doc.revision(), revision);
    let _ = Position {
        block: doc.blocks()[0].id,
        offset: 0,
        affinity: Affinity::After,
    };
}
