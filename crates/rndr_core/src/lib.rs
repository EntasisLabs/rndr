//! Application structure for rndr. No GPU types live here.
//!
//! Three.js-shaped cut from mob3: the world owns entities and hierarchy.
//! A render plugin reads the flat [`layout::Rect`] list.

mod app;
mod caret;
mod disclosure;
mod entity;
mod focus;
mod hierarchy;
mod input;
mod layout;
mod pane;
mod style;
mod world;

pub use app::{App, Schedule, System};
pub use caret::{apply_caret, set_editing, Caret, Editing};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    Focusable, NodeSpec,
};
pub use entity::Entity;
pub use focus::apply_focus;
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use input::{Focus, Input, Key, Viewport};
pub use layout::{
    ancestor_with, apply_scroll, hit_test, layout_propagate, Axis, Clip, Layout, Rect, Scroll, Size,
};
pub use pane::{apply_pane, detail_pane, set_selection, shell, tree_column, Pane, Selection};
pub use style::{Color, Label};
pub use world::{Bundle, Component, World};
