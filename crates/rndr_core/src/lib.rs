//! Application structure for rndr. No GPU types live here.
//!
//! Three.js-shaped cut from mob3: the world owns entities and hierarchy.
//! A render plugin reads the flat [`layout::Rect`] list.

mod app;
mod caret;
mod disclosure;
mod entity;
mod focus;
mod font;
mod fs;
mod hierarchy;
mod input;
mod layout;
mod memory_fs;
mod pane;
mod shell;
mod split;
mod style;
mod world;
mod wrap;

pub use app::{App, Plugin, Schedule, System};
pub use caret::{
    apply_caret, caret_line_col, selection_spans, set_editing, set_editing_field, Caret, EditField,
    Editing, Field,
};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    Focusable, NodeSpec,
};
pub use entity::Entity;
pub use focus::apply_focus;
pub use font::{
    apply_text_layout, field_text_width, layout_text, line_col, text_face, BitmapFont, Font,
    FontAtlas, GlyphBox, PlacedGlyph, SelectionBox, TextFace, TextLayout, TextLine,
};
pub use fs::{
    apply_dirty_marks, apply_fs_edit, apply_fs_load, apply_fs_save, spawn_fs_node, DirEntry, Dirty,
    DirtyMark, Files, Fs, FsPath,
};
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use input::{Focus, Input, Insets, Key, Pointer, Viewport, MOUSE_ID};
pub use layout::{
    ancestor_with, apply_scroll, hit_test, layout_propagate, Axis, Clip, Layout, Rect, Scroll,
    ScrollDrag, Size, SCROLL_DRAG_SLOP,
};
pub use memory_fs::{MemoryFs, MemoryFsPlugin};
pub use pane::{
    apply_pane, detail_pane, field_height, set_selection, shell, splitter, tree_column, Pane,
    Selection,
};
pub use shell::Shell;
pub use split::{apply_split, Drag, Splitter};
pub use style::{Body, Color, Label, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
pub use world::{Bundle, Component, World};
pub use wrap::{
    apply_wrap, cols_for, field_height_lines, index_at_pointer, index_at_visual, layout_of,
    visual_pos, visual_selection_spans, wrap_lines, VisualLine, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};
