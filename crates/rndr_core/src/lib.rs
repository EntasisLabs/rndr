//! Application structure for rndr. No GPU types live here.
//!
//! Three.js-shaped cut from mob3: the world owns entities and hierarchy.
//! A render plugin reads the flat [`layout::Rect`] list.

mod app;
mod caret;
mod disclosure;
mod entity;
mod focus;
mod fs;
mod hierarchy;
mod input;
mod layout;
mod pane;
mod split;
mod style;
mod world;

pub use app::{App, Schedule, System};
pub use caret::{apply_caret, set_editing, set_editing_field, Caret, EditField, Editing, Field};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    Focusable, NodeSpec,
};
pub use entity::Entity;
pub use focus::apply_focus;
pub use fs::{apply_fs_edit, apply_fs_load, apply_fs_save, spawn_fs_node, Dirty, FsPath};
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use input::{Focus, Input, Key, Viewport};
pub use layout::{
    ancestor_with, apply_scroll, hit_test, layout_propagate, Axis, Clip, Layout, Rect, Scroll, Size,
};
pub use pane::{
    apply_pane, detail_pane, field_height, set_selection, shell, splitter, tree_column, Pane,
    Selection,
};
pub use split::{apply_split, Drag, Splitter};
pub use style::{Body, Color, Label};
pub use world::{Bundle, Component, World};
