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
//! | `rndr_core` | This crate. The headless world. |
//! | `rndr_paint` | One frame, then quads in logical points. |
//! | `rndr_std` | [`std::fs`] backend for [`Fs`]. |
//! | `rndr_term` | Optional pseudoterminal. Off unless a feature asks for it. |
//! | `rndr_render` | The only OS window and GPU, plus offscreen capture. |
//! | `rndr_ui` | Opt-in components. Depends only on this crate. |
//! | `rndr_app` | Starter helpers for an application, not the component library. Depends only on this crate. |
//!
//! 0.6.0 publishes this crate together with `rndr`, `rndr-flow`, `rndr_paint`,
//! `rndr_std`, `rndr_term`, `rndr_ui`, `rndr_app`, `rndr_liquid`, `rndr_chart`,
//! `rndr_liquid_ui`, `rndr_canvas`, `rndr_canvas_ui`, `rndr_render`, `rndr_media`,
//! and `rndr_browser`. Repository examples are not published. The version is a
//! 0.y release, not a 1.0 stability promise.

#![deny(missing_docs)]

mod access;
mod access_text;
mod action;
mod app;
mod appearance;
mod button;
mod caret;
mod chrome;
mod clipboard;
pub mod clock;
mod close;
mod freeze;
pub use freeze::DocumentFreeze;
mod compose;
mod cursor;
mod decor;
mod desktop;
mod dialog;
mod disclosure;
mod edit;
/// Experimental revisioned source document sessions and view attachment.
pub mod editing;
/// Executable source, structured, projection and explicit legacy-fork recipes.
pub mod editing_recipes;
mod editor;
mod entity;
mod event;
pub mod feed;
mod field;
pub mod file_drop;
mod flex;
mod focus;
mod font;
mod frame_surface;
mod fs;
mod grid;
mod hierarchy;
pub mod hotkey;
mod icon;
mod image_view;
mod ime;
mod input;
mod interaction;
mod keymap;
mod keys;
mod layout;
mod lifecycle;
mod list;
pub mod live;
mod markdown;
mod memory_fs;
mod mono;
pub mod motion;
mod open_url;
mod overlay;
mod pane;
mod panel_place;
mod pointers;
#[cfg(feature = "pool")]
mod pool;
mod prompt;
mod runtime;
mod scrollbar;
mod secondary;
mod selectable;
/// Owned revision-fenced asynchronous source extension protocols.
pub mod source_extensions;
/// Experimental whole-line source folds and optional language hooks.
pub mod source_folding;
/// Live additive provider decoration and gutter paint projection.
pub mod source_presentation;
/// Source-preserving staged projection with checked targets; not a Markdown codec.
pub mod source_projection;
/// Normalized directed multi-selections and visual rectangular selection policy.
pub mod source_selection;
pub use scrollbar::{
    apply_scrollbars, scrollbar_dragging, scrollbar_geometry, set_scrollbar_value, Scrollbar,
    ScrollbarGeometry,
};
mod app_menu;
mod shell;
mod sim;
mod slot;
/// Revision-bound literal source search and atomic replacements.
pub mod source_search;
mod split;
/// Experimental structured tree model; separate from source sessions.
pub mod structured;
/// Typed noneditable list, task, quote and rule paint projection.
pub mod structured_chrome;
pub mod structured_embeds;
/// Native projection and input for structured documents, not source storage.
pub mod structured_native;
mod style;
mod tabs;
mod text;
mod theme;
pub mod tray;
mod typing;
pub mod window;
mod window_chrome;
mod world;
mod wrap;

pub use access::{
    access_id, access_tree, apply_access, entity_from_access_id, AccessCommand, AccessMark,
    AccessNode, AccessRole, AccessTree, CollectionAccess, DataAccess, DescribedBy, LivePoliteness,
    LiveRegion, NumericAccess, NumericAccessSet, ACCESS_ROOT,
};
pub use access_text::{access_text, AccessTextSnapshot, AccessTextState};
pub use action::{apply_actions, Actions, Binding, Chord, Fired, COPY, CUT, PASTE};
pub use app::{App, Plugin, Schedule, System};
pub use appearance::{
    publish_system_appearance, system_appearance, SystemAppearance, SystemAppearanceChanged,
};
pub use button::Button;
pub use caret::{
    apply_caret, caret_line_col, selection_spans, set_editing, set_editing_field, Caret, CaretRow,
    EditField, Editing, Field, TextSelection,
};
pub use chrome::Chrome;
pub use clipboard::{Clipboard, ClipboardImage};
pub use clock::{ManualClock, SystemClock, Time};
pub use close::{CloseGuard, CloseRequest, CloseState};
pub use compose::{BackdropBlur, Blur, Opacity, Transform};
pub use cursor::{cursor_image, rotate_cursor, CursorImage};
pub use decor::{BorderSides, ClipRounded, Corners, Gradient, GradientStop, Shadow, Shadows};
pub use desktop::{
    deliver, desktop, install_wake, CountingWake, Desktop, OpenWindow, WindowHub, WindowId,
    WindowMsg,
};
pub use dialog::{
    request_file_dialog, take_file_dialog, FileChosen, FileDialog, FileDialogKind, FileFilter,
};
pub use disclosure::{
    page_layout, scroll_page, spawn_node, toggle_disclosure, toggle_disclosures, Disclosure,
    Focusable, NodeSpec,
};
pub use edit::edit_string;
pub use editor::{
    edit_buffer, spans_for_text, Buffer, BufferEditError, Decoration, DecorationKind,
    DecorationMark, DecorationMarks, Decorations, Editor, EditorLayoutStats, Gutter,
};
pub use entity::Entity;
pub use event::{events, send, Event, Events};
pub use feed::{Epoch, Feed, FeedConfig, FeedError, OnFull};
pub use field::{
    replace_field_text, reset_field_draft, CapturesKeys, FieldTransaction, FieldTransactions,
    FocusBoundary, ImePreview, Submitted, TextChanged, TextField,
};
pub use file_drop::{FileDrop, FileHover, FileHoverCancel, FileHoverSet};
pub use flex::{
    paint_order, Absolute, Align, AlignSelf, AspectRatio, Items, Justify, MaxSize, Padding, Sticky,
    Wrap,
};
pub use focus::{apply_focus, reveal_entity};
pub use font::{
    apply_text_layout, field_text_width, layout_text, layout_text_spans, line_col, measure_text,
    text_face, BitmapFont, ColorAtlas, Font, FontAtlas, FontAxis, FontFeature, GlyphBox,
    PlacedGlyph, SelectionBox, Spans, TextFace, TextHighlight, TextLayout, TextLine, TextMeasure,
    TextSpan,
};
pub use frame_surface::{
    FrameError, FrameSurface, PixelFormat, SurfaceFit, SurfaceStats, SurfaceView, VideoFrame,
    YuvMatrix, YuvRange,
};
pub use fs::{
    apply_dirty_marks, apply_fs_edit, apply_fs_load, apply_fs_save, spawn_fs_node, DirEntry, Dirty,
    DirtyMark, Files, Fs, FsPath,
};
pub use grid::{Grid, GridSpan, Track};
pub use hierarchy::{children_of, despawn, roots, set_parent, Children, Parent};
pub use hotkey::{
    note_unregistered, GlobalHotkey, HotkeyBinding, HotkeyError, HotkeyHandle, HotkeyId,
    HotkeyRejected,
};
pub use icon::{
    install_fallback_font, is_icon_scalar, strip_icon_scalars, Icon, ICON_SCALAR_START,
};
pub use image_view::ImageView;
pub use ime::{resolve_ime, text_frame, ImeEvent, ResolvedIme, TextFrame};
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
    Rect, Scroll, ScrollDrag, Size, ViewportRows, SCROLL_DRAG_SLOP,
};
pub use list::{
    list_window, List, ListActivated, ListItem, ListKeys, ListRows, LIST_ACTIVATE, LIST_DOWN,
    LIST_END, LIST_HOME, LIST_PAGE_DOWN, LIST_PAGE_UP, LIST_UP,
};
pub use markdown::{
    clear_markdown_slots, code_token_spans, markdown_realized_blocks, markdown_view,
    mount_markdown_slot, parse_markdown, set_markdown_slot_host, LinkActivated, MarkdownBlock,
    MarkdownKind, MarkdownLink, MarkdownPlugin, MarkdownSlot, MarkdownSlotError, MarkdownSlotHost,
    MarkdownSlotHosts, MarkdownSlotId, MarkdownTheme, MarkdownView,
};
pub use memory_fs::{MemoryFs, MemoryFsPlugin};
pub use mono::{
    apply_mono_grid, MonoBand, MonoCache, MonoCell, MonoCursor, MonoCursorShape, MonoGrid, MonoRow,
    MonoRun, MonoStyle, MonoUnderline,
};
pub use overlay::{Layer, Modal, DISMISS};
pub use panel_place::{
    panel_placement, windows_panel_action, PanelAnchor, PanelMargins, PanelPlacement,
    WindowsPanelAction, WIN32_EX_NOACTIVATE, WIN32_GWL_EXSTYLE, WIN32_HWND_TOPMOST,
    WIN32_SWP_NOACTIVATE, WIN32_SWP_NOMOVE, WIN32_SWP_NOSIZE, WIN32_SWP_SHOWWINDOW,
    WIN32_SW_SHOWNOACTIVATE,
};
pub use selectable::{
    selectable_input, Selectable, SelectablePlugin, MULTI_PRESS_INTERVAL, MULTI_PRESS_SLOP,
};
mod popup;
pub use app_menu::{
    app_menu, app_menu_id, app_menu_pending, app_menu_support, parse_app_menu_id,
    push_app_menu_pick, set_app_menu, take_app_menu_change, AppMenuPicked,
};
pub use lifecycle::{is_link, launch_links, push_opened_url, push_reopened, Reopened, UrlOpened};
pub use live::{
    Live, LiveBlur, LiveBorder, LiveFill, LiveGradient, LiveLabel, LiveOffset, LiveOpacity,
    LiveRect, LiveScale, LiveShadow, LiveText, LiveTextColor, LiveValue, Smooth,
};
pub use open_url::{check_url, request_open_url, take_url_requests, OpenUrlError};
pub use pane::{
    apply_pane, detail_pane, field_height, set_selection, shell, splitter, tree_column, Pane,
    Selection,
};
pub use pointers::{
    touch_id, Contact, ContactPhase, ContactUpdate, Gesture, GesturePhase, GestureState,
    PointerButtons, PointerKind, PointerSample, Pointers, PEN_ID, TOUCH_ID_BIT,
};
#[cfg(feature = "pool")]
pub use pool::thread_pool;
pub use popup::{
    close_popup, open_popup, popup_rect, AnchoredPopup, FocusOwner, PassivePopup, PopupPlacement,
};
pub use prompt::{Placeholder, Prompt, PromptHistory};
pub use runtime::{
    after, cancel_debounce, debounce, debounce_local, every, frame_wake, run_until_parked, sleep,
    spawn, spawn_detached, spawn_local, Executor, InlineExecutor, Job, NopWake, Sleep, TaskHandle,
    TimerHandle, Wake,
};
pub use secondary::{ContextTarget, SecondaryPress};
pub use shell::Shell;
pub use sim::Sim;
pub use slot::{effects_animating, EffectSlot, SurfaceSlot};
pub use split::{apply_split, Drag, Splitter};
pub use style::{
    label_box, Body, Border, Color, CrossMark, Label, LabelBox, ScrollIndicator, TextOverflow,
    CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
pub use tabs::{Hidden, Tab, TabPanel, Tabs};
pub use text::{
    ensure_text_engine, has_implicit_text_engine, style_for, style_of, text_engine, BuiltinText,
    FontStyle, TextAlign, TextEngine, TextStyle, FONT_WEIGHT_BOLD, FONT_WEIGHT_REGULAR,
};
pub use theme::{theme_of, Radius, Theme};
pub use tray::{
    displayed_label, map_report, menu_label, parse_menu_id, phase, push_report, set_checked,
    set_enabled, set_icon, set_label, set_menu, set_tooltip, starts_hidden, toggle_on_click,
    FakeTray, IconError, Image, Item, Menu, MenuItem, Submenu, TrayCommand, TrayConfig, TrayEvent,
    TrayIcon, TrayItemId, TrayMapped, TrayMenu, TrayPhase, TrayReport, TraySupport,
};
pub use window::{
    platform_support, support_describes_panels_only, Availability, Dismiss, DismissHold,
    LogicalSize, Position, Resized, ScaleChanged, WindowConfig, WindowEffect, WindowFocus,
    WindowHandle, WindowKind, WindowMetrics, WindowPhase, WindowSignal, WindowSupport,
    WindowTransition,
};
pub use window_chrome::{
    apply_window_chrome, request_window_action, resize_edge_at, take_window_actions, Backdrop,
    ResizeEdge, Titlebar, WindowAction, WindowButton, WindowChrome, WindowDrag, RESIZE_CORNER,
    RESIZE_EDGE,
};
pub use world::{Bundle, Component, World};
pub use wrap::{
    apply_wrap, cols_for, field_height_lines, index_at_pointer, index_at_visual, layout_of,
    visual_pos, visual_selection_spans, wrap_lines, VisualLine, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};

mod manual_tabs;
pub use manual_tabs::{ControlDisabled, ManualTabs, ResolvedColor, SelectedState, TabOrientation};

pub use interaction::{
    access_details, cancel_activation, interaction_pressed, interaction_visible, request_focus,
    take_action_intents, take_action_intents_matching, try_activate, AccessDetails, ActionIntent,
    ActionIntents, ActivationHandler, ActivationResult, FocusTarget, GenericInteraction,
    KeyNavigation, ReleaseActivation, Semantics, TabRelation, TabStop,
};

pub use interaction::{
    forget_interaction_targets, link_tab_panel, move_tab_focus, TabFocusMove, TabLinkError,
};

pub use access::{try_apply_access, AccessCommandResult, SelectionAccess};
