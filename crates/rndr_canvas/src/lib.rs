//! Headless canvas documents for whiteboards and design tools.
//!
//! A [`Document`] holds a [`Scene`] of ink, text, shape, line, image, group
//! and frame [`Node`]s. Edits are [`Transaction`]s of [`Operation`]s made
//! against a known revision; a [`CanvasRecord`] admits them, validates the
//! result and keeps undo history and retry receipts. [`CanvasStore`]
//! persists records, with [`MemoryStore`] and [`FsStore`] provided.
//! [`SceneIndex`] answers geometry questions (bounds, hit testing,
//! selection, grouping, alignment, snapping) and returns edits as
//! operations, so every change still goes through a transaction.
//!
//! Scenes serialize with the same JSON shape as Medousa Create version 3
//! and documents with either header are accepted.
//!
//! ```
//! use rndr_canvas::*;
//!
//! let store = MemoryStore::new();
//! let doc = store.create(CreateRequest {
//!     id: "board".into(),
//!     title: "Board".into(),
//!     scene: Scene::default(),
//! })?;
//! let note = Node::new(
//!     "note",
//!     Content::Shape {
//!         shape: ShapeKind::Rectangle,
//!         fill: "#ffd166".into(),
//!         stroke: "transparent".into(),
//!         stroke_width: 0.0,
//!         line: None,
//!     },
//!     Transform { x: 10.0, y: 10.0, width: 120.0, height: 80.0, rotation: 0.0 },
//! );
//! let done = store.transact("board", Transaction {
//!     id: "t1".into(),
//!     expected_revision: doc.revision,
//!     label: "Add note".into(),
//!     operations: vec![Operation::put(note)],
//! })?;
//! assert_eq!(done.document.revision, 1);
//!
//! let index = SceneIndex::new(&done.document.scene.nodes);
//! assert_eq!(index.hit(Vec2::new(50.0, 50.0), 0.0).map(|n| n.id.as_str()), Some("note"));
//!
//! let undone = store.history("board", HistoryRequest {
//!     id: "u1".into(),
//!     expected_revision: 1,
//!     redo: false,
//! })?;
//! assert!(undone.document.scene.nodes.is_empty());
//! # Ok::<(), CanvasError>(())
//! ```

mod error;
pub mod geometry;
pub mod hit;
pub mod layout;
pub mod legacy;
pub mod ops;
pub mod record;
pub mod schema;
pub mod selection;
pub mod store;
pub mod text;
pub mod validate;

pub use error::{CanvasError, Result};
pub use geometry::{Camera, LineGeometry, Matrix, Rect, SceneIndex, Vec2};
pub use layout::{Axis, Guide, Snapping};
pub use legacy::upgrade_legacy_arrows;
pub use ops::{apply_operations, HistoryRequest, Operation, Transaction};
pub use record::{CanvasRecord, CreateRequest, Mutation};
pub use schema::{
    Appearance, Asset, Brush, BrushKind, Content, Crop, Document, FontStyle, InkPoint, InputKind,
    Line, LinePoint, Node, Scene, Shadow, ShapeKind, TextAlign, TextLayout, TextSpan, Transform,
};
pub use selection::{Alignment, Endpoint, Handle, SelectionEdit, SelectionFrame};
pub use store::{CanvasStore, FsStore, ListPage, ListQuery, MemoryStore, Summary};
pub use text::{EstimateMeasure, StyledRun, TextMeasure};
pub use validate::{validate_document, validate_scene};
