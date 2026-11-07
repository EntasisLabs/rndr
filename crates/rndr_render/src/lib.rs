//! winit + wgpu plugin. Writes [`rndr_core::Input`] in logical points and
//! presents a frame. Quads stay on the signed-distance path. A display list
//! of paths, images, glyphs, and layers is drawn in the same frame.
//!
//! [`run`] is the only function here that opens a window. It reads a
//! [`rndr_core::WindowConfig`] resource when one is present. Otherwise the
//! window stays 800×600, decorated, resizable, and titled `rndr`. Hide and
//! show leave the wgpu surface in place.
//!
//! # Live observation
//!
//! Feature `observe` is off by default and also requires an explicit session
//! and whole-frame public-content attestation. It captures the live whole-window
//! surface after ordinary draw commands, with the matching flattened accessibility
//! tree from the paint boundary; privacy or supported post-paint state changes
//! deny both channels. The transport is Unix-only. It is not component/UiId or
//! layout-tree capture, headless rendering, automatic secret detection, or proof
//! that the OS compositor displayed the frame. See `docs/native-observation.md`
//! in the repository for limits, privacy policy and the Python client/tests.
//!
//! # Example
//!
//! ```no_run
//! use rndr_core::{App, Color, Label};
//! use rndr_render::run;
//!
//! fn main() {
//!     let mut app = App::new();
//!     app.world_mut()
//!         .spawn((Label::new("hello"), Color::rgb(0.12, 0.13, 0.16)));
//!     run(app);
//! }
//! ```
//!
//! `hotkey`, `tray`, `parley`, and `accesskit` are on by default. `dialog` is
//! off by default and shows native open and save dialogs. docs.rs builds every
//! feature. Its Linux image provides fontconfig (used by `parley`), plus
//! libxkbcommon, Wayland, and Vulkan for winit and wgpu. `hotkey` and `tray`
//! do not link libdbus. Linux dialogs use the file-chooser portal.

#![deny(missing_docs)]

#[cfg(feature = "accesskit")]
mod a11y;
#[cfg(feature = "accesskit")]
mod a11y_text;
#[cfg(feature = "accesskit")]
mod access_winit;
/// Opt-in component-targeted offscreen rendering; no native window or socket.
#[cfg(feature = "observe")]
pub mod capture;
mod clipboard;
mod close;
mod color;
mod decode;
#[cfg(feature = "dialog")]
mod dialogs;
mod export;
mod file_drop;
mod gpu;
#[cfg(feature = "hotkey")]
mod hotkey;
mod images;
#[cfg(target_os = "macos")]
mod native_quit;
#[cfg(feature = "observe")]
pub mod observe;
mod outline;
#[cfg(target_os = "linux")]
mod overlay_keys;
mod pen;
mod platform;
mod scene;
mod shell;
mod startup;
mod tablet;
#[cfg(feature = "parley")]
mod text;
#[cfg(feature = "tray")]
mod tray;
#[cfg(target_os = "linux")]
mod wayland_layer;

#[cfg(feature = "parley")]
pub use text::ParleyText;

use std::hash::{Hash, Hasher};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "parley")]
use rndr_core::TextEngine;
use rndr_core::{
    deliver, ensure_text_engine, legacy_key, resolve_ime, send, text_engine, touch_id, App, Caret,
    Clipboard, ContactPhase, ContactUpdate, CursorImage, CursorStyle, Focus, Gesture, GesturePhase,
    GestureState, ImeEvent, Input, KeyCode as Physical, KeyEvent, LogicalKey, Modifiers,
    MotionWatch, NamedKey as CoreNamed, Platform, Pointer, PointerButton, PointerButtons,
    PointerKind, Pointers, Rect, TextLayout, Viewport, Wake, WindowConfig, WindowEffect, WindowHub,
    WindowKind, MOUSE_ID, PEN_ID, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
#[cfg(not(feature = "observe"))]
use rndr_paint::frame;
use rndr_paint::{atlas_r8, ATLAS_H, ATLAS_W};
use winit::application::ApplicationHandler;
use winit::cursor::{CursorIcon, CustomCursor, CustomCursorSource};
use winit::event::{
    ButtonSource, ElementState, Ime, MouseButton, MouseScrollDelta, PointerSource, StartCause,
    TouchPhase, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::WindowId;

use gpu::Gpu;

pub use decode::{DecodeError, Raster};
pub use export::{
    encode_png, render_frame_rgba, render_scene_rgba, ExportError, Exporter, RgbaImage,
    EXPORT_MAX_PIXELS, EXPORT_MAX_SIDE,
};
#[cfg(feature = "parley")]
use gpu::{AtlasKind, AtlasRegion, AtlasUpload};
pub use images::{
    decode, load_image, load_rgba, set_image_budget, DecodedImage, ImageError, IMAGE_BUDGET_DEFAULT,
};
pub use startup::{device_init_plan, DeviceInit, StartupTrace};

/// User event the runtime posts when a task finishes, a timer is armed, or the
/// tray icon is clicked. The tray variant is what wakes `Wait` while the
/// window is hidden.
#[derive(Debug)]
enum RunnerEvent {
    Wake,
    #[cfg(target_os = "macos")]
    Quit,
    #[cfg(feature = "tray")]
    Tray(rndr_core::TrayReport),
    #[cfg(feature = "accesskit")]
    Access(access_winit::Event),
}

/// Posts a [`RunnerEvent`] and wakes the loop.
///
/// winit 0.31's proxy only wakes. The event itself travels on this channel,
/// and the wake is sent after the send so the loop observes it.
struct RunnerBus {
    tx: std::sync::mpsc::Sender<RunnerEvent>,
    proxy: EventLoopProxy,
}

impl Clone for RunnerBus {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            proxy: self.proxy.clone(),
        }
    }
}

impl RunnerBus {
    fn post(&self, event: RunnerEvent) {
        let _ = self.tx.send(event);
        self.proxy.wake_up();
    }

    fn wake(&self) {
        self.post(RunnerEvent::Wake);
    }

    #[cfg(target_os = "macos")]
    fn quit(&self) {
        self.post(RunnerEvent::Quit);
    }

    #[cfg(feature = "tray")]
    fn tray(&self, report: rndr_core::TrayReport) {
        self.post(RunnerEvent::Tray(report));
    }

    #[cfg(feature = "accesskit")]
    fn access(&self, event: access_winit::Event) {
        self.post(RunnerEvent::Access(event));
    }
}

struct ProxyWake {
    bus: RunnerBus,
}

impl Wake for ProxyWake {
    fn wake(&self) {
        self.bus.wake();
    }
}

/// `Wait` when nothing is scheduled, `WaitUntil` the next timer otherwise.
/// A deadline that is already due stays on `Wait`: the caller requests a redraw,
/// and that queued event wakes the loop without spinning.
pub(crate) fn idle_control_flow(deadline: Option<Instant>, now: Instant) -> ControlFlow {
    match deadline {
        Some(when) if when > now => ControlFlow::WaitUntil(when),
        _ => ControlFlow::Wait,
    }
}

/// Chord used when neither `--hotkey` nor `RNDR_HOTKEY` is set.
pub const DEFAULT_HOTKEY: &str = "Alt+Space";

/// Binding from a `--hotkey` argument, then the `RNDR_HOTKEY` environment
/// variable, then [`DEFAULT_HOTKEY`].
///
/// `--hotkey VALUE` and `--hotkey=VALUE` both count. The first one wins.
pub fn configured_hotkey<I, S>(args: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let env = std::env::var("RNDR_HOTKEY").ok();
    select_hotkey(args, env.as_deref())
}

/// Same order as [`configured_hotkey`], with the environment value passed in.
pub fn select_hotkey<I, S>(args: I, env: Option<&str>) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        if let Some(value) = arg.strip_prefix("--hotkey=") {
            if !value.is_empty() {
                return value.to_string();
            }
        }
        if arg == "--hotkey" {
            if let Some(value) = args.next() {
                let value = value.as_ref();
                if !value.is_empty() {
                    return value.to_string();
                }
            }
        }
    }
    match env.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => value.to_string(),
        None => DEFAULT_HOTKEY.to_string(),
    }
}

/// Grab `binding` for the life of the returned handle.
///
/// Installs the operating-system backend on the first call when the `hotkey`
/// feature is on. With the feature off, or with no backend this session can
/// build, this returns [`rndr_core::HotkeyError::Unsupported`].
pub fn register(
    world: &mut rndr_core::World,
    binding: impl rndr_core::hotkey::IntoHotkey,
) -> Result<rndr_core::HotkeyHandle, rndr_core::HotkeyError> {
    #[cfg(feature = "hotkey")]
    hotkey::ensure(world);
    rndr_core::hotkey::register(world, binding)
}

/// Grab `binding` and show+focus or hide the window on each press.
///
/// This is the launcher case: one call, hold the handle for the process.
/// The press is also delivered as [`rndr_core::GlobalHotkey`].
pub fn toggle_window(
    world: &mut rndr_core::World,
    binding: impl rndr_core::hotkey::IntoHotkey,
) -> Result<rndr_core::HotkeyHandle, rndr_core::HotkeyError> {
    #[cfg(feature = "hotkey")]
    hotkey::ensure(world);
    rndr_core::hotkey::toggle_window(world, binding)
}

/// One or more windows driven by a single event loop.
///
/// [`Session::new`] is the window `run` opens. [`Session::window`] adds another
/// app, with its own world and surface. [`rndr_core::Desktop::open`] can add more after
/// the loop has started. Closing the first window exits. Closing a later
/// window removes only that window. [`Input::quit`] on any window exits.
/// An opt-in [`rndr_core::CloseGuard`] delays either path until protection is
/// ready. Process exit drains every window, including hidden/secondary windows.
///
/// # Panics
///
/// Panics if the process cannot create the event loop or a window, or if the
/// loop returns an error. A redraw after a window exists also panics if that
/// surface was not created.
pub struct Session {
    apps: Vec<App>,
}

impl Session {
    /// A session whose first window is `app`.
    pub fn new(app: App) -> Self {
        Self { apps: vec![app] }
    }

    /// Queue `app` as another window, created when [`Session::run`] starts.
    pub fn window(mut self, app: App) -> Self {
        self.apps.push(app);
        self
    }

    /// Block on the desktop event loop until the session exits.
    pub fn run(self) {
        run_apps(self.apps);
    }
}

/// Block on the desktop event loop. Core stays headless; this is the only winit caller.
///
/// Returns when the window is closed, [`rndr_core::Input::quit`] is set, or a
/// tray item built to quit the process fires. Hide leaves the process running.
/// This is [`Session::new`]`(app).`[`Session::run`]`()`.
///
/// # Panics
///
/// Panics if the process cannot create the event loop or the window, or if
/// the loop returns an error. A redraw after that window exists also panics
/// if the surface was not created.
pub fn run(app: App) {
    Session::new(app).run();
}

fn run_apps(apps: Vec<App>) {
    let mut apps = apps;
    if apps.is_empty() {
        return;
    }
    #[cfg(feature = "hotkey")]
    hotkey::ensure(apps[0].world_mut());
    let config = WindowConfig::get(apps[0].world());
    for app in &mut apps {
        install_text_engine(app);
    }
    if rndr_core::tray::starts_hidden(apps[0].world()) {
        rndr_core::tray::mark_hidden(apps[0].world_mut());
    }
    let mut builder = EventLoop::builder();
    platform::configure_event_loop(&mut builder, &config);
    let event_loop = builder.build().expect("event loop");
    let (tx, rx) = std::sync::mpsc::channel();
    let bus = RunnerBus {
        tx,
        proxy: event_loop.create_proxy(),
    };
    install_tray_handlers(&bus);
    let wake: Arc<dyn Wake> = Arc::new(ProxyWake { bus: bus.clone() });
    let hub = WindowHub::new();
    hub.set_wake(Arc::clone(&wake));
    let pool = rndr_core::thread_pool();
    let pen = pen::PenFeed::new();
    let mut slots = Vec::with_capacity(apps.len());
    for (index, mut app) in apps.into_iter().enumerate() {
        let desktop = hub.attach();
        let desktop_id = desktop.id();
        app.world_mut().insert_resource(desktop);
        app.set_wake(Arc::clone(&wake));
        app.set_executor(Arc::clone(&pool));
        let config = WindowConfig::get(app.world());
        slots.push(Handler::new(
            app,
            config,
            index == 0,
            desktop_id,
            pen.clone(),
            bus.clone(),
        ));
    }
    event_loop.set_control_flow(ControlFlow::Wait);
    let session = Running {
        slots,
        hub,
        wake,
        events: rx,
        bus,
        pen,
        executor: pool,
        close: close::CloseDrain::default(),
    };
    event_loop.run_app(session).expect("run");
    #[cfg(target_os = "macos")]
    native_quit::clear();
}

fn install_text_engine(app: &mut App) {
    #[cfg(feature = "parley")]
    if (!app.world().has_resource::<Arc<dyn TextEngine>>()
        && !app.world().has_resource::<rndr_core::TextFace>())
        || rndr_core::has_implicit_text_engine(app.world())
    {
        let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::new());
        app.world_mut().insert_resource(engine);
    }
    #[cfg(not(feature = "parley"))]
    let _ = app;
}

fn install_tray_handlers(bus: &RunnerBus) {
    #[cfg(feature = "tray")]
    {
        let icon_bus = bus.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if let Some(report) = tray::map_icon_event(&event) {
                icon_bus.tray(report);
            }
        }));
        let menu_bus = bus.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |event| {
            if let Some(report) = tray::map_menu_event(&event) {
                menu_bus.tray(report);
            }
        }));
    }
    #[cfg(not(feature = "tray"))]
    {
        let _ = bus;
    }
}

struct Handler {
    app: App,
    config: WindowConfig,
    gpu: Option<Gpu>,
    /// Device creation started off the UI thread. The shell stays here until
    /// the worker returns, so show and focus can be applied without waiting.
    pending_rx: Option<mpsc::Receiver<gpu::GpuParts>>,
    pending_shell: Option<shell::Shell>,
    stashed_effects: Vec<WindowEffect>,
    trace: StartupTrace,
    winit_id: Option<WindowId>,
    /// First window. Closing it exits the session.
    primary: bool,
    desktop_id: rndr_core::WindowId,
    /// Last cursor in physical pixels. Converted to points in [`Handler::sync_input`].
    cursor_px: [f32; 2],
    pointer: Pointer,
    pointers: Pointers,
    gestures: GestureState,
    queued_gestures: Vec<Gesture>,
    queued_files: Vec<file_drop::NativeFile>,
    /// Paths in the current hover. Cancel clears every entry.
    hovered_files: rndr_core::FileHoverSet,
    scroll: [f32; 2],
    keys: Vec<KeyEvent>,
    ime_events: Vec<ImeEvent>,
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_key: bool,
    platform: Platform,
    /// Receive-order stamped onto each [`KeyEvent`]. Starts at 0; the first
    /// key is 1. [`KeyEvent::UNORDERED`] is reserved and never assigned.
    key_order: u64,
    /// Cursor in physical pixels at the button-down this frame.
    press_px: Option<[f32; 2]>,
    os_clipboard: clipboard::OsClipboard,
    /// The window was created hidden so the first present is the first frame
    /// the user sees. Cleared after [`Handler::reveal_window`].
    show_after_present: bool,
    cursor_cache: Option<(u64, CustomCursor)>,
    pen_seen: u64,
    accepts_pen: bool,
    close: close::CloseSlot,
    started: Instant,
    pen: pen::PenFeed,
    /// Cloned into the accessibility adapter when that feature is enabled.
    #[cfg_attr(not(feature = "accesskit"), allow(dead_code))]
    bus: RunnerBus,
    drag: file_drop::DragTracker,
    next_fetch: u64,
    outstanding_fetch: Option<OutstandingFetch>,
    #[cfg(feature = "accesskit")]
    access: Option<access_winit::Adapter>,
    #[cfg(feature = "accesskit")]
    access_bridge: a11y::Bridge,
    #[cfg(feature = "tray")]
    tray: tray::Session,
}

impl Handler {
    fn new(
        mut app: App,
        config: WindowConfig,
        primary: bool,
        desktop_id: rndr_core::WindowId,
        pen: pen::PenFeed,
        bus: RunnerBus,
    ) -> Self {
        app.world_mut()
            .insert_resource(rndr_core::FileHoverSet::default());
        Self {
            app,
            config,
            gpu: None,
            pending_rx: None,
            pending_shell: None,
            stashed_effects: Vec::new(),
            trace: StartupTrace::start(),
            winit_id: None,
            primary,
            desktop_id,
            cursor_px: [0.0, 0.0],
            pointer: Pointer::default(),
            pointers: Pointers::new(),
            gestures: GestureState::new(),
            queued_gestures: Vec::new(),
            queued_files: Vec::new(),
            hovered_files: rndr_core::FileHoverSet::default(),
            scroll: [0.0, 0.0],
            keys: Vec::new(),
            ime_events: Vec::new(),
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
            platform: host_platform(),
            key_order: 0,
            press_px: None,
            os_clipboard: clipboard::OsClipboard::new(),
            show_after_present: false,
            cursor_cache: None,
            pen_seen: 0,
            accepts_pen: true,
            close: close::CloseSlot::default(),
            started: Instant::now(),
            pen,
            bus,
            drag: file_drop::DragTracker::default(),
            next_fetch: 1,
            outstanding_fetch: None,
            #[cfg(feature = "accesskit")]
            access: None,
            #[cfg(feature = "accesskit")]
            access_bridge: a11y::Bridge::default(),
            #[cfg(feature = "tray")]
            tray: tray::Session::new(),
        }
    }
}

fn map_pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Left => PointerButton::Left,
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        MouseButton::Back => PointerButton::Other(8),
        MouseButton::Forward => PointerButton::Other(9),
        MouseButton::Button6 => PointerButton::Other(6),
        MouseButton::Button7 => PointerButton::Other(7),
        MouseButton::Button8 => PointerButton::Other(8),
        MouseButton::Button9 => PointerButton::Other(9),
        MouseButton::Button10 => PointerButton::Other(10),
        MouseButton::Button11 => PointerButton::Other(11),
        MouseButton::Button12 => PointerButton::Other(12),
        MouseButton::Button13 => PointerButton::Other(13),
        MouseButton::Button14 => PointerButton::Other(14),
        MouseButton::Button15 => PointerButton::Other(15),
        MouseButton::Button16 => PointerButton::Other(16),
        MouseButton::Button17 => PointerButton::Other(17),
        MouseButton::Button18 => PointerButton::Other(18),
        MouseButton::Button19 => PointerButton::Other(19),
        MouseButton::Button20 => PointerButton::Other(20),
        MouseButton::Button21 => PointerButton::Other(21),
        MouseButton::Button22 => PointerButton::Other(22),
        MouseButton::Button23 => PointerButton::Other(23),
        MouseButton::Button24 => PointerButton::Other(24),
        MouseButton::Button25 => PointerButton::Other(25),
        MouseButton::Button26 => PointerButton::Other(26),
        MouseButton::Button27 => PointerButton::Other(27),
        MouseButton::Button28 => PointerButton::Other(28),
        MouseButton::Button29 => PointerButton::Other(29),
        MouseButton::Button30 => PointerButton::Other(30),
        MouseButton::Button31 => PointerButton::Other(31),
        MouseButton::Button32 => PointerButton::Other(32),
    }
}

/// Physical pixels per point. A missing or non-positive factor reads as 1.
pub(crate) fn positive_scale(factor: f64) -> f32 {
    let scale = factor as f32;
    if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    }
}

// Winit uses native implicit mouse capture (AppKit drag/up delivery, Windows
// SetCapture, X11/Wayland button grabs). CursorGrabMode confines/locks the
// cursor and is not selection capture. Keep coordinates free to leave the
// window; clear held state on blur in case capture/release delivery was lost.
fn cancel_pointer_input(pointer: &mut Pointer, pointers: &mut Pointers) {
    pointer.down = false;
    pointer.pressed = false;
    pointer.released = true;
    pointer.press_at = None;
    *pointers = Pointers::default();
}

fn gesture_phase(phase: TouchPhase) -> GesturePhase {
    match phase {
        TouchPhase::Started => GesturePhase::Started,
        TouchPhase::Moved => GesturePhase::Changed,
        TouchPhase::Ended => GesturePhase::Ended,
        TouchPhase::Cancelled => GesturePhase::Cancelled,
    }
}

impl Handler {
    fn apply_mouse(&mut self, phase: ContactPhase) {
        let scale = self.scale();
        let position = [self.cursor_px[0] / scale, self.cursor_px[1] / scale];
        self.pointer.position = position;
        let mut buttons = PointerButtons::new();
        if self.pointer.down {
            buttons.set(self.pointer.button, true);
        }
        self.pointers.apply(ContactUpdate {
            id: MOUSE_ID,
            kind: PointerKind::Mouse,
            position,
            pressure: None,
            tilt: None,
            twist: None,
            buttons,
            button: self.pointer.button,
            phase,
            time: self.started.elapsed(),
        });
    }

    fn scale(&self) -> f32 {
        self.gpu
            .as_ref()
            .map(|gpu| positive_scale(gpu.shell().scale_factor()))
            .unwrap_or(1.0)
    }

    fn sync_input(&mut self) {
        let scale = self.scale();
        self.pointer.id = MOUSE_ID;
        self.pointer.position = [self.cursor_px[0] / scale, self.cursor_px[1] / scale];
        self.pointer.press_at = self.press_px.map(|px| [px[0] / scale, px[1] / scale]);
        let pen = self.pen.clone();
        self.note_pen(&pen);
        if let Some(id) = self
            .app
            .world()
            .resource::<Pointers>()
            .map(|pointers| pointers.primary_id())
        {
            self.pointers.set_primary(id);
        }
        let mut gestures = self.gestures.update(&self.pointers);
        gestures.append(&mut self.queued_gestures);
        let resolved = resolve_ime(&self.ime_events);
        let platform = self.platform;
        let mirrored = self.pointers.mirror();
        self.app.world_mut().insert_resource(platform);
        self.app.world_mut().insert_resource(self.pointers.clone());
        if let Some(input) = self.app.world_mut().resource_mut::<Input>() {
            input.pointer = mirrored.unwrap_or(self.pointer);
            input.scroll = self.scroll;
            input.keys.clone_from(&resolved.keys);
            input.key = resolved
                .keys
                .iter()
                .find(|event| !event.released)
                .and_then(|event| legacy_key(event, platform));
            input.chars.clear();
            for event in resolved.keys.iter().filter(|event| !event.released) {
                if let Some(text) = event.insertable_text() {
                    input.chars.push_str(&text);
                }
            }
            input.ime_preedit.clone_from(&resolved.preedit);
            input.ime_cursor = resolved.cursor;
            input.ime_commit.clone_from(&resolved.commit);
            input.ime_enabled = resolved.enabled;
            input.shift = self.shift;
            input.ctrl = self.ctrl;
            input.alt = self.alt;
            input.super_key = self.super_key;
        }
        for gesture in gestures {
            send(self.app.world_mut(), gesture);
        }
        for file in std::mem::take(&mut self.queued_files) {
            match file_drop::deliver(&mut self.hovered_files, file) {
                file_drop::DeliveredFile::Hover(event) => send(self.app.world_mut(), event),
                file_drop::DeliveredFile::Drop(event) => send(self.app.world_mut(), event),
                file_drop::DeliveredFile::Cancel(event) => send(self.app.world_mut(), event),
            }
        }
        self.app
            .world_mut()
            .insert_resource(self.hovered_files.clone());
        self.pointer.pressed = false;
        self.pointer.released = false;
        self.pointer.press_at = None;
        self.press_px = None;
        self.scroll = [0.0, 0.0];
        self.keys.clear();
        self.ime_events.clear();
    }

    fn note_pen(&mut self, pen: &pen::PenFeed) {
        if !self.accepts_pen {
            return;
        }
        let Some(reading) = pen.since(self.pen_seen) else {
            return;
        };
        self.pen_seen = reading.generation;
        let was_down = self
            .pointers
            .get(PEN_ID)
            .is_some_and(|contact| contact.down);
        let phase = match (was_down, reading.down) {
            (false, true) => ContactPhase::Start,
            (true, false) => ContactPhase::End,
            _ => ContactPhase::Move,
        };
        let mut buttons = PointerButtons::new();
        if reading.down {
            buttons.set(PointerButton::Left, true);
        }
        self.pointers.apply(ContactUpdate {
            id: PEN_ID,
            kind: reading.kind,
            position: self.pointer.position,
            pressure: Some(reading.pressure),
            tilt: reading.tilt,
            twist: reading.twist,
            buttons,
            button: PointerButton::Left,
            phase,
            time: self.started.elapsed(),
        });
    }

    /// Write the window into [`Viewport`], step the world, paint, and present.
    /// Returns whether the surface presented, so input edges survive a lost frame.
    /// Hiding does not drop the surface created on resume.
    fn redraw_frame(&mut self, event_loop: &dyn ActiveEventLoop) -> bool {
        let Some((raw_width, raw_height, scale)) = self.gpu.as_ref().map(|gpu| {
            let size = gpu.shell().inner_size();
            (
                size.width,
                size.height,
                positive_scale(gpu.shell().scale_factor()),
            )
        }) else {
            return false;
        };
        deliver(self.app.world_mut(), self.desktop_id);
        self.sync_input();
        self.poll_dialog();
        // Publish before `frame` so this update's layout sees the new scale and
        // the resize events that flip at the start of `App::update`.
        rndr_core::window::publish_metrics(self.app.world_mut(), raw_width, raw_height, scale);
        let width = raw_width.max(1);
        let height = raw_height.max(1);
        {
            let board = &mut self.os_clipboard;
            if let Some(clipboard) = self.app.world_mut().resource_mut::<Clipboard>() {
                board.pull(clipboard);
            }
        }
        #[cfg(not(feature = "observe"))]
        let painted = frame(&mut self.app);
        #[cfg(feature = "observe")]
        let (painted, observation) = rndr_paint::frame_with_snapshot(&mut self.app, |world| {
            let scale = world
                .resource::<Viewport>()
                .map(|v| v.pixels_per_point())
                .unwrap_or(1.0);
            observe::begin(world, raw_width, raw_height, scale)
        });
        {
            let board = &mut self.os_clipboard;
            if let Some(clipboard) = self.app.world_mut().resource_mut::<Clipboard>() {
                board.push(clipboard);
                clipboard.dirty = false;
                clipboard.primary_dirty = false;
                clipboard.image_dirty = false;
            }
        }
        // A requested capture binds paint-time engine resources. Reject a
        // Render-stage invalidation before atlas upload or surface submission.
        #[cfg(feature = "observe")]
        let observation = if let Some(capture) = observation {
            let scale = self
                .app
                .world()
                .resource::<Viewport>()
                .map(|v| v.pixels_per_point())
                .unwrap_or(1.0);
            let Some(capture) = observe::validate(self.app.world(), capture, scale) else {
                return false;
            };
            Some(capture)
        } else {
            None
        };
        self.upload_atlases();
        crate::images::ingest(self.app.world_mut());
        let images = crate::images::pending(self.app.world());
        let budget = crate::images::budget_of(self.app.world());
        let scale = self
            .app
            .world()
            .resource::<Viewport>()
            .map(|viewport| viewport.pixels_per_point())
            .unwrap_or(1.0);
        #[cfg(feature = "observe")]
        {
            let gpu = self.gpu.as_mut().expect("surface");
            gpu.observe_armed = observation.is_some();
            gpu.observed = None;
        }
        let (presented, uploaded) = {
            let gpu = self.gpu.as_mut().expect("surface");
            let world = self.app.world();
            gpu.present_frame(&painted, &images, budget, width, height, scale, Some(world))
        };
        #[cfg(feature = "observe")]
        if let Some(capture) = observation {
            let gpu = self.gpu.as_mut().expect("surface");
            let text_engine_kind = capture.text_engine_kind();
            observe::complete(
                capture,
                gpu.observed.take().unwrap_or(Err("surface_unavailable")),
                presented,
                gpu.observation_provenance(!painted.draws.is_empty(), text_engine_kind),
            );
            gpu.observe_armed = false;
        }
        if !uploaded.is_empty() {
            crate::images::release_cpu(self.app.world_mut(), &uploaded);
        }
        self.apply_cursor(event_loop);
        self.reveal_window();
        self.place_ime_cursor();
        self.allow_ime();
        self.publish_access();
        if presented && self.trace.first_present.is_none() {
            self.trace.note_present();
            self.publish_trace();
        }
        if presented {
            let id = self
                .app
                .world()
                .resource::<Pointers>()
                .map(|pointers| pointers.primary_id())
                .unwrap_or(MOUSE_ID);
            self.pointers.set_primary(id);
            self.pointers.end_frame();
        }
        presented
    }

    fn poll_dialog(&mut self) {
        #[cfg(feature = "dialog")]
        {
            let Some(dialog) = rndr_core::take_file_dialog(self.app.world_mut()) else {
                return;
            };
            let path = dialogs::show(self.gpu.as_ref().and_then(Gpu::winit_window), &dialog);
            send(self.app.world_mut(), rndr_core::FileChosen { path });
        }
    }

    fn allow_ime(&self) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let allow = self
            .app
            .world()
            .resource::<Focus>()
            .and_then(|focus| focus.entity)
            .is_some_and(|entity| self.app.world().has::<rndr_core::CapturesKeys>(entity));
        gpu.shell().set_ime_allowed(allow);
    }

    fn publish_access(&mut self) {
        #[cfg(feature = "accesskit")]
        if self.access.is_some() {
            if let Some(access) = self.access.as_mut() {
                access.update_if_active(|| self.access_bridge.update(self.app.world()));
            }
        }
    }

    #[cfg(feature = "accesskit")]
    fn process_access(&mut self, event: &WindowEvent) {
        let window = self.gpu.as_ref().and_then(|gpu| {
            gpu.winit_window()
                .map(|window| window as *const dyn winit::window::Window)
        });
        let Some(access) = self.access.as_mut() else {
            return;
        };
        let Some(window) = window else {
            return;
        };
        // SAFETY: `window` points at the winit window stored in `gpu`. `gpu` is
        // not mutably borrowed, and `process_event` only reads the window.
        access.process_event(unsafe { &*window }, event);
    }

    #[cfg(feature = "accesskit")]
    fn on_access(&mut self, event: &access_winit::Event) {
        use access_winit::WindowEvent as AccessEvent;
        match &event.window_event {
            AccessEvent::InitialTreeRequested => self.publish_access(),
            AccessEvent::ActionRequested(request) => {
                if let Some(command) = self.access_bridge.command(self.app.world(), request) {
                    rndr_core::apply_access(self.app.world_mut(), command);
                }
                self.request_redraw();
            }
            AccessEvent::AccessibilityDeactivated => {}
        }
    }

    fn apply_cursor(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let style = self
            .app
            .world()
            .resource::<CursorStyle>()
            .copied()
            .unwrap_or_default();
        match style {
            CursorStyle::Hand => gpu.shell().set_cursor_icon(CursorIcon::Pointer),
            CursorStyle::Arrow => gpu.shell().set_cursor_icon(CursorIcon::Default),
            CursorStyle::Image => self.apply_image_cursor(event_loop),
        }
    }

    fn apply_image_cursor(&mut self, event_loop: &dyn ActiveEventLoop) {
        let Some(image) = self.app.world().resource::<CursorImage>().cloned() else {
            if let Some(gpu) = self.gpu.as_ref() {
                gpu.shell().set_cursor_icon(CursorIcon::Default);
            }
            return;
        };
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        image.width.hash(&mut hasher);
        image.height.hash(&mut hasher);
        image.hotspot_x.hash(&mut hasher);
        image.hotspot_y.hash(&mut hasher);
        image.rgba.hash(&mut hasher);
        let hash = hasher.finish();
        if self.cursor_cache.as_ref().map(|cached| cached.0) != Some(hash) {
            if let Ok(source) = CustomCursorSource::from_rgba(
                image.rgba.clone(),
                image.width,
                image.height,
                image.hotspot_x,
                image.hotspot_y,
            ) {
                let Ok(cursor) = event_loop.create_custom_cursor(source) else {
                    return;
                };
                self.cursor_cache = Some((hash, cursor));
            }
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        if let Some((_, cursor)) = &self.cursor_cache {
            gpu.shell().set_custom_cursor(cursor.clone());
        } else {
            gpu.shell().set_cursor_icon(CursorIcon::Default);
        }
    }

    fn reveal_window(&mut self) {
        if !self.show_after_present {
            return;
        }
        self.show_after_present = false;
        if let Some(gpu) = self.gpu.as_mut() {
            let panel = self.config.kind == WindowKind::Panel;
            gpu.shell_mut().apply_effect(panel, WindowEffect::Show);
        }
    }

    fn upload_atlases(&mut self) {
        #[cfg(feature = "parley")]
        {
            let uploads = self
                .app
                .world()
                .resource::<Arc<dyn TextEngine>>()
                .and_then(|engine| engine.as_any().downcast_ref::<ParleyText>())
                .map(|parley| parley.take_uploads());
            let Some(uploads) = uploads else {
                return;
            };
            let Some(gpu) = self.gpu.as_mut() else {
                return;
            };
            gpu.upload_atlas(AtlasKind::Coverage, &layer_upload(uploads.coverage));
            gpu.upload_atlas(AtlasKind::Color, &layer_upload(uploads.color));
        }
        #[cfg(not(feature = "parley"))]
        {
            let _ = self;
        }
    }

    fn place_ime_cursor(&self) {
        let Some(gpu) = self.gpu.as_ref() else {
            return;
        };
        let Some((x, y, w, h)) = ime_cursor_area(self.app.world()) else {
            return;
        };
        gpu.shell().set_ime_cursor_area(
            winit::dpi::LogicalPosition::new(x, y),
            winit::dpi::LogicalSize::new(w, h),
        );
    }
}

#[cfg(feature = "parley")]
fn layer_upload(layer: crate::text::AtlasLayer) -> AtlasUpload {
    AtlasUpload {
        width: layer.width,
        height: layer.height,
        bpp: layer.bpp,
        full: layer.full,
        pixels: layer.pixels,
        regions: layer
            .rects
            .into_iter()
            .map(|rect| AtlasRegion {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                bytes: rect.bytes,
            })
            .collect(),
    }
}

fn ime_cursor_area(world: &rndr_core::World) -> Option<(f64, f64, f64, f64)> {
    let entity = world.resource::<Focus>()?.entity?;
    let rect = *world.get::<Rect>(entity)?;
    let caret = *world.get::<Caret>(entity)?;
    let layout = world.get::<TextLayout>(entity);
    let index = layout
        .and_then(|layout| layout.ime_cursor)
        .unwrap_or(caret.index);
    let (x, y, h) = if let Some(layout) = layout {
        let (x, y) = layout.caret_xy(index);
        let h = layout
            .lines
            .iter()
            .find(|line| index >= line.start && index <= line.end)
            .map(|line| line.h)
            .unwrap_or(16.0);
        (x, y, h)
    } else {
        (0.0, 0.0, 16.0)
    };
    Some((
        f64::from(rect.x + TEXT_ORIGIN_X + x),
        f64::from(rect.y + TEXT_ORIGIN_Y + y),
        2.0,
        f64::from(h),
    ))
}

fn host_platform() -> Platform {
    #[cfg(target_os = "macos")]
    {
        Platform::Mac
    }
    #[cfg(target_os = "windows")]
    {
        Platform::Windows
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            Platform::Wayland
        } else {
            Platform::X11
        }
    }
}

fn modifiers_of(shift: bool, ctrl: bool, alt: bool, super_key: bool) -> Modifiers {
    Modifiers {
        shift,
        ctrl,
        alt,
        super_key,
    }
}

fn map_key_event(
    event: &winit::event::KeyEvent,
    modifiers: Modifiers,
    released: bool,
    synthetic: bool,
    order: u64,
) -> Option<KeyEvent> {
    let logical = map_logical(&event.key_without_modifiers);
    let code = map_physical(&event.physical_key);
    let text = if released || synthetic {
        None
    } else {
        produced_text(event.text.as_deref(), modifiers)
    };
    if matches!(logical, LogicalKey::Unidentified) && code.is_none() && text.is_none() {
        return None;
    }
    Some(KeyEvent {
        logical,
        text,
        code,
        modifiers,
        repeat: event.repeat,
        released,
        synthetic,
        order,
    })
}

fn produced_text(text: Option<&str>, modifiers: Modifiers) -> Option<String> {
    // Super never inserts. Ctrl and Alt keep reported text so AltGr still types.
    if modifiers.super_key {
        return None;
    }
    let text = text?;
    let filtered: String = text.chars().filter(|ch| !ch.is_control()).collect();
    if filtered.is_empty() {
        None
    } else {
        Some(filtered)
    }
}

fn map_logical(key: &Key) -> LogicalKey {
    match key {
        Key::Named(named) => map_named(named)
            .map(LogicalKey::Named)
            .unwrap_or(LogicalKey::Unidentified),
        Key::Character(text) if text == " " => LogicalKey::Named(CoreNamed::Space),
        Key::Character(text) => text
            .chars()
            .next()
            .map(LogicalKey::Character)
            .unwrap_or(LogicalKey::Unidentified),
        Key::Dead(_) | Key::Unidentified(_) => LogicalKey::Unidentified,
    }
}

fn map_named(key: &NamedKey) -> Option<CoreNamed> {
    Some(match key {
        NamedKey::ArrowUp => CoreNamed::ArrowUp,
        NamedKey::ArrowDown => CoreNamed::ArrowDown,
        NamedKey::ArrowLeft => CoreNamed::ArrowLeft,
        NamedKey::ArrowRight => CoreNamed::ArrowRight,
        NamedKey::Enter => CoreNamed::Enter,
        NamedKey::Escape => CoreNamed::Escape,
        NamedKey::Tab => CoreNamed::Tab,
        NamedKey::Backspace => CoreNamed::Backspace,
        NamedKey::Delete => CoreNamed::Delete,
        NamedKey::Insert => CoreNamed::Insert,
        NamedKey::Home => CoreNamed::Home,
        NamedKey::End => CoreNamed::End,
        NamedKey::PageUp => CoreNamed::PageUp,
        NamedKey::PageDown => CoreNamed::PageDown,
        NamedKey::F1 => CoreNamed::F(1),
        NamedKey::F2 => CoreNamed::F(2),
        NamedKey::F3 => CoreNamed::F(3),
        NamedKey::F4 => CoreNamed::F(4),
        NamedKey::F5 => CoreNamed::F(5),
        NamedKey::F6 => CoreNamed::F(6),
        NamedKey::F7 => CoreNamed::F(7),
        NamedKey::F8 => CoreNamed::F(8),
        NamedKey::F9 => CoreNamed::F(9),
        NamedKey::F10 => CoreNamed::F(10),
        NamedKey::F11 => CoreNamed::F(11),
        NamedKey::F12 => CoreNamed::F(12),
        NamedKey::F13 => CoreNamed::F(13),
        NamedKey::F14 => CoreNamed::F(14),
        NamedKey::F15 => CoreNamed::F(15),
        NamedKey::F16 => CoreNamed::F(16),
        NamedKey::F17 => CoreNamed::F(17),
        NamedKey::F18 => CoreNamed::F(18),
        NamedKey::F19 => CoreNamed::F(19),
        NamedKey::F20 => CoreNamed::F(20),
        NamedKey::F21 => CoreNamed::F(21),
        NamedKey::F22 => CoreNamed::F(22),
        NamedKey::F23 => CoreNamed::F(23),
        NamedKey::F24 => CoreNamed::F(24),
        _ => return None,
    })
}

fn map_physical(key: &PhysicalKey) -> Option<Physical> {
    let PhysicalKey::Code(code) = key else {
        return None;
    };
    Some(match code {
        KeyCode::KeyA => Physical::Letter('a'),
        KeyCode::KeyB => Physical::Letter('b'),
        KeyCode::KeyC => Physical::Letter('c'),
        KeyCode::KeyD => Physical::Letter('d'),
        KeyCode::KeyE => Physical::Letter('e'),
        KeyCode::KeyF => Physical::Letter('f'),
        KeyCode::KeyG => Physical::Letter('g'),
        KeyCode::KeyH => Physical::Letter('h'),
        KeyCode::KeyI => Physical::Letter('i'),
        KeyCode::KeyJ => Physical::Letter('j'),
        KeyCode::KeyK => Physical::Letter('k'),
        KeyCode::KeyL => Physical::Letter('l'),
        KeyCode::KeyM => Physical::Letter('m'),
        KeyCode::KeyN => Physical::Letter('n'),
        KeyCode::KeyO => Physical::Letter('o'),
        KeyCode::KeyP => Physical::Letter('p'),
        KeyCode::KeyQ => Physical::Letter('q'),
        KeyCode::KeyR => Physical::Letter('r'),
        KeyCode::KeyS => Physical::Letter('s'),
        KeyCode::KeyT => Physical::Letter('t'),
        KeyCode::KeyU => Physical::Letter('u'),
        KeyCode::KeyV => Physical::Letter('v'),
        KeyCode::KeyW => Physical::Letter('w'),
        KeyCode::KeyX => Physical::Letter('x'),
        KeyCode::KeyY => Physical::Letter('y'),
        KeyCode::KeyZ => Physical::Letter('z'),
        KeyCode::Digit0 => Physical::Digit(0),
        KeyCode::Digit1 => Physical::Digit(1),
        KeyCode::Digit2 => Physical::Digit(2),
        KeyCode::Digit3 => Physical::Digit(3),
        KeyCode::Digit4 => Physical::Digit(4),
        KeyCode::Digit5 => Physical::Digit(5),
        KeyCode::Digit6 => Physical::Digit(6),
        KeyCode::Digit7 => Physical::Digit(7),
        KeyCode::Digit8 => Physical::Digit(8),
        KeyCode::Digit9 => Physical::Digit(9),
        KeyCode::F1 => Physical::Function(1),
        KeyCode::F2 => Physical::Function(2),
        KeyCode::F3 => Physical::Function(3),
        KeyCode::F4 => Physical::Function(4),
        KeyCode::F5 => Physical::Function(5),
        KeyCode::F6 => Physical::Function(6),
        KeyCode::F7 => Physical::Function(7),
        KeyCode::F8 => Physical::Function(8),
        KeyCode::F9 => Physical::Function(9),
        KeyCode::F10 => Physical::Function(10),
        KeyCode::F11 => Physical::Function(11),
        KeyCode::F12 => Physical::Function(12),
        KeyCode::F13 => Physical::Function(13),
        KeyCode::F14 => Physical::Function(14),
        KeyCode::F15 => Physical::Function(15),
        KeyCode::F16 => Physical::Function(16),
        KeyCode::F17 => Physical::Function(17),
        KeyCode::F18 => Physical::Function(18),
        KeyCode::F19 => Physical::Function(19),
        KeyCode::F20 => Physical::Function(20),
        KeyCode::F21 => Physical::Function(21),
        KeyCode::F22 => Physical::Function(22),
        KeyCode::F23 => Physical::Function(23),
        KeyCode::F24 => Physical::Function(24),
        KeyCode::ArrowUp => Physical::ArrowUp,
        KeyCode::ArrowDown => Physical::ArrowDown,
        KeyCode::ArrowLeft => Physical::ArrowLeft,
        KeyCode::ArrowRight => Physical::ArrowRight,
        KeyCode::Enter => Physical::Enter,
        KeyCode::Escape => Physical::Escape,
        KeyCode::Tab => Physical::Tab,
        KeyCode::Backspace => Physical::Backspace,
        KeyCode::Delete => Physical::Delete,
        KeyCode::Insert => Physical::Insert,
        KeyCode::Home => Physical::Home,
        KeyCode::End => Physical::End,
        KeyCode::PageUp => Physical::PageUp,
        KeyCode::PageDown => Physical::PageDown,
        KeyCode::Space => Physical::Space,
        KeyCode::Minus => Physical::Minus,
        KeyCode::Equal => Physical::Equal,
        KeyCode::BracketLeft => Physical::BracketLeft,
        KeyCode::BracketRight => Physical::BracketRight,
        KeyCode::Backslash => Physical::Backslash,
        KeyCode::Semicolon => Physical::Semicolon,
        KeyCode::Quote => Physical::Quote,
        KeyCode::Comma => Physical::Comma,
        KeyCode::Period => Physical::Period,
        KeyCode::Slash => Physical::Slash,
        KeyCode::Backquote => Physical::Backquote,
        _ => return None,
    })
}

impl Handler {
    fn request_redraw(&self) {
        if let Some(gpu) = &self.gpu {
            gpu.shell().request_redraw();
        } else if let Some(shell) = &self.pending_shell {
            shell.request_redraw();
        }
    }

    fn sync_flow(&self, event_loop: &dyn ActiveEventLoop) {
        let hidden = !rndr_core::window::is_visible(self.app.world());
        // A hidden window does not deliver redraws. Poll only while a tray
        // report or a task is still queued after the hidden pump; idle goes
        // back to Wait.
        if hidden && (self.app.wants_frame() || rndr_core::tray::pending(self.app.world())) {
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        if self.app.wants_frame() {
            self.request_redraw();
        }
        let now = Instant::now();
        event_loop.set_control_flow(idle_control_flow(self.app.next_deadline(), now));
    }

    /// Step the world without presenting. Tray clicks and timers still run,
    /// and a quit item exits, while the window is hidden.
    fn run_hidden_frame(&mut self, _event_loop: &dyn ActiveEventLoop) {
        deliver(self.app.world_mut(), self.desktop_id);
        for _ in 0..8 {
            self.app.update();
            self.apply_effects();
            self.apply_tray();
            if !self.app.wants_frame() && !rndr_core::tray::pending(self.app.world()) {
                break;
            }
        }
    }

    fn apply_tray(&mut self) {
        let commands = rndr_core::tray::drain_commands(self.app.world_mut());
        #[cfg(feature = "tray")]
        if !commands.is_empty() {
            if let Some(config) = self
                .app
                .world()
                .resource::<rndr_core::TrayConfig>()
                .cloned()
            {
                self.tray.apply(&config, &commands);
            }
        }
        #[cfg(not(feature = "tray"))]
        let _ = commands;
    }

    /// Apply show / hide / focus to the existing window. The surface stays.
    ///
    /// This does not create a device. If device init is still running, the
    /// effects wait and are applied when that init finishes.
    fn apply_effects(&mut self) {
        self.install_gpu_if_ready();
        let effects = rndr_core::window::drain_effects(self.app.world_mut());
        if effects.is_empty() {
            return;
        }
        if self.gpu.is_none() {
            self.stashed_effects.extend(effects);
            return;
        }
        self.apply_effect_list(effects);
    }

    fn apply_effect_list(&mut self, effects: Vec<WindowEffect>) {
        let panel = self.config.kind == WindowKind::Panel;
        let mut redraw = false;
        if let Some(gpu) = self.gpu.as_mut() {
            for effect in effects {
                redraw |= matches!(effect, WindowEffect::Show | WindowEffect::Focus);
                gpu.shell_mut().apply_effect(panel, effect);
            }
        }
        if redraw {
            self.request_redraw();
        }
    }

    fn has_layer(&self) -> bool {
        self.gpu.as_ref().is_some_and(|gpu| gpu.shell().is_layer())
            || self
                .pending_shell
                .as_ref()
                .is_some_and(|shell| shell.is_layer())
    }

    fn device_pending(&self) -> bool {
        self.pending_rx.is_some()
    }

    fn publish_trace(&mut self) {
        self.app.world_mut().insert_resource(self.trace.clone());
    }

    /// Install a device that the worker has finished. Does not block.
    fn install_gpu_if_ready(&mut self) {
        let Some(rx) = self.pending_rx.as_ref() else {
            return;
        };
        let Ok(parts) = rx.try_recv() else {
            return;
        };
        self.pending_rx = None;
        let shell = self
            .pending_shell
            .take()
            .expect("shell kept for background device init");
        self.gpu = Some(Gpu::from_parts(shell, parts));
        self.trace.device_ready = Some(self.trace.started.elapsed());
        self.publish_trace();
        let effects = std::mem::take(&mut self.stashed_effects);
        if !effects.is_empty() {
            self.apply_effect_list(effects);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_device(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        shell: shell::Shell,
        atlas_w: u32,
        atlas_h: u32,
        atlas_pixels: Vec<u8>,
        color_w: u32,
        color_h: u32,
        color_pixels: Vec<u8>,
        observe: bool,
    ) {
        let (instance, surface) = shell.create_wgpu(event_loop.owned_display_handle());
        let size = shell.inner_size();
        let transparent = self.config.transparent;
        let (tx, rx) = mpsc::sync_channel(1);
        let bus = self.bus.clone();
        std::thread::Builder::new()
            .name("rndr-gpu".to_string())
            .spawn(move || {
                let parts = pollster::block_on(Gpu::build_parts(
                    instance,
                    surface,
                    size,
                    gpu::AtlasBytes {
                        width: atlas_w,
                        height: atlas_h,
                        pixels: &atlas_pixels,
                    },
                    gpu::AtlasBytes {
                        width: color_w,
                        height: color_h,
                        pixels: &color_pixels,
                    },
                    transparent,
                    #[cfg(feature = "observe")]
                    observe,
                ));
                let _ = tx.send(parts);
                bus.wake();
            })
            .expect("gpu thread");
        #[cfg(not(feature = "observe"))]
        let _ = observe;
        self.pending_rx = Some(rx);
        self.pending_shell = Some(shell);
    }

    /// Open a layer-shell panel on Linux Wayland, or a winit window otherwise.
    /// A missing protocol or display falls back to winit without an error.
    fn open_shell(&mut self, event_loop: &dyn ActiveEventLoop, show: bool) -> shell::Shell {
        #[cfg(target_os = "linux")]
        if self.config.kind == WindowKind::Panel && platform::session_is_wayland(event_loop) {
            let bus = self.bus.clone();
            let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
                bus.wake();
            });
            if let Ok(layer) = wayland_layer::LayerWindow::open(&self.config, show, false) {
                if show {
                    self.show_after_present = true;
                }
                return shell::Shell::layer(layer, wake);
            }
        }
        let mut attrs = platform::attributes(&self.config);
        attrs = attrs.with_visible(false);
        if show {
            // Keep the window unmapped until the first present so the first
            // frame the user sees is painted. A layer-shell panel does not
            // use this winit window.
            self.show_after_present = true;
        }
        let window: Arc<dyn winit::window::Window> =
            Arc::from(event_loop.create_window(attrs).expect("window"));
        self.winit_id = Some(window.id());
        platform::finish_create(window.as_ref(), event_loop, &self.config);
        shell::Shell::winit(window)
    }

    fn on_redraw(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.redraw_frame(event_loop) {
            if let Some(input) = self.app.world_mut().resource_mut::<Input>() {
                input.pointer.pressed = false;
                input.scroll = [0.0, 0.0];
                input.key = None;
                input.keys.clear();
                input.chars.clear();
                input.ime_commit.clear();
            }
        }
        self.apply_effects();
        self.apply_tray();
    }

    fn pump_shell(&mut self, event_loop: &dyn ActiveEventLoop) -> bool {
        if !self.has_layer() {
            return false;
        }
        let events = self
            .gpu
            .as_mut()
            .map(|gpu| gpu.shell_mut().pump())
            .unwrap_or_default();
        let mut redraw = false;
        for event in events {
            redraw |= self.apply_shell_event(event);
        }
        if redraw && rndr_core::window::is_visible(self.app.world()) {
            self.on_redraw(event_loop);
            true
        } else {
            false
        }
    }

    fn apply_shell_event(&mut self, event: shell::ShellEvent) -> bool {
        use shell::ShellEvent;
        match event {
            ShellEvent::Close => {
                self.close.requested = true;
                false
            }
            ShellEvent::Focused(focused) => {
                if focused {
                    rndr_core::window::platform_focused(self.app.world());
                } else {
                    cancel_pointer_input(&mut self.pointer, &mut self.pointers);
                    self.press_px = None;
                    self.accepts_pen = false;
                    self.gestures = GestureState::default();
                    self.queued_gestures.clear();
                    rndr_core::window::platform_blurred(self.app.world());
                }
                true
            }
            ShellEvent::Resized | ShellEvent::Scale(_) => {
                if let Some(gpu) = self.gpu.as_mut() {
                    let size = gpu.shell().inner_size();
                    gpu.resize(size);
                }
                true
            }
            ShellEvent::Cursor { x, y } => {
                self.cursor_px = [x as f32, y as f32];
                self.accepts_pen = true;
                self.apply_mouse(ContactPhase::Move);
                let watch = self
                    .app
                    .world()
                    .resource::<MotionWatch>()
                    .is_some_and(|watch| watch.active);
                self.pointer.down || watch
            }
            ShellEvent::Button { pressed, button } => {
                self.pointer.id = MOUSE_ID;
                self.pointer.button = match button {
                    0 => PointerButton::Left,
                    1 => PointerButton::Right,
                    2 => PointerButton::Middle,
                    other => PointerButton::Other(u16::try_from(other).unwrap_or(u16::MAX)),
                };
                self.accepts_pen = true;
                if pressed {
                    self.press_px = Some(self.cursor_px);
                    self.pointer.pressed = true;
                    self.pointer.down = true;
                    self.apply_mouse(ContactPhase::Start);
                } else {
                    self.pointer.down = false;
                    self.pointer.released = true;
                    self.apply_mouse(ContactPhase::End);
                }
                true
            }
            ShellEvent::Scroll { x, y } => {
                self.scroll[0] += x as f32;
                self.scroll[1] += y as f32;
                true
            }
            ShellEvent::Key {
                logical,
                text,
                code,
                mods,
                released,
            } => {
                if self.key_order < u64::MAX - 1 {
                    self.key_order += 1;
                }
                let order = self.key_order;
                rndr_core::hotkey::note_key_order(self.app.world_mut(), order);
                self.shift = mods.shift;
                self.ctrl = mods.ctrl;
                self.alt = mods.alt;
                self.super_key = mods.super_key;
                self.ime_events.push(ImeEvent::Key(KeyEvent {
                    logical,
                    text: if released { None } else { text },
                    code,
                    modifiers: mods,
                    repeat: false,
                    released,
                    synthetic: false,
                    order,
                }));
                true
            }
            ShellEvent::Redraw => true,
        }
    }
}

impl Handler {
    fn note_pointer(&mut self, position: winit::dpi::PhysicalPosition<f64>, source: PointerSource) {
        match source {
            PointerSource::Mouse | PointerSource::Unknown => {
                self.cursor_px = [position.x as f32, position.y as f32];
                self.accepts_pen = true;
                self.apply_mouse(ContactPhase::Move);
                let watch = self
                    .app
                    .world()
                    .resource::<MotionWatch>()
                    .is_some_and(|watch| watch.active);
                if self.pointer.down || watch {
                    self.request_redraw();
                }
            }
            PointerSource::Touch { finger_id, force } => {
                self.apply_touch(
                    finger_id.into_raw() as u64,
                    position,
                    ContactPhase::Move,
                    force.as_ref(),
                );
                self.request_redraw();
            }
            PointerSource::TabletTool { kind, data, .. } => {
                self.accepts_pen = true;
                let held = tablet::tip_down(&data);
                let update = tablet::tablet_contact(
                    kind,
                    &data,
                    ContactPhase::Move,
                    PointerButton::Left,
                    held,
                    position,
                    f64::from(self.scale()),
                    self.started.elapsed(),
                );
                self.cursor_px = [position.x as f32, position.y as f32];
                self.pointers.apply(update);
                self.request_redraw();
            }
            _ => {}
        }
    }

    fn apply_touch(
        &mut self,
        id: u64,
        position: winit::dpi::PhysicalPosition<f64>,
        phase: ContactPhase,
        force: Option<&winit::event::Force>,
    ) {
        self.accepts_pen = false;
        let scale = self.scale();
        let position = [position.x as f32 / scale, position.y as f32 / scale];
        let mut buttons = PointerButtons::new();
        if !matches!(phase, ContactPhase::End | ContactPhase::Cancel) {
            buttons.set(PointerButton::Left, true);
        }
        let pressure = force.and_then(|force| {
            let normalized = force.normalized(None);
            normalized.is_finite().then_some(normalized as f32)
        });
        self.pointers.apply(ContactUpdate {
            id: touch_id(id),
            kind: PointerKind::Touch,
            position,
            pressure,
            tilt: None,
            twist: None,
            buttons,
            button: PointerButton::Left,
            phase,
            time: self.started.elapsed(),
        });
    }

    fn drag_window_is_wayland(&self) -> bool {
        let Some(window) = self.gpu.as_ref().and_then(|gpu| gpu.shell().winit_window()) else {
            return self.platform == rndr_core::Platform::Wayland;
        };
        matches!(
            winit::raw_window_handle::HasWindowHandle::window_handle(window)
                .ok()
                .map(|handle| handle.as_raw()),
            Some(winit::raw_window_handle::RawWindowHandle::Wayland(_))
        )
    }

    fn drag_sample(
        &self,
        position: Option<winit::dpi::PhysicalPosition<f64>>,
    ) -> file_drop::DragPointSample {
        let Some(position) = position else {
            return file_drop::DragPointSample::Absent;
        };
        match file_drop::logical_point(
            file_drop::DragPoint::ClientPhysical([position.x as f32, position.y as f32]),
            f64::from(self.scale()),
        ) {
            Some(point) => file_drop::DragPointSample::Valid(point),
            None => file_drop::DragPointSample::Invalid,
        }
    }

    fn on_drag(&mut self, event_loop: &dyn ActiveEventLoop, notice: file_drop::DragNotice) {
        if self.drag_window_is_wayland() {
            return;
        }
        let notice = match notice {
            file_drop::DragNotice::Entered { id, point, .. } => {
                let transfer = winit::data_transfer::DataTransferId::from_raw(id);
                let _ = event_loop
                    .set_valid_dnd_actions(transfer, &[winit::event_loop::DndAction::Copy]);
                let serial = event_loop
                    .fetch_data_transfer(transfer, &winit::data_transfer::TypeHint::UriList)
                    .ok();
                let request = self.next_fetch;
                self.next_fetch = self.next_fetch.wrapping_add(1);
                self.outstanding_fetch = serial.map(|serial| OutstandingFetch {
                    id,
                    serial,
                    request,
                });
                file_drop::DragNotice::Entered {
                    id,
                    point,
                    request: serial.map(|_| request),
                }
            }
            file_drop::DragNotice::Left { id } => {
                if self
                    .outstanding_fetch
                    .as_ref()
                    .is_some_and(|fetch| fetch.id == id)
                {
                    self.outstanding_fetch = None;
                }
                file_drop::DragNotice::Left { id }
            }
            other => other,
        };
        let produced = self.drag.apply(notice);
        if !produced.is_empty() {
            self.queued_files.extend(produced);
            self.request_redraw();
        }
    }
}

struct OutstandingFetch {
    id: i64,
    serial: winit::event_loop::AsyncRequestSerial,
    request: u64,
}

/// The fetch token when this response is the one still outstanding.
fn accepted_fetch(
    fetch: &OutstandingFetch,
    id: i64,
    serial: winit::event_loop::AsyncRequestSerial,
) -> Option<u64> {
    (fetch.id == id && fetch.serial == serial).then_some(fetch.request)
}

impl ApplicationHandler for Handler {
    fn new_events(&mut self, _event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.request_redraw();
        }
    }

    fn can_create_surfaces(&mut self, _event_loop: &dyn ActiveEventLoop) {}

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.install_gpu_if_ready();
        self.pump_shell(event_loop);
        let hidden = !rndr_core::window::is_visible(self.app.world());
        if hidden && (self.app.wants_frame() || rndr_core::tray::pending(self.app.world())) {
            self.run_hidden_frame(event_loop);
        }
        self.sync_flow(event_loop);
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if self.gpu.is_some() || self.pending_rx.is_some() {
            // A later resume must keep the surface. Show/hide never drops `Gpu`,
            // and a hidden start does not start a second device.
            return;
        }
        self.trace = StartupTrace::start();
        let show = rndr_core::window::is_visible(self.app.world());
        let shell = self.open_shell(event_loop, show);
        #[cfg(feature = "accesskit")]
        if let Some(window) = shell.winit_arc() {
            let bus = self.bus.clone();
            self.access = Some(access_winit::Adapter::new(
                event_loop,
                window.as_ref(),
                move |event| bus.access(event),
            ));
        }
        ensure_text_engine(self.app.world_mut());
        let engine = text_engine(self.app.world());
        let coverage = engine.coverage_atlas();
        // Emoji lives in the color atlas. The builtin engine leaves it empty;
        // the shader still binds a 1×1 texture so both engines share a pipeline.
        let color_atlas = engine.color_atlas();
        if let Some(atlas) = &color_atlas {
            debug_assert!(
                atlas.width == 0
                    || atlas.rgba.len() == (atlas.width as usize) * (atlas.height as usize) * 4
            );
        }
        let (atlas_w, atlas_h, atlas_pixels) = if let Some(atlas) = coverage {
            (atlas.width, atlas.height, atlas.r8.to_vec())
        } else {
            (ATLAS_W, ATLAS_H, atlas_r8())
        };
        let (color_w, color_h, color_pixels) = if let Some(atlas) = color_atlas {
            if atlas.width == 0 || atlas.height == 0 {
                (1, 1, vec![0, 0, 0, 0])
            } else {
                (atlas.width, atlas.height, atlas.rgba.to_vec())
            }
        } else {
            (1, 1, vec![0, 0, 0, 0])
        };
        self.trace.window_ready = Some(self.trace.started.elapsed());
        let observe = {
            #[cfg(feature = "observe")]
            {
                self.app
                    .world()
                    .resource::<observe::ObservationSession>()
                    .is_some()
            }
            #[cfg(not(feature = "observe"))]
            {
                false
            }
        };
        if device_init_plan(show) == DeviceInit::OffUiThread {
            self.trace.device_init_blocked_ui_thread = false;
            self.spawn_device(
                event_loop,
                shell,
                atlas_w,
                atlas_h,
                atlas_pixels,
                color_w,
                color_h,
                color_pixels,
                observe,
            );
            self.publish_trace();
            self.run_hidden_frame(event_loop);
            return;
        }
        self.trace.device_init_blocked_ui_thread = true;
        let gpu = pollster::block_on(Gpu::new(
            event_loop.owned_display_handle(),
            shell,
            gpu::AtlasBytes {
                width: atlas_w,
                height: atlas_h,
                pixels: &atlas_pixels,
            },
            gpu::AtlasBytes {
                width: color_w,
                height: color_h,
                pixels: &color_pixels,
            },
            self.config.transparent,
            #[cfg(feature = "observe")]
            observe,
        ));
        self.gpu = Some(gpu);
        self.trace.device_ready = Some(self.trace.started.elapsed());
        self.publish_trace();
        if rndr_core::window::is_visible(self.app.world()) {
            self.request_redraw();
        } else {
            // The loop is already running. Build the tray here so a tray-only
            // app does not wait for a redraw the hidden window will not send.
            self.run_hidden_frame(event_loop);
        }
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        #[cfg(feature = "accesskit")]
        self.process_access(&event);
        match event {
            WindowEvent::CloseRequested => {
                // Running coordinates this after native dispatch; never destroy
                // an app here, including unguarded primaries with guarded siblings.
                self.close.requested = true;
            }
            WindowEvent::Focused(focused) => {
                if focused {
                    rndr_core::window::platform_focused(self.app.world());
                } else {
                    cancel_pointer_input(&mut self.pointer, &mut self.pointers);
                    self.press_px = None;
                    self.accepts_pen = false;
                    self.gestures = GestureState::default();
                    self.queued_gestures.clear();
                    rndr_core::window::platform_blurred(self.app.world());
                }
                self.request_redraw();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                // Resized follows with the physical size. Publishing happens
                // on that redraw, from the window's live scale factor.
                self.request_redraw();
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.shift = state.shift_key();
                self.ctrl = state.control_key();
                self.alt = state.alt_key();
                self.super_key = state.meta_key();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.ime_events.push(ImeEvent::Preedit(text, cursor));
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_events.push(ImeEvent::Commit(text));
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Enabled) => {
                self.ime_events.push(ImeEvent::Enabled);
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_events.push(ImeEvent::Disabled);
                self.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                // X11 and Windows set `is_synthetic` for focus changes. macOS
                // and Wayland always pass false. The order is recorded before
                // the event is copied into Input, including while hidden.
                if self.key_order < u64::MAX - 1 {
                    self.key_order += 1;
                }
                let order = self.key_order;
                rndr_core::hotkey::note_key_order(self.app.world_mut(), order);
                let released = event.state == ElementState::Released;
                let modifiers = modifiers_of(self.shift, self.ctrl, self.alt, self.super_key);
                if let Some(key) = map_key_event(&event, modifiers, released, is_synthetic, order) {
                    self.ime_events.push(ImeEvent::Key(key));
                    self.request_redraw();
                }
            }
            WindowEvent::SurfaceResized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                    gpu.shell().request_redraw();
                }
            }
            WindowEvent::PointerMoved {
                position, source, ..
            } => {
                self.note_pointer(position, source);
            }
            WindowEvent::PointerButton {
                state,
                position,
                button,
                ..
            } => {
                self.cursor_px = [position.x as f32, position.y as f32];
                match button {
                    ButtonSource::Mouse(button) => {
                        self.pointer.id = MOUSE_ID;
                        self.pointer.button = map_pointer_button(button);
                        self.accepts_pen = true;
                        match state {
                            ElementState::Pressed => {
                                self.press_px = Some(self.cursor_px);
                                self.pointer.pressed = true;
                                self.pointer.down = true;
                                self.apply_mouse(ContactPhase::Start);
                            }
                            ElementState::Released => {
                                self.pointer.down = false;
                                self.pointer.released = true;
                                self.apply_mouse(ContactPhase::End);
                            }
                        }
                    }
                    ButtonSource::Touch { finger_id, force } => {
                        let phase = match state {
                            ElementState::Pressed => ContactPhase::Start,
                            ElementState::Released => ContactPhase::End,
                        };
                        self.apply_touch(
                            finger_id.into_raw() as u64,
                            position,
                            phase,
                            force.as_ref(),
                        );
                    }
                    ButtonSource::TabletTool { kind, button, data } => {
                        self.accepts_pen = true;
                        let (phase, held) =
                            tablet::tool_phase(button, state == ElementState::Pressed);
                        let update = tablet::tablet_contact(
                            kind,
                            &data,
                            phase,
                            tablet::tool_button(button),
                            held,
                            position,
                            f64::from(self.scale()),
                            self.started.elapsed(),
                        );
                        self.pointers.apply(update);
                    }
                    ButtonSource::Unknown(_) | _ => {}
                }
                self.request_redraw();
            }
            WindowEvent::PointerLeft {
                position,
                kind: winit::event::PointerKind::Touch(finger),
                ..
            } => {
                let point = position.unwrap_or(winit::dpi::PhysicalPosition::new(
                    f64::from(self.cursor_px[0]),
                    f64::from(self.cursor_px[1]),
                ));
                self.apply_touch(finger.into_raw() as u64, point, ContactPhase::Cancel, None);
                self.request_redraw();
            }
            WindowEvent::PinchGesture { delta, phase, .. } => {
                self.queued_gestures
                    .push(Gesture::pinch(gesture_phase(phase), delta));
                self.request_redraw();
            }
            WindowEvent::RotationGesture { delta, phase, .. } => {
                self.queued_gestures
                    .push(Gesture::rotate(gesture_phase(phase), delta));
                self.request_redraw();
            }
            WindowEvent::DragEntered { id, position } => {
                self.on_drag(
                    event_loop,
                    file_drop::DragNotice::Entered {
                        id: id.into_raw(),
                        point: self.drag_sample(position),
                        request: None,
                    },
                );
            }
            WindowEvent::DragPosition { id, position, .. } => {
                self.on_drag(
                    event_loop,
                    file_drop::DragNotice::Moved {
                        id: id.into_raw(),
                        point: self.drag_sample(Some(position)),
                    },
                );
            }
            WindowEvent::DataTransferReceived { id, serial, value } => {
                let raw = id.into_raw();
                let Some(request) = self
                    .outstanding_fetch
                    .as_ref()
                    .and_then(|fetch| accepted_fetch(fetch, raw, serial))
                else {
                    return;
                };
                self.outstanding_fetch = None;
                let paths = value
                    .try_as_uris()
                    .ok()
                    .into_iter()
                    .flatten()
                    .filter_map(|uri| file_drop::path_from_file_uri(&uri))
                    .collect();
                self.on_drag(
                    event_loop,
                    file_drop::DragNotice::Paths {
                        id: raw,
                        request,
                        paths,
                    },
                );
            }
            WindowEvent::DragDropped { id, .. } => {
                self.on_drag(
                    event_loop,
                    file_drop::DragNotice::Dropped { id: id.into_raw() },
                );
            }
            WindowEvent::DragLeft { id } => {
                self.on_drag(
                    event_loop,
                    file_drop::DragNotice::Left { id: id.into_raw() },
                );
            }
            WindowEvent::MouseWheel { delta, .. } => {
                match delta {
                    MouseScrollDelta::LineDelta(x, y) => {
                        self.scroll[0] += x * 32.0;
                        self.scroll[1] += y * 32.0;
                    }
                    MouseScrollDelta::PixelDelta(pos) => {
                        let scale = self.scale();
                        self.scroll[0] += pos.x as f32 / scale;
                        self.scroll[1] += pos.y as f32 / scale;
                    }
                    _ => {}
                }
                if let Some(gpu) = &self.gpu {
                    gpu.shell().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.on_redraw(event_loop),
            _ => {}
        }
    }
}

struct Running {
    slots: Vec<Handler>,
    hub: WindowHub,
    wake: Arc<dyn Wake>,
    events: std::sync::mpsc::Receiver<RunnerEvent>,
    bus: RunnerBus,
    pen: pen::PenFeed,
    executor: Arc<dyn rndr_core::Executor>,
    close: close::CloseDrain,
}

impl Running {
    fn reconcile_close(&mut self, event_loop: &dyn ActiveEventLoop) {
        // Updates (including hidden frames) and build closures may queue opens.
        // Admit one batch before checking guards. Recursive opens stay queued
        // and prevent commitment; their wake schedules another bounded pass.
        self.spawn_windows(event_loop);
        let exit = self.close.step(
            self.slots
                .iter_mut()
                .map(|slot| (slot.app.world_mut(), &mut slot.close, slot.primary)),
        );
        if exit {
            // No application code may run between guard validation and the
            // atomic queue/admission commitment. An open racing this check
            // either joins the drain or occurs after shutdown was committed.
            if self.hub.try_commit_exit() {
                event_loop.exit();
            } else {
                self.wake.wake();
            }
            return;
        }
        for slot in &mut self.slots {
            if std::mem::take(&mut slot.close.needs_frame) {
                if rndr_core::window::is_visible(slot.app.world()) {
                    slot.request_redraw();
                } else {
                    // A newly requested hidden gate still needs a system pass,
                    // even with no queued task or redraw to wake it.
                    slot.run_hidden_frame(event_loop);
                }
            }
        }
        self.slots.retain(|slot| !slot.close.closed);
        if self.slots.is_empty() && self.hub.try_commit_exit() {
            event_loop.exit();
        }
    }

    fn spawn_windows(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.pen.install();
        for open in self.hub.take_opens() {
            let mut app = App::new();
            app.world_mut().insert_resource(open.config.clone());
            let desktop = self.hub.attach();
            let desktop_id = desktop.id();
            app.world_mut().insert_resource(desktop);
            app.set_wake(Arc::clone(&self.wake));
            app.set_executor(Arc::clone(&self.executor));
            install_text_engine(&mut app);
            (open.build)(&mut app);
            let config = WindowConfig::get(app.world());
            self.slots.push(Handler::new(
                app,
                config,
                false,
                desktop_id,
                self.pen.clone(),
                self.bus.clone(),
            ));
        }
        #[cfg(target_os = "macos")]
        native_quit::configure(
            self.slots
                .iter()
                .any(|slot| slot.app.world().has_resource::<rndr_core::CloseGuard>()),
            &self.bus,
        );
        for slot in &mut self.slots {
            if slot.gpu.is_none() && !slot.device_pending() {
                slot.resumed(event_loop);
            }
        }
    }

    fn sync_flow(&self, event_loop: &dyn ActiveEventLoop) {
        let mut deadline = None;
        let mut poll = false;
        for slot in &self.slots {
            let hidden = !rndr_core::window::is_visible(slot.app.world());
            let tray_pending = slot.primary && rndr_core::tray::pending(slot.app.world());
            if hidden && (slot.app.wants_frame() || tray_pending) {
                poll = true;
            } else if slot.app.wants_frame() {
                slot.request_redraw();
            }
            if let Some(next) = slot.app.next_deadline() {
                deadline = Some(match deadline {
                    Some(current) if current < next => current,
                    _ => next,
                });
            }
        }
        if poll {
            event_loop.set_control_flow(ControlFlow::Poll);
            return;
        }
        if self.slots.iter().any(|slot| slot.has_layer()) {
            let soon = Instant::now() + std::time::Duration::from_millis(200);
            deadline = Some(match deadline {
                Some(current) if current < soon => current,
                _ => soon,
            });
        }
        event_loop.set_control_flow(idle_control_flow(deadline, Instant::now()));
    }
}

impl ApplicationHandler for Running {
    fn new_events(&mut self, _event_loop: &dyn ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            for slot in &self.slots {
                slot.request_redraw();
            }
        }
    }

    fn resumed(&mut self, event_loop: &dyn ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }
        self.spawn_windows(event_loop);
    }

    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.resumed(event_loop);
    }

    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        if event_loop.exiting() {
            return;
        }
        let pending: Vec<RunnerEvent> = self.events.try_iter().collect();
        for event in pending {
            match event {
                #[cfg(target_os = "macos")]
                RunnerEvent::Quit => {
                    if let Some(slot) = self.slots.first_mut() {
                        slot.app
                            .world_mut()
                            .resource_mut::<Input>()
                            .expect("input")
                            .quit = true;
                    }
                }
                #[cfg(feature = "accesskit")]
                RunnerEvent::Access(access) => {
                    let id = access.window_id;
                    if let Some(slot) = self.slots.iter_mut().find(|slot| slot.winit_id == Some(id))
                    {
                        slot.on_access(&access);
                    }
                }
                #[cfg(feature = "tray")]
                RunnerEvent::Tray(report) => {
                    if let Some(slot) = self.slots.iter_mut().find(|slot| slot.primary) {
                        rndr_core::tray::push_report(slot.app.world_mut(), report);
                    }
                }
                RunnerEvent::Wake => {}
            }
            self.spawn_windows(event_loop);
            for slot in &mut self.slots {
                slot.install_gpu_if_ready();
                let drew = slot.pump_shell(event_loop);
                if slot.has_layer() {
                    if rndr_core::window::is_visible(slot.app.world()) {
                        if !drew {
                            slot.on_redraw(event_loop);
                        }
                    } else if slot.primary || slot.app.wants_frame() {
                        slot.run_hidden_frame(event_loop);
                    }
                    continue;
                }
                if rndr_core::window::is_visible(slot.app.world()) {
                    slot.request_redraw();
                } else if slot.primary || slot.app.wants_frame() {
                    slot.run_hidden_frame(event_loop);
                }
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &dyn ActiveEventLoop) {
        // winit can finish dispatching a batch after exit() is called. Once
        // committed, no later callback may update a Ready world or build opens.
        if event_loop.exiting() {
            return;
        }
        self.spawn_windows(event_loop);
        for slot in &mut self.slots {
            slot.install_gpu_if_ready();
            slot.pump_shell(event_loop);
            let hidden = !rndr_core::window::is_visible(slot.app.world());
            let tray_pending = slot.primary && rndr_core::tray::pending(slot.app.world());
            if hidden && (slot.app.wants_frame() || tray_pending) {
                slot.run_hidden_frame(event_loop);
            }
        }
        self.reconcile_close(event_loop);
        self.sync_flow(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if event_loop.exiting() {
            return;
        }
        if let Some(slot) = self
            .slots
            .iter_mut()
            .find(|slot| slot.winit_id == Some(window_id))
        {
            slot.window_event(event_loop, window_id, event);
        }
        self.reconcile_close(event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::idle_control_flow;
    use std::time::{Duration, Instant};
    use winit::event_loop::{AsyncRequestSerial, ControlFlow};

    #[test]
    fn a_path_response_must_match_the_outstanding_transfer() {
        let first = AsyncRequestSerial::get();
        let second = AsyncRequestSerial::get();
        let fetch = super::OutstandingFetch {
            id: 4,
            serial: first,
            request: 9,
        };
        assert_eq!(super::accepted_fetch(&fetch, 4, first), Some(9));
        assert_eq!(super::accepted_fetch(&fetch, 4, second), None);
        assert_eq!(super::accepted_fetch(&fetch, 5, first), None);
    }

    #[test]
    fn native_startup_after_headless_update_selects_the_platform_default() {
        let mut app = rndr_core::App::new();
        let label = app
            .world_mut()
            .spawn((rndr_core::Label::new("Desktop text"),));
        app.update();
        assert!(rndr_core::has_implicit_text_engine(app.world()));
        super::install_text_engine(&mut app);
        let engine = rndr_core::text_engine(app.world());
        #[cfg(feature = "parley")]
        assert!(engine.as_any().is::<super::ParleyText>());
        #[cfg(not(feature = "parley"))]
        assert!(engine.as_any().is::<rndr_core::BuiltinText>());
        app.update();
        let expected = engine.layout(
            "Desktop text",
            &rndr_core::style_for(app.world(), label),
            &[],
            f32::INFINITY,
        );
        assert_eq!(
            app.world().get::<rndr_core::TextLayout>(label).unwrap(),
            &expected,
            "stored glyph UVs, advances and caret stops must belong to the replacement engine"
        );
        assert!(!expected.glyphs.is_empty());
        // The renderer must shape the next frame with the chosen engine, not
        // reuse the old bitmap UVs; observation independently binds identity.
        let before = std::sync::Arc::clone(&engine);
        super::install_text_engine(&mut app);
        assert!(std::sync::Arc::ptr_eq(
            &before,
            &rndr_core::text_engine(app.world())
        ));
    }

    #[test]
    fn native_startup_preserves_text_face_before_first_update() {
        let mut app = rndr_core::App::new();
        app.world_mut()
            .insert_resource(rndr_core::TextFace(std::sync::Arc::new(
                rndr_core::BitmapFont,
            )));
        super::install_text_engine(&mut app);
        app.update();
        assert!(rndr_core::text_engine(app.world())
            .as_any()
            .is::<rndr_core::BuiltinText>());
        assert!(!rndr_core::has_implicit_text_engine(app.world()));
    }

    #[test]
    fn native_startup_preserves_application_owned_text() {
        use rndr_core::{App, BuiltinText, TextEngine, TextFace};
        use std::sync::Arc;
        // Explicit engines, even builtin ones, are intentional.
        let mut app = App::new();
        let explicit: Arc<dyn TextEngine> = Arc::new(BuiltinText::new());
        app.world_mut().insert_resource(Arc::clone(&explicit));
        app.update();
        super::install_text_engine(&mut app);
        assert!(Arc::ptr_eq(&explicit, &rndr_core::text_engine(app.world())));
        // Interior configuration of the auto engine is also intentional.
        let mut app = App::new();
        app.update();
        let configured = rndr_core::text_engine(app.world());
        configured.set_fallbacks("custom", &["bitmap".into()]);
        super::install_text_engine(&mut app);
        assert!(Arc::ptr_eq(
            &configured,
            &rndr_core::text_engine(app.world())
        ));
        // TextFace remains the supported legacy customization seam.
        let mut app = App::new();
        app.world_mut()
            .insert_resource(TextFace(Arc::new(rndr_core::BitmapFont)));
        app.update();
        let configured = rndr_core::text_engine(app.world());
        super::install_text_engine(&mut app);
        assert!(Arc::ptr_eq(
            &configured,
            &rndr_core::text_engine(app.world())
        ));
        #[cfg(feature = "parley")]
        {
            let mut app = App::new();
            let custom: Arc<dyn TextEngine> = Arc::new(super::ParleyText::new());
            app.world_mut().insert_resource(Arc::clone(&custom));
            app.update();
            super::install_text_engine(&mut app);
            assert!(Arc::ptr_eq(&custom, &rndr_core::text_engine(app.world())));
        }
    }

    #[test]
    fn hotkey_argument_beats_the_environment() {
        assert_eq!(
            super::select_hotkey(["--hotkey", "Ctrl+Space"], Some("Alt+K")),
            "Ctrl+Space"
        );
        assert_eq!(
            super::select_hotkey(["--hotkey=primary+Space"], Some("Alt+K")),
            "primary+Space"
        );
        assert_eq!(super::select_hotkey(["--other"], Some(" Alt+K ")), "Alt+K");
        assert_eq!(
            super::select_hotkey(std::iter::empty::<&str>(), Some("")),
            super::DEFAULT_HOTKEY
        );
        assert_eq!(
            super::select_hotkey(std::iter::empty::<&str>(), None),
            super::DEFAULT_HOTKEY
        );
    }

    #[test]
    fn blur_cancels_native_held_pointer_and_pending_press() {
        let mut pointer = super::Pointer {
            down: true,
            pressed: true,
            press_at: Some([1.0, 2.0]),
            ..Default::default()
        };
        let mut pointers = super::Pointers::default();
        super::cancel_pointer_input(&mut pointer, &mut pointers);
        assert!(!pointer.down);
        assert!(!pointer.pressed);
        assert!(pointer.released);
        assert!(pointer.press_at.is_none());
        assert!(pointers.mirror().is_none());
    }

    #[test]
    fn reported_text_survives_ctrl_and_alt_but_not_super() {
        let altgr = super::modifiers_of(false, true, true, false);
        assert_eq!(super::produced_text(Some("é"), altgr).as_deref(), Some("é"));
        let alt = super::modifiers_of(false, false, true, false);
        assert_eq!(super::produced_text(Some("€"), alt).as_deref(), Some("€"));
        let ctrl = super::modifiers_of(false, true, false, false);
        assert_eq!(super::produced_text(Some("c"), ctrl).as_deref(), Some("c"));
        assert_eq!(super::produced_text(Some("\u{1}"), ctrl), None);
        let super_key = super::modifiers_of(false, false, false, true);
        assert_eq!(super::produced_text(Some("a"), super_key), None);
        assert_eq!(super::produced_text(None, alt), None);
    }

    #[test]
    fn idle_flow_waits_until_a_future_deadline() {
        let now = Instant::now();
        assert!(matches!(idle_control_flow(None, now), ControlFlow::Wait));
        assert!(matches!(
            idle_control_flow(Some(now + Duration::from_millis(20)), now),
            ControlFlow::WaitUntil(_)
        ));
        assert!(matches!(
            idle_control_flow(Some(now - Duration::from_millis(1)), now),
            ControlFlow::Wait
        ));
    }
}
