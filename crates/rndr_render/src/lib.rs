//! winit + wgpu plugin. Writes [`rndr_core::Input`] in logical points and presents the quad list.
//!
//! The window comes from a [`rndr_core::WindowConfig`] resource. When the app
//! did not insert one, the window stays 800×600, decorated, resizable, and
//! titled `rndr`. Hide and show leave the wgpu surface in place.

mod color;
mod gpu;
#[cfg(feature = "hotkey")]
mod hotkey;
mod platform;
#[cfg(feature = "parley")]
mod text;
#[cfg(feature = "tray")]
mod tray;

#[cfg(feature = "parley")]
pub use text::ParleyText;

use std::sync::Arc;
use std::time::Instant;

#[cfg(feature = "parley")]
use rndr_core::TextEngine;
use rndr_core::{
    ensure_text_engine, legacy_key, text_engine, App, Caret, Clipboard, Focus, Input,
    KeyCode as Physical, KeyEvent, LogicalKey, Modifiers, NamedKey as CoreNamed, Platform, Pointer,
    Rect, TextLayout, Viewport, Wake, WindowConfig, WindowEffect, WindowKind, MOUSE_ID,
    TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
};
use rndr_paint::{atlas_r8, frame, ATLAS_H, ATLAS_W};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::WindowId;

use gpu::Gpu;
#[cfg(feature = "parley")]
use gpu::{AtlasKind, AtlasRegion, AtlasUpload};

/// User event the runtime posts when a task finishes, a timer is armed, or the
/// tray icon is clicked. The tray variant is what wakes `Wait` while the
/// window is hidden.
#[derive(Debug)]
enum RunnerEvent {
    Wake,
    #[cfg(feature = "tray")]
    Tray(rndr_core::TrayReport),
}

struct ProxyWake {
    proxy: EventLoopProxy<RunnerEvent>,
}

impl Wake for ProxyWake {
    fn wake(&self) {
        let _ = self.proxy.send_event(RunnerEvent::Wake);
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

/// Block on the desktop event loop. Core stays headless; this is the only winit caller.
pub fn run(mut app: App) {
    // A hotkey registered before `run` already installed the backend. Installing
    // here covers a Startup system that registers through `rndr_core` directly.
    #[cfg(feature = "hotkey")]
    hotkey::ensure(app.world_mut());
    let config = WindowConfig::get(app.world());
    install_text_engine(&mut app);
    if rndr_core::tray::starts_hidden(app.world()) {
        rndr_core::tray::mark_hidden(app.world_mut());
    }
    let mut builder = EventLoop::<RunnerEvent>::with_user_event();
    platform::configure_event_loop(&mut builder, &config);
    let event_loop = builder.build().expect("event loop");
    let proxy = event_loop.create_proxy();
    install_tray_handlers(&proxy);
    app.set_wake(Arc::new(ProxyWake { proxy }));
    app.set_executor(rndr_core::thread_pool());
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut handler = Handler {
        app,
        config,
        gpu: None,
        cursor_px: [0.0, 0.0],
        pointer: Pointer::default(),
        scroll: [0.0, 0.0],
        keys: Vec::new(),
        ime_preedit: String::new(),
        ime_cursor: None,
        ime_commit: String::new(),
        shift: false,
        ctrl: false,
        alt: false,
        super_key: false,
        platform: host_platform(),
        key_order: 0,
        #[cfg(feature = "tray")]
        tray: tray::Session::new(),
    };
    event_loop.run_app(&mut handler).expect("run");
}

fn install_text_engine(app: &mut App) {
    #[cfg(feature = "parley")]
    if !app.world().has_resource::<Arc<dyn TextEngine>>() {
        let engine: Arc<dyn TextEngine> = Arc::new(ParleyText::new());
        app.world_mut().insert_resource(engine);
    }
    #[cfg(not(feature = "parley"))]
    let _ = app;
}

fn install_tray_handlers(proxy: &EventLoopProxy<RunnerEvent>) {
    #[cfg(feature = "tray")]
    {
        let icon_proxy = proxy.clone();
        tray_icon::TrayIconEvent::set_event_handler(Some(move |event| {
            if let Some(report) = tray::map_icon_event(&event) {
                let _ = icon_proxy.send_event(RunnerEvent::Tray(report));
            }
        }));
        let menu_proxy = proxy.clone();
        tray_icon::menu::MenuEvent::set_event_handler(Some(move |event| {
            if let Some(report) = tray::map_menu_event(&event) {
                let _ = menu_proxy.send_event(RunnerEvent::Tray(report));
            }
        }));
    }
    #[cfg(not(feature = "tray"))]
    {
        let _ = proxy;
    }
}

struct Handler {
    app: App,
    config: WindowConfig,
    gpu: Option<Gpu>,
    /// Last cursor in physical pixels. Converted to points in [`Handler::sync_input`].
    cursor_px: [f32; 2],
    pointer: Pointer,
    scroll: [f32; 2],
    keys: Vec<KeyEvent>,
    ime_preedit: String,
    ime_cursor: Option<(usize, usize)>,
    ime_commit: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_key: bool,
    platform: Platform,
    /// Receive-order stamped onto each [`KeyEvent`]. Starts at 0; the first
    /// key is 1. [`KeyEvent::UNORDERED`] is reserved and never assigned.
    key_order: u64,
    #[cfg(feature = "tray")]
    tray: tray::Session,
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

impl Handler {
    fn scale(&self) -> f32 {
        self.gpu
            .as_ref()
            .map(|gpu| positive_scale(gpu.window().scale_factor()))
            .unwrap_or(1.0)
    }

    fn sync_input(&mut self) {
        let scale = self.scale();
        self.pointer.id = MOUSE_ID;
        self.pointer.position = [self.cursor_px[0] / scale, self.cursor_px[1] / scale];
        let platform = self.platform;
        self.app.world_mut().insert_resource(platform);
        if let Some(input) = self.app.world_mut().resource_mut::<Input>() {
            input.pointer = self.pointer;
            input.scroll = self.scroll;
            input.keys.clone_from(&self.keys);
            input.key = self
                .keys
                .iter()
                .find(|event| !event.released)
                .and_then(|event| legacy_key(event, platform));
            input.chars.clear();
            for event in self.keys.iter().filter(|event| !event.released) {
                if let Some(text) = event.insertable_text() {
                    input.chars.push_str(&text);
                }
            }
            input.ime_preedit.clone_from(&self.ime_preedit);
            input.ime_cursor = self.ime_cursor;
            input.ime_commit.clone_from(&self.ime_commit);
            input.shift = self.shift;
            input.ctrl = self.ctrl;
            input.alt = self.alt;
            input.super_key = self.super_key;
        }
        self.pointer.pressed = false;
        self.scroll = [0.0, 0.0];
        self.keys.clear();
        self.ime_commit.clear();
    }

    /// Write the window into [`Viewport`], step the world, paint, and present.
    /// Returns whether the surface presented, so input edges survive a lost frame.
    /// Hiding does not drop the surface created on resume.
    fn redraw_frame(&mut self) -> bool {
        let Some((raw_width, raw_height, scale)) = self.gpu.as_ref().map(|gpu| {
            let size = gpu.window().inner_size();
            (
                size.width,
                size.height,
                positive_scale(gpu.window().scale_factor()),
            )
        }) else {
            return false;
        };
        // Publish before `frame` so this update's layout sees the new scale and
        // the resize events that flip at the start of `App::update`.
        rndr_core::window::publish_metrics(self.app.world_mut(), raw_width, raw_height, scale);
        let width = raw_width.max(1);
        let height = raw_height.max(1);
        let painted = frame(&mut self.app);
        // winit 0.30 has no OS clipboard. The resource is the value; clear the
        // dirty bit after the frame so a platform can see that core wrote it.
        if let Some(clipboard) = self.app.world_mut().resource_mut::<Clipboard>() {
            clipboard.dirty = false;
        }
        self.upload_atlases();
        let scale = self
            .app
            .world()
            .resource::<Viewport>()
            .map(|viewport| viewport.pixels_per_point())
            .unwrap_or(1.0);
        let presented =
            self.gpu
                .as_mut()
                .expect("surface")
                .present(&painted.quads, width, height, scale);
        self.place_ime_cursor();
        presented
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
        gpu.window().set_ime_cursor_area(
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
    let logical = map_logical(&event.key_without_modifiers());
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
        NamedKey::Space => CoreNamed::Space,
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
            gpu.window().request_redraw();
        }
    }

    fn sync_flow(&self, event_loop: &ActiveEventLoop) {
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

    fn should_exit(&self) -> bool {
        self.app
            .world()
            .resource::<Input>()
            .is_some_and(|input| input.quit)
    }

    /// Step the world without presenting. Tray clicks and timers still run,
    /// and a quit item exits, while the window is hidden.
    fn run_hidden_frame(&mut self, event_loop: &ActiveEventLoop) {
        for _ in 0..8 {
            self.app.update();
            self.apply_effects();
            self.apply_tray();
            if self.should_exit() {
                event_loop.exit();
                return;
            }
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
    fn apply_effects(&mut self) {
        if self.gpu.is_none() {
            return;
        }
        let effects = rndr_core::window::drain_effects(self.app.world_mut());
        if effects.is_empty() {
            return;
        }
        let panel = self.config.kind == WindowKind::Panel;
        let mut redraw = false;
        if let Some(gpu) = &self.gpu {
            let window = gpu.window();
            for effect in effects {
                redraw |= matches!(effect, WindowEffect::Show | WindowEffect::Focus);
                platform::apply_effect(window, panel, effect);
            }
        }
        if redraw {
            self.request_redraw();
        }
    }
}

impl ApplicationHandler<RunnerEvent> for Handler {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.request_redraw();
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: RunnerEvent) {
        #[cfg(feature = "tray")]
        if let RunnerEvent::Tray(report) = event {
            rndr_core::tray::push_report(self.app.world_mut(), report);
        }
        #[cfg(not(feature = "tray"))]
        let _ = event;
        if rndr_core::window::is_visible(self.app.world()) {
            self.request_redraw();
        } else {
            // A hidden window does not get redraws. A hotkey, a finished task,
            // or a tray click still has to step the world on this wake.
            self.run_hidden_frame(event_loop);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let hidden = !rndr_core::window::is_visible(self.app.world());
        if hidden && (self.app.wants_frame() || rndr_core::tray::pending(self.app.world())) {
            self.run_hidden_frame(event_loop);
        }
        if self.should_exit() {
            event_loop.exit();
            return;
        }
        self.sync_flow(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            // A later resume (Android, or a hide that the OS reports as a pause)
            // must keep the surface. Show/hide never drops `Gpu`.
            return;
        }
        let mut attrs = platform::attributes(&self.config);
        if !rndr_core::window::is_visible(self.app.world()) {
            attrs = attrs.with_visible(false);
        }
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        platform::finish_create(&window, event_loop, &self.config);
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
        let gpu = pollster::block_on(Gpu::new(
            event_loop.owned_display_handle(),
            window,
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
        ));
        self.gpu = Some(gpu);
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
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(focused) => {
                if focused {
                    rndr_core::window::platform_focused(self.app.world());
                } else {
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
                self.super_key = state.super_key();
            }
            WindowEvent::Ime(Ime::Preedit(text, cursor)) => {
                self.ime_preedit = text;
                self.ime_cursor = cursor;
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_commit.push_str(&text);
                self.ime_preedit.clear();
                self.ime_cursor = None;
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::Ime(Ime::Enabled | Ime::Disabled) => {}
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
                    self.keys.push(key);
                    if let Some(gpu) = &self.gpu {
                        gpu.window().request_redraw();
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(gpu) = &mut self.gpu {
                    gpu.resize(size);
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_px = [position.x as f32, position.y as f32];
                if self.pointer.down {
                    if let Some(gpu) = &self.gpu {
                        gpu.window().request_redraw();
                    }
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                match state {
                    ElementState::Pressed => {
                        self.pointer.id = MOUSE_ID;
                        self.pointer.pressed = true;
                        self.pointer.down = true;
                    }
                    ElementState::Released => {
                        self.pointer.down = false;
                    }
                }
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
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
                }
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.sync_input();
                if self.redraw_frame() {
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
                if self.should_exit() {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::idle_control_flow;
    use std::time::{Duration, Instant};
    use winit::event_loop::ControlFlow;

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
