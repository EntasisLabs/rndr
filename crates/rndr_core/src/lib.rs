//! Headless application core: entities, layout, input, text, events, and tasks.
//!
//! No window and no GPU types live here. A desktop program usually depends on
//! the `rndr` facade, which opens a window and presents this world. Tests and
//! tools can drive [`App`] directly.
//!
//! # Model
//!
//! [`World`] stores entities, components, and resources. [`App::new`] installs
//! [`Input`], [`Viewport`], and the host [`Platform`]. [`App::update`] runs one
//! frame: [`Schedule::Startup`] once, then Input, Update, PostUpdate, and
//! Layout. Paint and present live in `rndr_paint` and `rndr_render`.
//!
//! Layout, hit-testing, and scroll are in logical points. [`Viewport::scale`]
//! is physical pixels per point. The desktop crate divides window coordinates
//! by that scale and multiplies rectangles when it uploads them.
//!
//! [`Shell`] registers the file-tree systems. [`Chrome`] adds chords, buttons,
//! tabs, a single-line field, a multi-line prompt, overlays, and a virtual
//! list, and leaves `Shell` in place. [`Sim`] is the headless clock and
//! executor for tests. [`feed`] carries values produced off the UI thread.
//! The pump delivers them as [`Event`]s and wakes the window once per burst.
//!
//! # Example
//!
//! ```
//! use rndr_core::{App, Color, Label};
//!
//! let mut app = App::new();
//! let entity = app.world_mut().spawn((
//!     Label::new("hello"),
//!     Color::rgb(0.9, 0.9, 0.9),
//! ));
//! app.update();
//! assert!(app.world().is_alive(entity));
//! assert_eq!(app.world().get::<Label>(entity).unwrap().text, "hello");
//! ```
//!
//! # Features
//!
//! The optional `pool` feature adds a small thread-pool executor. The desktop
//! runner turns it on. The optional `tokio` feature spawns a [`feed`] producer
//! on a runtime the application already started. The default build depends on
//! neither.
//!
//! The optional `highlight` feature colors fenced code by token class. Without
//! it, a code block is one color.
//!
//! [`hotkey`], [`tray`], and [`window`] are the headless halves of those
//! desktop features: parsing, commands, and events. The operating-system
//! backends live in `rndr_render`.
//!
//! # Crate map
//!
//! | Crate | Role |
//! | --- | --- |
//! | `rndr` | Facade that re-exports the crates below. |
//! | `rndr_core` | This crate. |
//! | `rndr_paint` | One frame, then quads in logical points. |
//! | `rndr_std` | [`std::fs`] backend for [`Fs`]. |
//! | `rndr_render` | Window, input, and presentation. |

#![deny(missing_docs)]

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
pub mod feed;
mod field;
mod focus;
mod font;
mod fs;
mod hierarchy;
pub mod hotkey;
mod input;
mod keymap;
mod keys;
mod layout;
mod list;
mod markdown;
mod memory_fs;
mod mono;
mod overlay;
mod pane;
#[cfg(feature = "pool")]
mod pool;
mod prompt;
mod runtime;
mod shell;
mod sim;
mod slot;
mod split;
mod style;
mod tabs;
mod text;
mod theme;
pub mod tray;
mod typing;
pub mod window;
mod world;
mod wrap;

pub use action::{apply_actions, Actions, Binding, Chord, Fired, COPY, CUT, PASTE};
pub use app::{App, Plugin, Schedule, System};
pub use button::Button;
pub use caret::{
    apply_caret, caret_line_col, selection_spans, set_editing, set_editing_field, Caret, EditField,
    Editing, Field, TextSelection,
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
pub use feed::{Epoch, Feed, FeedConfig, FeedError, OnFull};
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
pub use hotkey::{
    note_unregistered, GlobalHotkey, HotkeyBinding, HotkeyError, HotkeyHandle, HotkeyId,
    HotkeyRejected,
};
pub use input::{
    CursorStyle, Focus, Input, Insets, Key, MotionWatch, Pointer, PointerButton, Viewport, MOUSE_ID,
};
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
pub use markdown::{
    markdown_view, parse_markdown, LinkActivated, MarkdownBlock, MarkdownKind, MarkdownLink,
    MarkdownPlugin, MarkdownTheme, MarkdownView,
};
pub use memory_fs::{MemoryFs, MemoryFsPlugin};
pub use mono::{
    apply_mono_grid, MonoBand, MonoCache, MonoCell, MonoCursor, MonoCursorShape, MonoGrid, MonoRow,
    MonoRun, MonoStyle, MonoUnderline,
};
pub use overlay::{Layer, Modal, DISMISS};
pub use pane::{
    apply_pane, detail_pane, field_height, set_selection, shell, splitter, tree_column, Pane,
    Selection,
};
#[cfg(feature = "pool")]
pub use pool::thread_pool;
pub use prompt::{Placeholder, Prompt, PromptHistory};
pub use runtime::{
    after, cancel_debounce, debounce, debounce_local, every, frame_wake, run_until_parked, sleep,
    spawn, spawn_detached, spawn_local, Executor, InlineExecutor, Job, NopWake, Sleep, TaskHandle,
    TimerHandle, Wake,
};
pub use shell::Shell;
pub use sim::Sim;
pub use slot::SurfaceSlot;
pub use split::{apply_split, Drag, Splitter};
pub use style::{Body, Color, Label, TextOverflow, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
pub use tabs::{Hidden, Tab, TabPanel, Tabs};
pub use text::{
    ensure_text_engine, style_for, style_of, text_engine, BuiltinText, FontStyle, TextEngine,
    TextStyle, FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR,
};
pub use theme::{theme_of, Radius, Theme};
pub use tray::{
    displayed_label, map_report, menu_label, parse_menu_id, phase, push_report, set_checked,
    set_enabled, set_icon, set_label, set_menu, set_tooltip, starts_hidden, toggle_on_click,
    FakeTray, IconError, Image, Item, Menu, MenuItem, Submenu, TrayCommand, TrayConfig, TrayEvent,
    TrayIcon, TrayItemId, TrayMapped, TrayMenu, TrayPhase, TrayReport, TraySupport,
};
pub use window::{
    platform_support, Availability, Dismiss, DismissHold, LogicalSize, Position, Resized,
    ScaleChanged, WindowConfig, WindowEffect, WindowFocus, WindowHandle, WindowKind, WindowMetrics,
    WindowPhase, WindowSignal, WindowSupport, WindowTransition,
};
pub use world::{Bundle, Component, World};
pub use wrap::{
    apply_wrap, cols_for, field_height_lines, index_at_pointer, index_at_visual, layout_of,
    visual_pos, visual_selection_spans, wrap_lines, VisualLine, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};
