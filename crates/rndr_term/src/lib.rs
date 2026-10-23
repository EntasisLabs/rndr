//! Embedded terminal session.
//!
//! A [`Terminal`] entity owns a PTY and a terminal state machine. Output is
//! copied into a [`MonoGrid`] each frame, and only rows
//! whose cells changed are shaped again. The reader thread never touches the
//! world: it appends bytes and wakes the window loop.
//!
//! [`TerminalPlugin`] registers the input and update systems. [`open_terminal`]
//! spawns the shell. Core and paint stay free of the PTY; this crate is the
//! only one that links it.
//!
//! The state machine is `alacritty_terminal`: it already tracks a cell grid,
//! scrollback, damage, selection, and the full set of modes a full-screen
//! program toggles. The PTY is `portable-pty`, which opens a pseudoterminal
//! on Unix and ConPTY on Windows.

#![deny(missing_docs)]

mod encode;
mod screen;
mod session;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use rndr_core::{
    after, ancestor_with, events, frame_wake, hit_test, platform_of, send, text_engine,
    CapturesKeys, Clipboard, Entity, Event, Focus, Focusable, Input, KeyCode, KeyEvent, Layout,
    LogicalKey, Modifiers, MonoGrid, MotionWatch, NamedKey, Platform, Plugin, PointerButton, Rect,
    Schedule, Size, TextStyle, TimerHandle, World, FONT_WEIGHT_REGULAR,
};

pub use encode::{
    encode_key, encode_mouse, encode_paste, EncodeMode, MouseAction, MouseButton, MouseEncode,
    MouseProtocol,
};

use encode::MouseAction as Action;
use session::{OpenOptions, Session};

const SCROLLBACK: usize = 10_000;
const BLINK: Duration = Duration::from_millis(530);

/// Failure while opening a PTY or starting the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermError {
    message: String,
}

impl TermError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TermError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TermError {}

/// Program, size, and font for [`open_terminal`].
#[derive(Clone, Debug)]
pub struct TerminalOptions {
    /// Executable. [`TerminalOptions::shell`] fills this from the environment.
    pub program: String,
    /// Arguments after the program. Empty starts an interactive shell.
    pub args: Vec<String>,
    /// Working directory. `None` keeps the parent process directory.
    pub cwd: Option<PathBuf>,
    /// Lines kept above the viewport.
    pub scrollback: usize,
    /// Columns used until the entity has a real rect.
    pub cols: u16,
    /// Rows used until the entity has a real rect.
    pub rows: u16,
    /// Face for the grid. `None` uses a 16 point monospace face.
    pub font: Option<TextStyle>,
    /// Cell pitch in points. `None` measures `font` from the text engine.
    pub cell: Option<(f32, f32)>,
    /// Parse bytes with no child process. Tests use this.
    pub offline: bool,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            program: default_shell(),
            args: Vec::new(),
            cwd: None,
            scrollback: SCROLLBACK,
            cols: 80,
            rows: 24,
            font: None,
            cell: None,
            offline: false,
        }
    }
}

impl TerminalOptions {
    /// Interactive login shell: `$SHELL`, or `sh` on Unix and `cmd.exe` on Windows.
    pub fn shell() -> Self {
        Self::default()
    }

    /// Run `program` with no arguments.
    pub fn program(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            ..Self::default()
        }
    }

    /// Headless grid. [`feed_terminal`] supplies the bytes a PTY would.
    pub fn offline() -> Self {
        Self {
            offline: true,
            ..Self::default()
        }
    }
}

/// Marker for an entity that hosts a session.
///
/// The id matches [`TerminalExited`] and [`close_terminal`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Terminal {
    /// Session id assigned by [`open_terminal`].
    pub id: u64,
}

impl rndr_core::Component for Terminal {}

/// The child process ended.
///
/// The grid stays so the last screen can still be scrolled and copied.
/// `code` is the exit status when the platform reported one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalExited {
    /// Entity that hosted the session.
    pub entity: Entity,
    /// [`Terminal::id`] at exit.
    pub id: u64,
    /// Process status. `None` when the process was signaled or the status was unavailable.
    pub code: Option<i32>,
}

impl Event for TerminalExited {}

/// The shell set the window title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalTitle {
    /// Entity that hosted the session.
    pub entity: Entity,
    /// [`Terminal::id`].
    pub id: u64,
    /// Title text. Empty means the shell cleared it.
    pub title: String,
}

impl Event for TerminalTitle {}

/// The shell rang the bell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalBell {
    /// Entity that hosted the session.
    pub entity: Entity,
    /// [`Terminal::id`].
    pub id: u64,
}

impl Event for TerminalBell {}

/// Cursor blink tick. Private to the update system.
struct BlinkTick;

impl Event for BlinkTick {}

struct Slot {
    entity: Entity,
    session: Option<Session>,
}

struct Sessions {
    next: u64,
    map: HashMap<u64, Slot>,
    /// Visible phase of a blinking cursor.
    phase: bool,
    /// Armed blink. Dropping it cancels the tick.
    blink: Option<TimerHandle>,
    /// This plugin set [`MotionWatch`] so it can clear it later.
    motion: bool,
}

impl Default for Sessions {
    fn default() -> Self {
        Self {
            next: 1,
            map: HashMap::new(),
            phase: true,
            blink: None,
            motion: false,
        }
    }
}

/// Input, PTY reads, resize, and cursor blink for every [`Terminal`].
///
/// Inserts [`Focus`] and [`Clipboard`] when the app has not already.
pub struct TerminalPlugin;

impl Plugin for TerminalPlugin {
    fn build(self, app: &mut rndr_core::App) {
        if !app.world().has_resource::<Focus>() {
            app.world_mut().insert_resource(Focus::default());
        }
        if !app.world().has_resource::<Clipboard>() {
            app.world_mut().insert_resource(Clipboard::default());
        }
        if !app.world().has_local::<Sessions>() {
            app.world_mut().insert_local(Sessions::default());
        }
        app.add_system_front(Schedule::Input, drive_input);
        app.add_system(Schedule::Update, drive_update);
    }
}

/// Spawn `options` on `entity` and focus it.
///
/// The entity gains a [`MonoGrid`], a background [`Color`](rndr_core::Color),
/// a fill [`Layout`], [`Focusable`], and [`CapturesKeys`]. A later rect
/// resizes the PTY. Dropping the session, or [`close_terminal`], kills the
/// child and joins the reader.
pub fn open_terminal(
    world: &mut World,
    entity: Entity,
    options: TerminalOptions,
) -> Result<(), TermError> {
    if !world.is_alive(entity) {
        return Err(TermError::new("entity is not alive"));
    }
    if world.has::<Terminal>(entity) {
        close_terminal(world, entity);
    }
    ensure_sessions(world);
    let style = options.font.clone().unwrap_or_else(default_font);
    if style.family == "monospace" {
        let faces = [
            "DejaVu Sans Mono",
            "Liberation Mono",
            "Noto Sans Mono",
            "Menlo",
            "Consolas",
            "Cascadia Mono",
            "Courier New",
        ]
        .map(str::to_string);
        text_engine(world).set_fallbacks("monospace", &faces);
    }
    let (cell_w, cell_h) = options.cell.unwrap_or_else(|| measure_cell(world, &style));
    let pin_cell = options.cell.is_some();
    let cols = options.cols.max(2);
    let rows = options.rows.max(1);
    let session = Session::open(OpenOptions {
        program: options.program,
        args: options.args,
        cwd: options.cwd,
        scrollback: options.scrollback,
        cols,
        rows,
        offline: options.offline,
        wake: frame_wake(world),
    })
    .map_err(TermError::new)?;
    let mut session = session;
    session.cell_w = cell_w;
    session.cell_h = cell_h;
    session.pin_cell = pin_cell;
    session.resize(cols, rows, cell_w, cell_h);
    let mut grid = MonoGrid::blank(cols, rows, cell_w, cell_h);
    grid.font = style;
    grid.default_bg = screen::default_bg();
    let id = {
        let store = world.local_mut::<Sessions>().expect("sessions");
        let id = store.next;
        store.next = store.next.saturating_add(1);
        store.map.insert(
            id,
            Slot {
                entity,
                session: Some(session),
            },
        );
        id
    };
    world.insert(entity, Terminal { id });
    world.insert(entity, grid);
    world.insert(entity, screen::default_bg());
    if !world.has::<Layout>(entity) {
        world.insert(
            entity,
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
        );
    }
    world.insert(entity, Focusable);
    world.insert(entity, CapturesKeys);
    if let Some(focus) = world.resource_mut::<Focus>() {
        focus.entity = Some(entity);
    }
    publish_one(world, id);
    Ok(())
}

/// Kill the child, join the reader, and drop the grid components.
pub fn close_terminal(world: &mut World, entity: Entity) {
    let Some(terminal) = world.get::<Terminal>(entity).copied() else {
        return;
    };
    if let Some(store) = world.local_mut::<Sessions>() {
        store.map.remove(&terminal.id);
    }
    world.remove::<Terminal>(entity);
    world.remove::<MonoGrid>(entity);
    world.remove::<rndr_core::MonoCache>(entity);
}

/// Parse `bytes` as output from the child.
///
/// Offline sessions use this instead of a PTY. Returns false when `entity`
/// has no session.
pub fn feed_terminal(world: &mut World, entity: Entity, bytes: &[u8]) -> bool {
    let Some(id) = world.get::<Terminal>(entity).map(|terminal| terminal.id) else {
        return false;
    };
    let Some(mut session) = take_session(world, id) else {
        return false;
    };
    session.feed(bytes);
    put_session(world, id, session);
    publish_one(world, id);
    true
}

/// Bytes an offline session would have written to the child, then clear them.
///
/// A live session returns an empty buffer: those bytes already went to the PTY.
/// Returns `None` when `entity` has no session.
pub fn take_terminal_bytes(world: &mut World, entity: Entity) -> Option<Vec<u8>> {
    let id = world.get::<Terminal>(entity).map(|terminal| terminal.id)?;
    let mut session = take_session(world, id)?;
    let bytes = std::mem::take(&mut session.offline_out);
    put_session(world, id, session);
    Some(bytes)
}

/// Write `bytes` to the child, as if they were typed.
///
/// Returns false when `entity` has no session or the session is offline.
pub fn write_terminal(world: &mut World, entity: Entity, bytes: &[u8]) -> bool {
    let Some(id) = world.get::<Terminal>(entity).map(|terminal| terminal.id) else {
        return false;
    };
    let Some(mut session) = take_session(world, id) else {
        return false;
    };
    let live = session.writer_open();
    session.write_bytes(bytes);
    put_session(world, id, session);
    live
}

fn drive_input(world: &mut World) {
    let Some(input) = world.resource::<Input>().cloned() else {
        return;
    };
    let platform = platform_of(world);
    let pointer = input.pointer;
    let hit = hit_test(world, pointer.position[0], pointer.position[1])
        .and_then(|entity| ancestor_with::<Terminal>(world, entity));
    if pointer.pressed {
        if let Some(entity) = hit {
            if let Some(focus) = world.resource_mut::<Focus>() {
                focus.entity = Some(entity);
            }
        }
    }
    let focused = world
        .resource::<Focus>()
        .and_then(|focus| focus.entity)
        .filter(|entity| world.has::<Terminal>(*entity));
    if let Some(entity) = focused {
        apply_keys(world, entity, &input, platform);
    }
    let hover = hit.or(focused);
    if let Some(entity) = hover {
        apply_pointer(world, entity, &input, hit == Some(entity));
    }
}

fn drive_update(world: &mut World) {
    let blink = !events::<BlinkTick>(world).is_empty();
    let clipboard = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    let ids: Vec<u64> = world
        .local::<Sessions>()
        .map(|store| store.map.keys().copied().collect())
        .unwrap_or_default();
    if blink {
        if let Some(store) = world.local_mut::<Sessions>() {
            store.phase = !store.phase;
            store.blink = None;
        }
    }
    let mut dead = Vec::new();
    for id in ids {
        let entity = match world
            .local::<Sessions>()
            .and_then(|store| store.map.get(&id))
        {
            Some(slot) => slot.entity,
            None => continue,
        };
        if !world.is_alive(entity) || !world.has::<Terminal>(entity) {
            dead.push(id);
            continue;
        }
        let Some(mut session) = take_session(world, id) else {
            continue;
        };
        if !session.pin_cell {
            if let Some(grid) = world.get::<MonoGrid>(entity) {
                let (w, h) = measure_cell(world, &grid.font);
                if (w - session.cell_w).abs() > 0.5 || (h - session.cell_h).abs() > 0.5 {
                    session.cell_w = w;
                    session.cell_h = h;
                }
            }
        }
        if let Some(rect) = world.get::<Rect>(entity).copied() {
            if let Some((cols, rows)) = fit(rect, session.cell_w, session.cell_h) {
                session.resize(cols, rows, session.cell_w, session.cell_h);
            }
        }
        let pump = session.pump(&clipboard);
        let cursor_blinks = session.cursor_blinks;
        put_session(world, id, session);
        publish_one(world, id);
        if cursor_blinks {
            let phase = world.local::<Sessions>().is_none_or(|store| store.phase);
            if let Some(grid) = world.get_mut::<MonoGrid>(entity) {
                if let Some(cursor) = grid.cursor.as_mut() {
                    cursor.visible = phase;
                }
            }
        }
        if let Some(title) = pump.title {
            send(world, TerminalTitle { entity, id, title });
        }
        if pump.bell {
            send(world, TerminalBell { entity, id });
        }
        if let Some(text) = pump.copy {
            set_clipboard(world, &text);
        }
        if let Some(code) = pump.exit {
            send(world, TerminalExited { entity, id, code });
        }
    }
    if let Some(store) = world.local_mut::<Sessions>() {
        for id in dead {
            store.map.remove(&id);
        }
    }
    let wants_motion = motion_wanted(world);
    sync_motion(world, wants_motion);
    arm_blink(world);
}

fn apply_keys(world: &mut World, entity: Entity, input: &Input, platform: Platform) {
    let Some(id) = world.get::<Terminal>(entity).map(|terminal| terminal.id) else {
        return;
    };
    let Some(mut session) = take_session(world, id) else {
        return;
    };
    let mode = encode_mode(&session);
    let bracketed = session
        .modes()
        .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
    let composing = !input.ime_preedit.is_empty();
    let mut bytes = Vec::new();
    let mut copied = None;
    let clip = world
        .resource::<Clipboard>()
        .map(|clipboard| clipboard.text.clone())
        .unwrap_or_default();
    if !composing && input.ime_commit.is_empty() {
        for event in &input.keys {
            if let Some(chord) = copy_or_paste(event, platform) {
                match chord {
                    Chord::Copy => copied = session.selection_text(),
                    Chord::Paste => bytes.extend(encode_paste(&clip, bracketed)),
                }
                continue;
            }
            if local_scroll(event, &mut session) {
                continue;
            }
            bytes.extend(encode_key(event, mode));
        }
    } else if !input.ime_commit.is_empty() {
        bytes.extend(input.ime_commit.as_bytes());
    }
    if !bytes.is_empty() {
        session.write_bytes(&bytes);
    }
    let show = !bytes.is_empty();
    put_session(world, id, session);
    if show {
        if let Some(store) = world.local_mut::<Sessions>() {
            store.phase = true;
        }
    }
    if let Some(text) = copied {
        set_clipboard(world, &text);
    }
    if let Some(input) = world.resource_mut::<Input>() {
        input.keys.clear();
        input.chars.clear();
        input.key = None;
        input.ime_commit.clear();
    }
}

fn apply_pointer(world: &mut World, entity: Entity, input: &Input, inside: bool) {
    let Some(id) = world.get::<Terminal>(entity).map(|terminal| terminal.id) else {
        return;
    };
    let Some(rect) = world.get::<Rect>(entity).copied() else {
        return;
    };
    let Some(grid) = world.get::<MonoGrid>(entity).cloned() else {
        return;
    };
    let Some(mut session) = take_session(world, id) else {
        return;
    };
    let mouse = mouse_of(&session);
    let shift = input.shift;
    let reporting = mouse.protocol != MouseProtocol::Off && !shift;
    let cell = cell_at(
        rect,
        &grid,
        input.pointer.position[0],
        input.pointer.position[1],
    );
    let mut bytes = Vec::new();
    let scroll = input.scroll;
    if inside && (scroll[0] != 0.0 || scroll[1] != 0.0) {
        let lines = scroll_lines(scroll[1], grid.cell_h);
        if reporting {
            let (col, row) = cell.unwrap_or((0, 0));
            let button = if lines > 0 {
                MouseButton::WheelUp
            } else {
                MouseButton::WheelDown
            };
            let mods = modifiers_of(input);
            for _ in 0..lines.unsigned_abs() {
                bytes.extend(encode_mouse(button, Action::Press, col, row, mods, mouse));
            }
        } else if session.alt_scroll() {
            let key = if lines > 0 {
                NamedKey::ArrowUp
            } else {
                NamedKey::ArrowDown
            };
            let event = KeyEvent::named(key, Modifiers::default());
            let encoded = encode_key(&event, encode_mode(&session));
            for _ in 0..lines.unsigned_abs().saturating_mul(3) {
                bytes.extend(encoded.iter().copied());
            }
        } else {
            session.scroll_by(lines);
        }
    }
    if inside {
        if let Some((col, row)) = cell {
            let button = mouse_button(input.pointer.button);
            let mods = modifiers_of(input);
            if reporting {
                if input.pointer.pressed {
                    bytes.extend(encode_mouse(button, Action::Press, col, row, mods, mouse));
                } else if input.pointer.released {
                    bytes.extend(encode_mouse(button, Action::Release, col, row, mods, mouse));
                } else if input.pointer.down || mouse.protocol == MouseProtocol::Any {
                    let motion = if input.pointer.down {
                        button
                    } else {
                        MouseButton::None
                    };
                    bytes.extend(encode_mouse(motion, Action::Move, col, row, mods, mouse));
                }
            } else if button == MouseButton::Middle && input.pointer.pressed {
                let text = paste_selection(world);
                let bracketed = session
                    .modes()
                    .contains(alacritty_terminal::term::TermMode::BRACKETED_PASTE);
                bytes.extend(encode_paste(&text, bracketed));
            } else if button == MouseButton::Left || button == MouseButton::None {
                let origin = if input.pointer.pressed {
                    let at = input.pointer.press_at.unwrap_or(input.pointer.position);
                    cell_at(rect, &grid, at[0], at[1])
                } else {
                    None
                };
                if let Some((origin_col, origin_row)) = origin {
                    session.start_selection(origin_col as usize, origin_row as usize, input.shift);
                    if (origin_col, origin_row) != (col, row) {
                        session.drag_selection(col as usize, row as usize);
                    }
                } else if input.pointer.down && session.selecting {
                    session.drag_selection(col as usize, row as usize);
                }
                if input.pointer.released && session.selecting {
                    session.drag_selection(col as usize, row as usize);
                    if let Some(text) = session.selection_text() {
                        set_primary(world, &text);
                    }
                    session.selecting = false;
                }
            }
        }
    } else if input.pointer.released {
        session.selecting = false;
    }
    if !bytes.is_empty() {
        session.write_bytes(&bytes);
    }
    let consumed_scroll = inside && (scroll[0] != 0.0 || scroll[1] != 0.0);
    put_session(world, id, session);
    if consumed_scroll {
        if let Some(input) = world.resource_mut::<Input>() {
            input.scroll = [0.0, 0.0];
        }
    }
}

fn publish_one(world: &mut World, id: u64) {
    let Some(mut session) = take_session(world, id) else {
        return;
    };
    let entity = world
        .local::<Sessions>()
        .and_then(|store| store.map.get(&id))
        .map(|slot| slot.entity);
    let Some(entity) = entity else {
        put_session(world, id, session);
        return;
    };
    if let Some(grid) = world.get_mut::<MonoGrid>(entity) {
        if (grid.cell_w - session.cell_w).abs() > 0.01
            || (grid.cell_h - session.cell_h).abs() > 0.01
        {
            grid.cell_w = session.cell_w;
            grid.cell_h = session.cell_h;
            for gen in &mut grid.row_gen {
                *gen = gen.wrapping_add(1);
            }
        }
        screen::publish(&mut session.term, grid);
    }
    let phase = world.local::<Sessions>().is_none_or(|store| store.phase);
    if session.cursor_blinks && !phase {
        if let Some(grid) = world.get_mut::<MonoGrid>(entity) {
            if let Some(cursor) = grid.cursor.as_mut() {
                cursor.visible = false;
            }
        }
    }
    put_session(world, id, session);
}

fn take_session(world: &mut World, id: u64) -> Option<Session> {
    world
        .local_mut::<Sessions>()?
        .map
        .get_mut(&id)?
        .session
        .take()
}

fn put_session(world: &mut World, id: u64, session: Session) {
    if let Some(slot) = world
        .local_mut::<Sessions>()
        .and_then(|store| store.map.get_mut(&id))
    {
        slot.session = Some(session);
    }
}

fn ensure_sessions(world: &mut World) {
    if !world.has_local::<Sessions>() {
        world.insert_local(Sessions::default());
    }
}

fn set_clipboard(world: &mut World, text: &str) {
    if let Some(clipboard) = world.resource_mut::<Clipboard>() {
        clipboard.text = text.to_string();
        clipboard.dirty = true;
    } else {
        world.insert_resource(Clipboard {
            text: text.to_string(),
            dirty: true,
            ..Clipboard::default()
        });
    }
}

fn set_primary(world: &mut World, text: &str) {
    if let Some(clipboard) = world.resource_mut::<Clipboard>() {
        clipboard.primary = text.to_string();
        clipboard.primary_dirty = true;
    } else {
        world.insert_resource(Clipboard {
            primary: text.to_string(),
            primary_dirty: true,
            ..Clipboard::default()
        });
    }
}

/// Middle-click paste. The primary selection wins when the platform has one.
fn paste_selection(world: &World) -> String {
    let Some(clipboard) = world.resource::<Clipboard>() else {
        return String::new();
    };
    if !clipboard.primary.is_empty() {
        clipboard.primary.clone()
    } else {
        clipboard.text.clone()
    }
}

fn encode_mode(session: &Session) -> EncodeMode {
    let mode = session.modes();
    EncodeMode {
        app_cursor: mode.contains(alacritty_terminal::term::TermMode::APP_CURSOR),
        app_keypad: mode.contains(alacritty_terminal::term::TermMode::APP_KEYPAD),
        newline: mode.contains(alacritty_terminal::term::TermMode::LINE_FEED_NEW_LINE),
    }
}

fn mouse_of(session: &Session) -> MouseEncode {
    let mode = session.modes();
    let protocol = if mode.contains(alacritty_terminal::term::TermMode::MOUSE_MOTION) {
        MouseProtocol::Any
    } else if mode.contains(alacritty_terminal::term::TermMode::MOUSE_DRAG) {
        MouseProtocol::Drag
    } else if mode.contains(alacritty_terminal::term::TermMode::MOUSE_REPORT_CLICK) {
        MouseProtocol::Click
    } else {
        MouseProtocol::Off
    };
    MouseEncode {
        sgr: mode.contains(alacritty_terminal::term::TermMode::SGR_MOUSE),
        protocol,
    }
}

fn motion_wanted(world: &World) -> bool {
    let Some(store) = world.local::<Sessions>() else {
        return false;
    };
    store.map.values().any(|slot| {
        slot.session
            .as_ref()
            .is_some_and(|session| mouse_of(session).protocol == MouseProtocol::Any)
    })
}

fn sync_motion(world: &mut World, wants: bool) {
    let set = world.local::<Sessions>().is_some_and(|store| store.motion);
    if wants && !set {
        if let Some(watch) = world.resource_mut::<MotionWatch>() {
            watch.active = true;
        } else {
            world.insert_resource(MotionWatch { active: true });
        }
        if let Some(store) = world.local_mut::<Sessions>() {
            store.motion = true;
        }
    } else if !wants && set {
        if let Some(watch) = world.resource_mut::<MotionWatch>() {
            watch.active = false;
        }
        if let Some(store) = world.local_mut::<Sessions>() {
            store.motion = false;
        }
    }
}

fn arm_blink(world: &mut World) {
    let (needed, stop) = world
        .local::<Sessions>()
        .map(|store| {
            let blinking = store.map.values().any(|slot| {
                slot.session
                    .as_ref()
                    .is_some_and(|session| session.cursor_blinks)
            });
            (
                blinking && store.blink.is_none(),
                !blinking && store.blink.is_some(),
            )
        })
        .unwrap_or((false, false));
    if stop {
        if let Some(store) = world.local_mut::<Sessions>() {
            store.blink = None;
        }
        return;
    }
    if !needed {
        return;
    }
    let handle = after(world, BLINK, BlinkTick);
    if let Some(store) = world.local_mut::<Sessions>() {
        store.blink = Some(handle);
    }
}

fn local_scroll(event: &KeyEvent, session: &mut Session) -> bool {
    if event.released || event.modifiers.ctrl || event.modifiers.alt || !event.modifiers.shift {
        return false;
    }
    match event.logical {
        LogicalKey::Named(NamedKey::PageUp) => {
            session.scroll_page(true);
            true
        }
        LogicalKey::Named(NamedKey::PageDown) => {
            session.scroll_page(false);
            true
        }
        _ => false,
    }
}

enum Chord {
    Copy,
    Paste,
}

fn copy_or_paste(event: &KeyEvent, platform: Platform) -> Option<Chord> {
    if event.released || event.synthetic {
        return None;
    }
    if event.logical == LogicalKey::Named(NamedKey::Insert)
        && event.modifiers.shift
        && !event.modifiers.ctrl
    {
        return Some(Chord::Paste);
    }
    let letter = match event.logical {
        LogicalKey::Character(ch) if ch.is_ascii_alphabetic() => Some(ch.to_ascii_lowercase()),
        _ => match event.code {
            Some(KeyCode::Letter(ch)) => Some(ch.to_ascii_lowercase()),
            _ => None,
        },
    }?;
    let mods = event.modifiers;
    let chord = if platform.primary_is_super() {
        mods.super_key && !mods.ctrl && !mods.alt
    } else {
        mods.ctrl && mods.shift && !mods.super_key
    };
    if !chord {
        return None;
    }
    match letter {
        'c' => Some(Chord::Copy),
        'v' => Some(Chord::Paste),
        _ => None,
    }
}

fn modifiers_of(input: &Input) -> Modifiers {
    Modifiers {
        shift: input.shift,
        ctrl: input.ctrl,
        alt: input.alt,
        super_key: input.super_key,
    }
}

fn mouse_button(button: PointerButton) -> MouseButton {
    match button {
        PointerButton::Left => MouseButton::Left,
        PointerButton::Middle => MouseButton::Middle,
        PointerButton::Right => MouseButton::Right,
        PointerButton::Other(_) => MouseButton::Left,
    }
}

fn cell_at(rect: Rect, grid: &MonoGrid, x: f32, y: f32) -> Option<(u16, u16)> {
    if grid.cols == 0 || grid.rows == 0 || !rect.contains(x, y) {
        return None;
    }
    let col = ((x - rect.x) / grid.cell_w).floor();
    let row = ((y - rect.y) / grid.cell_h).floor();
    if !col.is_finite() || !row.is_finite() || col < 0.0 || row < 0.0 {
        return None;
    }
    let col = (col as u16).min(grid.cols - 1);
    let row = (row as u16).min(grid.rows - 1);
    Some((col, row))
}

fn scroll_lines(dy: f32, cell_h: f32) -> i32 {
    if dy == 0.0 || !dy.is_finite() {
        return 0;
    }
    let cells = (dy / cell_h.max(1.0)).round() as i32;
    if cells == 0 {
        dy.signum() as i32
    } else {
        cells
    }
}

fn fit(rect: Rect, cell_w: f32, cell_h: f32) -> Option<(u16, u16)> {
    if rect.w < cell_w || rect.h < cell_h {
        return None;
    }
    let cols = (rect.w / cell_w).floor();
    let rows = (rect.h / cell_h).floor();
    if !cols.is_finite() || !rows.is_finite() {
        return None;
    }
    Some(((cols as u16).clamp(2, 500), (rows as u16).clamp(1, 500)))
}

fn measure_cell(world: &World, style: &TextStyle) -> (f32, f32) {
    let engine = text_engine(world);
    let width = engine.unit_advance(style).max(1.0);
    let height = engine.line_height(style).max(width);
    (width, height)
}

fn default_font() -> TextStyle {
    TextStyle {
        family: "monospace".into(),
        size: 16.0,
        weight: FONT_WEIGHT_REGULAR,
        style: rndr_core::FontStyle::Normal,
    }
}

fn default_shell() -> String {
    #[cfg(unix)]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "sh".into())
    }
    #[cfg(windows)]
    {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        "sh".into()
    }
}
