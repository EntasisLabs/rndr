//! winit + wgpu plugin. Writes [`rndr_core::Input`] in logical points and draws the flat rect list.

mod gpu;

use std::sync::Arc;

use rndr_core::{App, Input, Key as Action, Pointer, MOUSE_ID};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use gpu::Gpu;

/// Block on the desktop event loop. Core stays headless; this is the only winit caller.
pub fn run(app: App) {
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut handler = Handler {
        app,
        gpu: None,
        cursor_px: [0.0, 0.0],
        pointer: Pointer::default(),
        scroll: [0.0, 0.0],
        key: None,
        chars: String::new(),
        shift: false,
        ctrl: false,
        alt: false,
        super_key: false,
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
    key: Option<Action>,
    chars: String,
    shift: bool,
    ctrl: bool,
    alt: bool,
    super_key: bool,
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
        if let Some(input) = self.app.world_mut().resource_mut::<Input>() {
            input.pointer = self.pointer;
            input.scroll = self.scroll;
            input.key = self.key;
            input.chars.clone_from(&self.chars);
            input.shift = self.shift;
            input.ctrl = self.ctrl;
        }
        self.pointer.pressed = false;
        self.scroll = [0.0, 0.0];
        self.key = None;
        self.chars.clear();
    }
}

fn map_key(key: &Key) -> Option<Action> {
    match key {
        Key::Named(NamedKey::ArrowUp) => Some(Action::Up),
        Key::Named(NamedKey::ArrowDown) => Some(Action::Down),
        Key::Named(NamedKey::ArrowLeft) => Some(Action::Left),
        Key::Named(NamedKey::ArrowRight) => Some(Action::Right),
        Key::Named(NamedKey::Enter) => Some(Action::Enter),
        Key::Named(NamedKey::Escape) => Some(Action::Escape),
        Key::Named(NamedKey::Tab) => Some(Action::Tab),
        Key::Named(NamedKey::Backspace) => Some(Action::Backspace),
        Key::Named(NamedKey::Delete) => Some(Action::Delete),
        _ => None,
    }
}

impl ApplicationHandler for Handler {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("rndr")
            .with_inner_size(winit::dpi::LogicalSize::new(800.0, 600.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("window"));
        let gpu = pollster::block_on(Gpu::new(event_loop.owned_display_handle(), window));
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let mut redraw = false;
                if let Some(key) = map_key(&event.logical_key) {
                    self.key = Some(key);
                    redraw = true;
                }
                if self.ctrl {
                    if let Key::Character(text) = &event.logical_key {
                        if text.eq_ignore_ascii_case("s") {
                            self.key = Some(Action::Save);
                            redraw = true;
                        }
                        if text.eq_ignore_ascii_case("n") {
                            self.key = Some(Action::New);
                            redraw = true;
                        }
                    }
                }
                if !self.ctrl && !self.alt && !self.super_key {
                    if let Key::Character(text) = &event.logical_key {
                        for ch in text.chars() {
                            if matches!(ch, ' '..='~') {
                                self.chars.push(ch);
                                redraw = true;
                            }
                        }
                    }
                }
                if redraw {
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
                if let Some(gpu) = &mut self.gpu {
                    gpu.draw(&mut self.app);
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
