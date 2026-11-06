//! Executable document-editing recipes and explicit authority boundaries.
//!
//! These additive APIs are supported within their documented bounded subsets;
//! experimental source/tree namespaces are not a blanket stable-API ratification.
//! Existing `Buffer`, `Editor`, and `edit_buffer` remain at the crate root.
//! The desktop facade reexports this module unchanged; headless users need only
//! `rndr_core`. No UI plugin, window, GPU, async executor or parser setup is needed.
//! The crate still has its pre-existing Markdown parser dependency for previews.
//!
//! # Source: exact bytes, one revision/history owner
//!
//! ```
//! use rndr_core::editing::{EditOrigin, SourceBatch, SourceChange, SourceDocuments,
//!     SourceSelection};
//! let raw = "\u{feff}A *quiet* line  \r\n```unknown\r\nraw\r\n```\r\n";
//! let mut docs = SourceDocuments::default();
//! let id = docs.open(raw);
//! let source = docs.get_mut(id).unwrap();
//! let start = raw.find("quiet").unwrap();
//! let selection = SourceSelection { anchor: start + 5, head: start };
//! source.set_selection(selection).unwrap();
//! let event = source.commit(SourceBatch {
//!     document: id, base: source.revision(), origin: EditOrigin::Host(7),
//!     changes: vec![SourceChange { range: start..start + 5,
//!         replacement: "calm".into() }],
//! }).unwrap().unwrap();
//! assert_eq!(source.text(), raw.replacen("quiet", "calm", 1));
//! assert_eq!(event.after, source.revision());
//! // One host consumer drains ordered events. Native/host commits share history.
//! assert_eq!(source.drain_events().count(), 1);
//! source.undo().unwrap();
//! assert_eq!(source.text(), raw);
//! assert_eq!(source.selection(), selection);
//! source.redo().unwrap();
//! assert_eq!(source.text(), raw.replacen("quiet", "calm", 1));
//! ```
//!
//! # Projection: capture, compose separately, apply an exact source splice
//!
//! ```
//! use rndr_core::editing::{EditOrigin, SourceDocuments};
//! use rndr_core::source_projection::{Projection, ProjectionSelection, ProjectionError};
//! let mut docs = SourceDocuments::default();
//! let id = docs.open("A *quiet* line  \r\n");
//! let source = docs.get_mut(id).unwrap();
//! let projection = Projection::build(&source.snapshot());
//! let target = projection.capture(source, ProjectionSelection::inward(2, 7)).unwrap();
//! let old_target = target.clone();
//! target.replace(source, "calm", EditOrigin::Host(8)).unwrap();
//! source.undo().unwrap(); // Equal bytes do not restore the old revision.
//! assert!(matches!(old_target.replace(source, "loud", EditOrigin::Host(9)),
//!     Err(ProjectionError::Source(_))));
//! assert_eq!(source.text(), "A *quiet* line  \r\n");
//! ```
//!
//! There is no general inline Markdown editor/codec here. A replacement draft
//! has its own input/composition state; only Apply enters source history. Cross-
//! segment or structural edits reveal source, never serialize rendered content.
//!
//! # Structured: a separate tree, not a source-preserving migration
//!
//! ```
//! use rndr_core::editing::{Affinity, EditOrigin};
//! use rndr_core::structured::{Marks, Position, StructuredDocument,
//!     StructuredSelection, TextSelection};
//! let mut tree = StructuredDocument::new("A quiet line");
//! let block = tree.blocks()[0].id;
//! let selection = StructuredSelection::Text(TextSelection {
//!     anchor: Position { block, offset: 2, affinity: Affinity::After },
//!     head: Position { block, offset: 7, affinity: Affinity::Before },
//! });
//! tree.set_selection(selection).unwrap();
//! tree.set_marks(Marks { strong: true, ..Marks::default() }, EditOrigin::Host(10)).unwrap();
//! assert_eq!(tree.plain_text(), "A quiet line");
//! assert!(tree.blocks()[0].runs.iter().any(|run| run.marks.strong));
//! tree.undo().unwrap();
//! assert_eq!(tree.selection(), selection);
//! assert!(tree.blocks()[0].runs.iter().all(|run| !run.marks.strong));
//! tree.redo().unwrap();
//! ```
//!
//! `plain_text()` is a copy fallback, not Markdown serialization. Tree IDs,
//! selection and undo belong to the tree, not to a `SourceSession`.
//!
//! # Legacy adoption: explicit fork, retain the old owner/history
//!
//! There is **no history-transfer API**. Attaching a session to an existing
//! legacy editor discards that entity's legacy history and replaces its Buffer
//! from the session. Do not call attach as an implicit lossless migration.
//! A safe opt-in is a new managed entity: copy exact bytes and a grapheme-valid
//! directed selection once, keep the old editor/history until the host explicitly
//! chooses to retire it. It is a fork, not two mirrored writable views.
//!
//! ```
//! use rndr_core::{App, Buffer, CapturesKeys, Chrome, Editor, Label, edit_buffer};
//! use rndr_core::editing::{attach_source, detach_source, SourceDocuments, SourceSelection};
//! let mut app = App::new();
//! app.add_plugin(Chrome);
//! let raw = "\u{feff}é task  \r\n";
//! let mut buffer = Buffer::from_str(raw);
//! buffer.anchor = raw.len(); buffer.caret = 3; // after BOM, before é
//! let legacy = app.world_mut().spawn((buffer, Editor::default(), Label::new(""), CapturesKeys));
//! edit_buffer(app.world_mut(), legacy, 5..5, "!").unwrap();
//! let b = app.world().get::<Buffer>(legacy).unwrap();
//! let bytes = b.text();
//! let selection = SourceSelection { anchor: b.anchor, head: b.caret };
//! let mut docs = SourceDocuments::default();
//! let id = docs.open(bytes.clone());
//! docs.get_mut(id).unwrap().set_selection(selection).unwrap();
//! app.world_mut().insert_resource(docs);
//! let managed = app.world_mut().spawn((Buffer::from_str(""), Editor::default(), Label::new("")));
//! attach_source(app.world_mut(), managed, id).unwrap();
//! let copied = app.world().get::<Buffer>(managed).unwrap();
//! assert_eq!(copied.text(), bytes);
//! assert_eq!((copied.anchor, copied.caret), (selection.anchor, selection.head));
//! // Legacy history was never attached away; the old entity remains available.
//! detach_source(app.world_mut(), managed).unwrap();
//! let session = app.world().resource::<SourceDocuments>().unwrap().get(id).unwrap();
//! assert_eq!(session.text(), bytes);
//! assert!(app.world().is_alive(legacy));
//! ```
//!
//! Retained UI mounting/teardown recipes live in `rndr_ui::editing_recipes`.
//! All managed writes must go through their document APIs. Raw Buffer writes and
//! `edit_buffer` are not supported on either managed source or structured views.
