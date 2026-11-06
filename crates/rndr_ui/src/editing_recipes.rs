//! Executable retained editing compositions, without a window or renderer.
//!
//! Install [`crate::UiPlugin`] once; it owns the existing core input dispatcher.
//! A surface owns its chrome and attachment, not the document/history. Do not
//! install a second editing dispatcher or mirror writes through raw Buffers.
//! These examples execute as doctests, not `no_run` pseudocode.
//!
//! # Source surface: update in place, destroy, remount
//!
//! ```
//! use rndr_core::{App, Layout};
//! use rndr_core::editing::{EditOrigin, SourceDocuments};
//! use rndr_ui::{UiPlugin, UiId};
//! use rndr_ui::editing::{EditorSurface, EditorSurfaceProps};
//! let mut app = App::new();
//! app.add_plugin(UiPlugin);
//! let mut docs = SourceDocuments::default();
//! let id = docs.open("\u{feff}source  \r\n");
//! let source = docs.get_mut(id).unwrap();
//! source.replace_selections(source.revision(), "!", EditOrigin::Host(1)).unwrap();
//! app.world_mut().insert_resource(docs);
//! let parent = app.world_mut().spawn((Layout::column(),));
//! let props = EditorSurfaceProps::new(id, "Source pane");
//! let first = EditorSurface::mount(app.world_mut(), parent, UiId(1), props.clone()).unwrap();
//! let same = EditorSurface::mount(app.world_mut(), parent, UiId(1), props.clone()).unwrap();
//! assert_eq!(first, same);
//! app.update(); // Input -> Update -> PostUpdate -> Layout, no native event loop
//! first.destroy(app.world_mut()).unwrap();
//! assert!(!app.world().is_alive(first.editor));
//! let second = EditorSurface::mount(app.world_mut(), parent, UiId(1), props).unwrap();
//! assert_ne!(first.editor, second.editor);
//! let source = app.world_mut().resource_mut::<SourceDocuments>().unwrap().get_mut(id).unwrap();
//! source.undo().unwrap();
//! assert_eq!(source.text(), "\u{feff}source  \r\n");
//! source.redo().unwrap(); // History survived the view, not copied from its Buffer.
//! second.destroy(app.world_mut()).unwrap();
//! ```
//!
//! New props default to editable; set `readonly` to the current document policy
//! when remounting. That prop changes document-wide policy, not a pane-local flag.
//! `apply_theme_bridge` is an explicit host opt-in for core caret/selection colors.
//!
//! # Structured surface: one tree-owned history, one attached view
//!
//! ```
//! use rndr_core::{App, Layout};
//! use rndr_core::editing::EditOrigin;
//! use rndr_core::structured::StructuredDocument;
//! use rndr_core::structured_native::StructuredDocuments;
//! use rndr_ui::{UiPlugin, UiId};
//! use rndr_ui::structured::{StructuredSurface, StructuredSurfaceProps};
//! let mut app = App::new();
//! app.add_plugin(UiPlugin);
//! let mut docs = StructuredDocuments::default();
//! let id = docs.insert(StructuredDocument::new("tree"));
//! docs.get_mut(id).unwrap().replace_selection("!", EditOrigin::Host(2)).unwrap();
//! app.world_mut().insert_resource(docs);
//! let parent = app.world_mut().spawn((Layout::column(),));
//! let props = StructuredSurfaceProps::new(id, "Tree pane");
//! let view = StructuredSurface::mount(app.world_mut(), parent, UiId(2), props.clone()).unwrap();
//! app.update();
//! view.destroy(app.world_mut()).unwrap();
//! let tree = app.world_mut().resource_mut::<StructuredDocuments>().unwrap().get_mut(id).unwrap();
//! tree.undo().unwrap();
//! assert_eq!(tree.plain_text(), "tree");
//! tree.redo().unwrap();
//! let next = StructuredSurface::mount(app.world_mut(), parent, UiId(2), props).unwrap();
//! next.destroy(app.world_mut()).unwrap();
//! assert!(app.world_mut().resource_mut::<StructuredDocuments>().unwrap().remove(id).is_some());
//! ```
//!
//! End or explicitly cancel native composition before detach/destroy; lifecycle
//! refusal is not permission to despawn internal entities behind the handle.
//! Unmount providers/child-editor sessions through their own owner APIs too.
//! `UiPlugin` systems/resources live for the App lifetime; uninstall is unsupported.
//! Full staged projection composition is the `source-session --bin projection`
//! example: render/capture with `rndr_core::source_projection`, compose a draft,
//! and Apply through the source owner. There is no retained inline projection
//! surface. Do not attach both source and tree authorities to the same entity.
