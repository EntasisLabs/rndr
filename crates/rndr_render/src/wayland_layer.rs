//! Linux Wayland panel as a `zwlr_layer_shell_v1` surface.
//!
//! winit 0.30 builds an xdg toplevel, which cannot position, hide, or take
//! the keyboard. This client uses its own connection and only runs for a
//! [`rndr_core::WindowKind::Panel`]. A compositor without the protocol returns
//! [`LayerOpenError::NoProtocol`] and the caller keeps the winit toplevel.
//!
//! Attaching a null buffer unmaps the surface and, per the protocol, returns
//! it to the post-create state. The next show commits with no buffer, waits
//! for `configure`, then allows a buffer. The GPU surface object is kept.
//!
//! Keyboard repeat is not synthesized. Text input (`zwp_text_input_v3`) is not
//! bound, so an input method on this surface is not delivered. Neither was
//! exercised on a live compositor.

use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

use rndr_core::{panel_placement, PanelAnchor, WindowConfig};
use wayland_client::protocol::{
    wl_compositor, wl_keyboard, wl_output, wl_pointer, wl_registry, wl_seat, wl_surface,
};
use wayland_client::{Connection, Dispatch, EventQueue, Proxy, QueueHandle};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};
use xkbcommon::xkb;

use crate::overlay_keys::{map_keysym, with_evdev, MappedKey};

const MAX_KEYMAP: usize = 1024 * 1024;
const NAMESPACE: &str = "rndr-panel";

/// Why a panel could not be created. The caller falls back to winit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerOpenError {
    /// `WAYLAND_DISPLAY` is unset or the socket could not be opened.
    NoDisplay,
    /// The compositor did not offer `zwlr_layer_shell_v1`.
    NoProtocol,
    /// Globals, the first configure, or the reader thread did not start.
    Roundtrip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct KeyMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LayerEvent {
    Close,
    Focused(bool),
    Resized,
    Scale(f64),
    /// The configure that follows a show is ready for a buffer.
    Redraw,
    /// Physical pixels.
    Cursor {
        x: f64,
        y: f64,
    },
    /// `button` is 0 left, 1 right, 2 middle.
    Button {
        pressed: bool,
        button: u32,
    },
    /// Logical scroll. Positive Y is up.
    Scroll {
        x: f64,
        y: f64,
    },
    Key {
        mapped: MappedKey,
        mods: KeyMods,
        released: bool,
    },
}

struct Shared {
    logical_w: AtomicU32,
    logical_h: AtomicU32,
    scale: AtomicU32,
    accepts_frames: AtomicBool,
    buffer_attached: AtomicBool,
    needs_configure: AtomicBool,
    visible: AtomicBool,
}

impl Shared {
    fn new(width: u32, height: u32, visible: bool) -> Self {
        Self {
            logical_w: AtomicU32::new(width),
            logical_h: AtomicU32::new(height),
            scale: AtomicU32::new(1),
            accepts_frames: AtomicBool::new(false),
            buffer_attached: AtomicBool::new(false),
            needs_configure: AtomicBool::new(true),
            visible: AtomicBool::new(visible),
        }
    }
}

struct Client {
    shared: Arc<Shared>,
    config: WindowConfig,
    focused: bool,
    factory: Option<wl_compositor::WlCompositor>,
    compositor_version: u32,
    shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    shell_version: u32,
    output: Option<wl_output::WlOutput>,
    output_scale: i32,
    surface: Option<wl_surface::WlSurface>,
    layer: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    seat: Option<wl_seat::WlSeat>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    xkb: Option<xkb::State>,
    mods: KeyMods,
    configured: bool,
    events: Vec<LayerEvent>,
    context: xkb::Context,
}

impl Client {
    fn new(config: WindowConfig, shared: Arc<Shared>) -> Self {
        Self {
            shared,
            config,
            focused: false,
            factory: None,
            compositor_version: 0,
            shell: None,
            shell_version: 0,
            output: None,
            output_scale: 1,
            surface: None,
            layer: None,
            seat: None,
            keyboard: None,
            pointer: None,
            xkb: None,
            mods: KeyMods {
                shift: false,
                ctrl: false,
                alt: false,
                super_key: false,
            },
            configured: false,
            events: Vec::new(),
            context: xkb::Context::new(xkb::CONTEXT_NO_FLAGS),
        }
    }

    fn anchor_bits(anchor: PanelAnchor) -> zwlr_layer_surface_v1::Anchor {
        let mut bits = zwlr_layer_surface_v1::Anchor::empty();
        if anchor.top {
            bits |= zwlr_layer_surface_v1::Anchor::Top;
        }
        if anchor.bottom {
            bits |= zwlr_layer_surface_v1::Anchor::Bottom;
        }
        if anchor.left {
            bits |= zwlr_layer_surface_v1::Anchor::Left;
        }
        if anchor.right {
            bits |= zwlr_layer_surface_v1::Anchor::Right;
        }
        bits
    }

    fn layer_kind(&self) -> zwlr_layer_shell_v1::Layer {
        let visible = self.shared.visible.load(Ordering::Relaxed);
        let placed = panel_placement(&self.config, visible, self.focused);
        if placed.above {
            zwlr_layer_shell_v1::Layer::Overlay
        } else {
            zwlr_layer_shell_v1::Layer::Top
        }
    }

    fn apply_placement(&self) {
        let Some(layer) = &self.layer else {
            return;
        };
        let visible = self.shared.visible.load(Ordering::Relaxed);
        let placed = panel_placement(&self.config, visible, self.focused);
        if self.shell_version >= 2 {
            layer.set_layer(self.layer_kind());
        }
        layer.set_anchor(Self::anchor_bits(placed.anchor));
        layer.set_margin(
            placed.margins.top,
            placed.margins.right,
            placed.margins.bottom,
            placed.margins.left,
        );
        layer.set_size(placed.width, placed.height);
        layer.set_exclusive_zone(0);
        let keyboard = if placed.take_keyboard {
            zwlr_layer_surface_v1::KeyboardInteractivity::Exclusive
        } else {
            zwlr_layer_surface_v1::KeyboardInteractivity::None
        };
        layer.set_keyboard_interactivity(keyboard);
        self.shared.logical_w.store(placed.width, Ordering::Relaxed);
        self.shared
            .logical_h
            .store(placed.height, Ordering::Relaxed);
    }

    fn commit(&self) {
        let Some(surface) = &self.surface else {
            return;
        };
        let scale = self.shared.scale.load(Ordering::Relaxed);
        if self.compositor_version >= 6 && scale >= 1 {
            surface.set_buffer_scale(scale as i32);
        }
        surface.commit();
    }

    fn spawn_surface(&mut self, qh: &QueueHandle<Self>) -> Result<(), LayerOpenError> {
        let factory = self.factory.as_ref().ok_or(LayerOpenError::NoProtocol)?;
        let shell = self.shell.as_ref().ok_or(LayerOpenError::NoProtocol)?;
        let surface = factory.create_surface(qh, ());
        let layer = shell.get_layer_surface(
            &surface,
            self.output.as_ref(),
            self.layer_kind(),
            NAMESPACE.to_string(),
            qh,
            (),
        );
        self.surface = Some(surface);
        self.layer = Some(layer);
        self.apply_placement();
        self.commit();
        Ok(())
    }

    fn note_scale(&mut self) {
        let scale = u32::try_from(self.output_scale.max(1)).unwrap_or(1);
        if self.shared.scale.swap(scale, Ordering::Relaxed) != scale {
            self.events.push(LayerEvent::Scale(f64::from(scale)));
        }
    }

    fn physical_cursor(&self, x: f64, y: f64) -> LayerEvent {
        let scale = f64::from(self.shared.scale.load(Ordering::Relaxed).max(1));
        LayerEvent::Cursor {
            x: x * scale,
            y: y * scale,
        }
    }
}

pub(crate) struct LayerWindow {
    connection: Connection,
    queue: EventQueue<Client>,
    client: Client,
    shared: Arc<Shared>,
    stop: Arc<AtomicBool>,
    wake_write: Option<std::fs::File>,
    thread: Option<JoinHandle<()>>,
    display_ptr: usize,
    surface_ptr: usize,
}

impl LayerWindow {
    pub(crate) fn open(
        config: &WindowConfig,
        visible: bool,
        focused: bool,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Result<Self, LayerOpenError> {
        let connection = Connection::connect_to_env().map_err(|_| LayerOpenError::NoDisplay)?;
        let mut queue = connection.new_event_queue();
        let qh = queue.handle();
        let _registry = connection.display().get_registry(&qh, ());
        let placed = panel_placement(config, visible, focused);
        let shared = Arc::new(Shared::new(placed.width, placed.height, visible));
        let mut client = Client::new(config.clone(), Arc::clone(&shared));
        client.focused = focused;
        queue
            .roundtrip(&mut client)
            .map_err(|_| LayerOpenError::Roundtrip)?;
        client.spawn_surface(&qh)?;
        queue
            .roundtrip(&mut client)
            .map_err(|_| LayerOpenError::Roundtrip)?;
        if !client.configured {
            queue
                .roundtrip(&mut client)
                .map_err(|_| LayerOpenError::Roundtrip)?;
        }
        if !client.configured {
            return Err(LayerOpenError::Roundtrip);
        }
        let display_ptr = connection.display().id().as_ptr() as usize;
        let surface_ptr = client
            .surface
            .as_ref()
            .ok_or(LayerOpenError::Roundtrip)?
            .id()
            .as_ptr() as usize;
        if display_ptr == 0 || surface_ptr == 0 {
            return Err(LayerOpenError::Roundtrip);
        }
        let stop = Arc::new(AtomicBool::new(false));
        let (wake_write, thread) = spawn_reader(connection.clone(), Arc::clone(&stop), wake)?;
        shared.needs_configure.store(false, Ordering::Relaxed);
        shared.accepts_frames.store(visible, Ordering::Relaxed);
        Ok(Self {
            connection,
            queue,
            client,
            shared,
            stop,
            wake_write: Some(wake_write),
            thread: Some(thread),
            display_ptr,
            surface_ptr,
        })
    }

    pub(crate) fn display_ptr(&self) -> usize {
        self.display_ptr
    }

    pub(crate) fn surface_ptr(&self) -> usize {
        self.surface_ptr
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        f64::from(self.shared.scale.load(Ordering::Relaxed).max(1))
    }

    pub(crate) fn logical_size(&self) -> (u32, u32) {
        (
            self.shared.logical_w.load(Ordering::Relaxed).max(1),
            self.shared.logical_h.load(Ordering::Relaxed).max(1),
        )
    }

    pub(crate) fn accepts_frames(&self) -> bool {
        self.shared.accepts_frames.load(Ordering::Relaxed)
    }

    pub(crate) fn note_presented(&self) {
        self.shared.buffer_attached.store(true, Ordering::Relaxed);
    }

    pub(crate) fn set_visible(&mut self, visible: bool) {
        self.shared.visible.store(visible, Ordering::Relaxed);
        if visible {
            self.client.apply_placement();
            if self.shared.needs_configure.load(Ordering::Relaxed) {
                self.shared.accepts_frames.store(false, Ordering::Relaxed);
            } else {
                self.shared.accepts_frames.store(true, Ordering::Relaxed);
            }
            self.client.commit();
        } else {
            self.shared.accepts_frames.store(false, Ordering::Relaxed);
            self.client.focused = false;
            self.client.apply_placement();
            if self.shared.buffer_attached.swap(false, Ordering::Relaxed) {
                if let Some(surface) = &self.client.surface {
                    surface.attach(None, 0, 0);
                }
                self.shared.needs_configure.store(true, Ordering::Relaxed);
            }
            self.client.commit();
        }
        let _ = self.connection.flush();
    }

    pub(crate) fn set_focused(&mut self, focused: bool) {
        self.client.focused = focused;
        self.client.apply_placement();
        self.client.commit();
        let _ = self.connection.flush();
    }

    pub(crate) fn pump(&mut self) -> Vec<LayerEvent> {
        let _ = self.connection.flush();
        if self.queue.dispatch_pending(&mut self.client).is_err() {
            self.client.events.push(LayerEvent::Close);
        }
        std::mem::take(&mut self.client.events)
    }
}

impl Drop for LayerWindow {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut file) = self.wake_write.take() {
            let _ = file.write_all(&[1]);
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn spawn_reader(
    connection: Connection,
    stop: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) -> Result<(std::fs::File, JoinHandle<()>), LayerOpenError> {
    let mut pipes = [0; 2];
    if unsafe { libc::pipe(pipes.as_mut_ptr()) } != 0 {
        return Err(LayerOpenError::Roundtrip);
    }
    for fd in pipes {
        unsafe {
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
    let read_fd = pipes[0];
    let write = unsafe { std::fs::File::from(OwnedFd::from_raw_fd(pipes[1])) };
    let wayland_fd = connection.backend().poll_fd().as_raw_fd();
    let thread = std::thread::Builder::new()
        .name("rndr-layer".to_string())
        .spawn(move || reader_loop(connection, wayland_fd, read_fd, stop, wake))
        .map_err(|_| {
            unsafe { libc::close(read_fd) };
            LayerOpenError::Roundtrip
        })?;
    Ok((write, thread))
}

fn reader_loop(
    connection: Connection,
    wayland_fd: i32,
    read_fd: i32,
    stop: Arc<AtomicBool>,
    wake: Arc<dyn Fn() + Send + Sync>,
) {
    let _owned_read = unsafe { OwnedFd::from_raw_fd(read_fd) };
    while !stop.load(Ordering::Relaxed) {
        let mut fds = [
            libc::pollfd {
                fd: wayland_fd,
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: read_fd,
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), 2, 500) };
        if stop.load(Ordering::Relaxed) || (rc > 0 && fds[1].revents != 0) {
            break;
        }
        if rc > 0 && (fds[0].revents & libc::POLLIN) != 0 {
            if let Some(guard) = connection.prepare_read() {
                if guard.read().is_err() {
                    wake();
                    break;
                }
            }
            wake();
        }
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Client {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };
        match interface.as_str() {
            "wl_compositor" if state.factory.is_none() => {
                let version = version.min(6);
                state.compositor_version = version;
                state.factory = Some(registry.bind(name, version, qh, ()));
            }
            "zwlr_layer_shell_v1" if state.shell.is_none() => {
                let version = version.min(4);
                state.shell_version = version;
                state.shell = Some(registry.bind(name, version, qh, ()));
            }
            "wl_output" if state.output.is_none() => {
                state.output = Some(registry.bind(name, version.min(4), qh, ()));
            }
            "wl_seat" if state.seat.is_none() => {
                state.seat = Some(registry.bind(name, version.min(7), qh, ()));
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for Client {
    fn event(
        _: &mut Self,
        _: &wl_compositor::WlCompositor,
        _: wl_compositor::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for Client {
    fn event(
        _: &mut Self,
        _: &zwlr_layer_shell_v1::ZwlrLayerShellV1,
        _: zwlr_layer_shell_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_surface::WlSurface, ()> for Client {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: wl_surface::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for Client {
    fn event(
        state: &mut Self,
        layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                layer.ack_configure(serial);
                if width > 0 {
                    state.shared.logical_w.store(width, Ordering::Relaxed);
                }
                if height > 0 {
                    state.shared.logical_h.store(height, Ordering::Relaxed);
                }
                state.configured = true;
                state.shared.needs_configure.store(false, Ordering::Relaxed);
                if state.shared.visible.load(Ordering::Relaxed) {
                    state.shared.accepts_frames.store(true, Ordering::Relaxed);
                    state.events.push(LayerEvent::Redraw);
                }
                state.events.push(LayerEvent::Resized);
            }
            zwlr_layer_surface_v1::Event::Closed => state.events.push(LayerEvent::Close),
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_output::WlOutput,
        event: wl_output::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_output::Event::Scale { factor } => {
                state.output_scale = factor;
                state.note_scale();
            }
            wl_output::Event::Done => state.commit(),
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Client {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities { capabilities } = event else {
            return;
        };
        let wayland_client::WEnum::Value(bits) = capabilities else {
            return;
        };
        if bits.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
            state.keyboard = Some(seat.get_keyboard(qh, ()));
        }
        if bits.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
            state.pointer = Some(seat.get_pointer(qh, ()));
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Keymap {
                format: _,
                fd,
                size,
            } => {
                state.install_keymap(fd, size);
            }
            wl_keyboard::Event::Enter { .. } => {
                state.events.push(LayerEvent::Focused(true));
            }
            wl_keyboard::Event::Leave { .. } => {
                state.events.push(LayerEvent::Focused(false));
            }
            wl_keyboard::Event::Key {
                key,
                state: key_state,
                ..
            } => state.key_event(key, key_state),
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
                ..
            } => {
                if let Some(xkb) = state.xkb.as_mut() {
                    xkb.update_mask(mods_depressed, mods_latched, mods_locked, 0, 0, group);
                    state.mods = mods_of(xkb);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for Client {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface_x,
                surface_y,
                ..
            }
            | wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state
                    .events
                    .push(state.physical_cursor(surface_x, surface_y));
            }
            wl_pointer::Event::Button {
                button,
                state: button_state,
                ..
            } => {
                let Some(index) = pointer_button(button) else {
                    return;
                };
                let pressed = matches!(
                    button_state,
                    wayland_client::WEnum::Value(wl_pointer::ButtonState::Pressed)
                );
                state.events.push(LayerEvent::Button {
                    pressed,
                    button: index,
                });
            }
            wl_pointer::Event::Axis { axis, value, .. } => match axis {
                wayland_client::WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                    state.events.push(LayerEvent::Scroll { x: 0.0, y: -value });
                }
                wayland_client::WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                    state.events.push(LayerEvent::Scroll { x: value, y: 0.0 });
                }
                _ => {}
            },
            _ => {}
        }
    }
}

impl Client {
    fn install_keymap(&mut self, fd: OwnedFd, size: u32) {
        let mut file = std::fs::File::from(fd);
        let size = usize::try_from(size).unwrap_or(0).min(MAX_KEYMAP);
        let mut buf = vec![0u8; size];
        if file.read_exact(&mut buf).is_err() {
            return;
        }
        if let Some(end) = buf.iter().position(|byte| *byte == 0) {
            buf.truncate(end);
        }
        let Ok(text) = String::from_utf8(buf) else {
            return;
        };
        let Some(keymap) = xkb::Keymap::new_from_string(
            &self.context,
            text,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        ) else {
            return;
        };
        self.xkb = Some(xkb::State::new(&keymap));
    }

    fn key_event(&mut self, key: u32, key_state: wayland_client::WEnum<wl_keyboard::KeyState>) {
        let pressed = matches!(
            key_state,
            wayland_client::WEnum::Value(wl_keyboard::KeyState::Pressed)
        );
        let code = xkb::Keycode::new(key.saturating_add(8));
        let (sym, utf8) = if let Some(xkb) = self.xkb.as_mut() {
            if pressed {
                xkb.update_key(code, xkb::KeyDirection::Down);
            }
            let sym = xkb.key_get_one_sym(code).raw();
            let utf8 = if pressed {
                xkb.key_get_utf8(code)
            } else {
                String::new()
            };
            if !pressed {
                xkb.update_key(code, xkb::KeyDirection::Up);
            }
            (sym, utf8)
        } else {
            return;
        };
        let Some(mapped) = map_keysym(sym, &utf8, !pressed) else {
            return;
        };
        self.events.push(LayerEvent::Key {
            mapped: with_evdev(mapped, key),
            mods: self.mods,
            released: !pressed,
        });
    }
}

fn pointer_button(code: u32) -> Option<u32> {
    match code {
        0x110 => Some(0),
        0x111 => Some(1),
        0x112 => Some(2),
        _ => None,
    }
}

fn mods_of(state: &xkb::State) -> KeyMods {
    KeyMods {
        shift: state.mod_name_is_active(xkb::MOD_NAME_SHIFT, xkb::STATE_MODS_EFFECTIVE),
        ctrl: state.mod_name_is_active(xkb::MOD_NAME_CTRL, xkb::STATE_MODS_EFFECTIVE),
        alt: state.mod_name_is_active(xkb::MOD_NAME_ALT, xkb::STATE_MODS_EFFECTIVE),
        super_key: state.mod_name_is_active(xkb::MOD_NAME_LOGO, xkb::STATE_MODS_EFFECTIVE),
    }
}
