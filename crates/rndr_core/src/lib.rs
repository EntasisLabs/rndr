//! Application structure for rndr. No GPU types live here.
//!
//! Three.js-shaped cut from mob3: the world owns entities and hierarchy.
//! A render plugin reads the flat [`layout::Rect`] list.

mod action;
mod app;
mod button;
mod caret;
mod chrome;
mod clipboard;
pub mod clock;
mod disclosure;
mod edit;
mod editor;
mod entity;
mod event;
mod field;
mod focus;
mod font;
mod fs;
mod hierarchy;
mod input;
mod keymap;
mod keys;
mod layout;
mod list;
mod memory_fs;
mod overlay;
mod pane;
#[cfg(feature = "pool")]
mod pool;
mod runtime;
mod shell;
mod sim;
mod slot;
mod split;
mod style;
mod tabs;
mod text;
mod theme;
mod typing;
pub mod window;
mod world;
mod wrap;

pub use action::{apply_actions, Actions, Binding, Chord, Fired, COPY, CUT, PASTE};
pub use app::{App, Plugin, Schedule, System};
pub use button::Button;
pub use caret::{
    apply_caret, caret_line_col, selection_spans, set_editing, set_editing_field, Caret, EditField,
    Editing, Field,
};
pub use chrome::Chrome;
pub use clipboard::Clipboard;
pub use clock::{ManualClock, SystemClock, Time};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    Focusable, NodeSpec,
};
pub use edit::edit_string;
pub use editor::{
    Buffer, Decoration, DecorationKind, DecorationMark, DecorationMarks, Decorations, Editor,
    Gutter,
};
pub use entity::Entity;
pub use event::{events, send, Event, Events};
pub use field::{CapturesKeys, ImePreview, Submitted, TextChanged, TextField};
pub use focus::apply_focus;
pub use font::{
    apply_text_layout, field_text_width, layout_text, layout_text_spans, line_col, measure_text,
    text_face, BitmapFont, ColorAtlas, Font, FontAtlas, GlyphBox, PlacedGlyph, SelectionBox, Spans,
    TextFace, TextHighlight, TextLayout, TextLine, TextMeasure, TextSpan,
};
pub use fs::{
    apply_dirty_marks, apply_fs_edit, apply_fs_load, apply_fs_save, spawn_fs_node, DirEntry, Dirty,
    DirtyMark, Files, Fs, FsPath,
};
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use input::{Focus, Input, Insets, Key, Pointer, Viewport, MOUSE_ID};
pub use keymap::{
    apply_keymap, context_path, context_stack, Clock, Context, Dispatch, KeyBinding, Keymap,
    KeymapFired,
};
pub use keys::{
    legacy_key, platform_of, standard_action, EditAction, KeyCode, KeyEvent, Keystroke, LogicalKey,
    Modifiers, NamedKey, Platform, Trigger,
};
pub use layout::{
    ancestor_with, apply_scroll, hit_test, layered_roots, layout_propagate, Axis, Clip, Layout,
    Rect, Scroll, ScrollDrag, Size, SCROLL_DRAG_SLOP,
};
pub use list::{
    List, ListActivated, ListItem, ListKeys, ListRows, LIST_ACTIVATE, LIST_DOWN, LIST_END,
    LIST_HOME, LIST_PAGE_DOWN, LIST_PAGE_UP, LIST_UP,
};
pub use memory_fs::{MemoryFs, MemoryFsPlugin};
pub use overlay::{Layer, Modal, DISMISS};
pub use pane::{
    apply_pane, detail_pane, field_height, set_selection, shell, splitter, tree_column, Pane,
    Selection,
};
#[cfg(feature = "pool")]
pub use pool::thread_pool;
pub use runtime::{
    after, cancel_debounce, debounce, debounce_local, every, run_until_parked, sleep, spawn,
    spawn_local, Executor, InlineExecutor, Job, NopWake, Sleep, TaskHandle, TimerHandle, Wake,
};
pub use shell::Shell;
pub use sim::Sim;
pub use slot::SurfaceSlot;
pub use split::{apply_split, Drag, Splitter};
pub use style::{Body, Color, Label, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
pub use tabs::{Hidden, Tab, TabPanel, Tabs};
pub use text::{
    ensure_text_engine, style_of, text_engine, BuiltinText, FontStyle, TextEngine, TextStyle,
    FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR,
};
pub use theme::{theme_of, Radius, Theme};
pub use window::{
    platform_support, Availability, Dismiss, LogicalSize, Position, Resized, ScaleChanged,
    WindowConfig, WindowEffect, WindowFocus, WindowHandle, WindowKind, WindowMetrics, WindowPhase,
    WindowSignal, WindowSupport, WindowTransition,
};
pub use world::{Bundle, Component, World};
pub use wrap::{
    apply_wrap, cols_for, field_height_lines, index_at_pointer, index_at_visual, layout_of,
    visual_pos, visual_selection_spans, wrap_lines, VisualLine, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};
