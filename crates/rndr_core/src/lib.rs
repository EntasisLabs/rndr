//! Application structure for rndr. No GPU types live here.
//!
//! Three.js-shaped cut from mob3: the world owns entities and hierarchy.
//! A render plugin reads the flat [`layout::Rect`] list.

mod app;
mod disclosure;
mod entity;
mod hierarchy;
mod input;
mod layout;
mod style;
mod world;

pub use app::{App, Schedule, System};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    NodeSpec,
};
pub use entity::Entity;
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use input::{Input, Viewport};
pub use layout::{
    ancestor_with, apply_scroll, hit_test, layout_propagate, Axis, Clip, Layout, Rect, Scroll, Size,
};
pub use style::{Color, Label};
pub use world::{Bundle, Component, World};
