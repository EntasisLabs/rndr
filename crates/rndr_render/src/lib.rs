//! winit + wgpu plugin. Writes [`rndr_core::Input`] in logical points and presents the quad list.

mod gpu;

use std::sync::Arc;
use std::time::Instant;

use rndr_core::{
    legacy_key, text_face, App, Clipboard, Input, KeyCode as Physical, KeyEvent, LogicalKey,
    Modifiers, NamedKey as CoreNamed, Platform, Pointer, Viewport, Wake, MOUSE_ID,
};
use rndr_paint::{atlas_r8, frame, ATLAS_H, ATLAS_W};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, Ime, MouseButton, MouseScrollDelta, StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;
use winit::window::{Window, WindowId};

use gpu::Gpu;

/// User event the runtime posts when a task finishes or a timer is armed.
#[derive(Debug)]
enum RunnerEvent {
    Wake,
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

/// Block on the desktop event loop. Core stays headless; this is the only winit caller.
pub fn run(mut app: App) {
    let mut builder = EventLoop::<RunnerEvent>::with_user_event();
    let event_loop = builder.build().expect("event loop");
    let proxy = event_loop.create_proxy();
    app.set_wake(Arc::new(ProxyWake { proxy }));
    app.set_executor(rndr_core::thread_pool());
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut handler = Handler {
        app,
        gpu: None,
        cursor_px: [0.0, 0.0],
        pointer: Pointer::default(),
        scroll: [0.0, 0.0],
        keys: Vec::new(),
        ime_preedit: String::new(),
        ime_commit: String::new(),
        shift: false,
        ctrl: false,
        alt: false,
        super_key: false,
        platform: host_platform(),
    };
    event_loop.run_app(&mut handler).expect("run");
}

struct Handler {
    app: App,
    gpu: Option<Gpu>,
    /// Last cursor in physical pixels. Converted to points in [`Handler::sync_input`].
    cursor_px: [f32; 2],
    pointer: Pointer,
    scroll: [f32; 2],
    keys: Vec<KeyEvent>,
    ime_preedit: String,
    ime_commit: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_key: bool,
    platform: Platform,
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
                .first()
                .and_then(|event| legacy_key(event, platform));
            input.chars.clear();
            for event in &self.keys {
                if let Some(text) = event.insertable_text() {
                    input.chars.push_str(&text);
                }
            }
            input.ime_preedit.clone_from(&self.ime_preedit);
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
    fn redraw_frame(&mut self) -> bool {
        let Some((width, height, scale)) = self.gpu.as_ref().map(|gpu| {
            let size = gpu.window().inner_size();
            (
                size.width.max(1),
                size.height.max(1),
                positive_scale(gpu.window().scale_factor()),
            )
        }) else {
            return false;
        };
        if let Some(viewport) = self.app.world_mut().resource_mut::<Viewport>() {
            viewport.width = width as f32 / scale;
            viewport.height = height as f32 / scale;
            viewport.scale = scale;
        }
        let painted = frame(&mut self.app);
        // winit 0.30 has no OS clipboard. The resource is the value; clear the
        // dirty bit after the frame so a platform can see that core wrote it.
        if let Some(clipboard) = self.app.world_mut().resource_mut::<Clipboard>() {
            clipboard.dirty = false;
        }
        let scale = self
            .app
            .world()
            .resource::<Viewport>()
            .map(|viewport| viewport.pixels_per_point())
            .unwrap_or(1.0);
        self.gpu
            .as_mut()
            .expect("surface")
            .present(&painted.quads, width, height, scale)
    }
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

fn map_key_event(event: &winit::event::KeyEvent, modifiers: Modifiers) -> Option<KeyEvent> {
    let logical = map_logical(&event.key_without_modifiers());
    let code = map_physical(&event.physical_key);
    let text = produced_text(event.text.as_deref(), modifiers);
    if matches!(logical, LogicalKey::Unidentified) && code.is_none() && text.is_none() {
        return None;
    }
    Some(KeyEvent {
        logical,
        text,
        code,
        modifiers,
        repeat: event.repeat,
    })
}

fn produced_text(text: Option<&str>, modifiers: Modifiers) -> Option<String> {
    if modifiers.command() {
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
        if self.app.wants_frame() {
            self.request_redraw();
        }
        let now = Instant::now();
        event_loop.set_control_flow(idle_control_flow(self.app.next_deadline(), now));
    }
}

impl ApplicationHandler<RunnerEvent> for Handler {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. }) {
            self.request_redraw();
        }
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, _event: RunnerEvent) {
        self.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.sync_flow(event_loop);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("rndr")
            .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let face = text_face(self.app.world());
        let (atlas_w, atlas_h, atlas_pixels) = if let Some(atlas) = face.atlas() {
            (atlas.width, atlas.height, atlas.r8.to_vec())
        } else {
            (ATLAS_W, ATLAS_H, atlas_r8())
        };
        let gpu = pollster::block_on(Gpu::new(
            event_loop.owned_display_handle(),
            window,
            atlas_w,
            atlas_h,
            &atlas_pixels,
        ));
        gpu.window().request_redraw();
        self.gpu = Some(gpu);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(modifiers) => {
                let state = modifiers.state();
                self.shift = state.shift_key();
                self.ctrl = state.control_key();
                self.alt = state.alt_key();
                self.super_key = state.super_key();
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.ime_preedit = text;
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.ime_commit.push_str(&text);
                self.ime_preedit.clear();
                if let Some(gpu) = &self.gpu {
                    gpu.window().request_redraw();
                }
            }
            WindowEvent::Ime(Ime::Enabled | Ime::Disabled) => {}
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let modifiers = modifiers_of(self.shift, self.ctrl, self.alt, self.super_key);
                if let Some(key) = map_key_event(&event, modifiers) {
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
                if self
                    .app
                    .world()
                    .resource::<Input>()
                    .is_some_and(|input| input.quit)
                {
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
