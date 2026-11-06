//! The OS window the GPU surface is attached to.
//!
//! A normal window is a winit window. A Linux Wayland [`rndr_core::WindowKind::Panel`]
//! is a layer-shell surface when the compositor offers the protocol. Hide and
//! show do not drop the surface.

#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use rndr_core::{Modifiers, WindowEffect};
use winit::cursor::{CursorIcon, CustomCursor};
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalSize};
use winit::dpi::{Position, Size};
use winit::event_loop::OwnedDisplayHandle;
use winit::window::Window;

use crate::platform;

#[cfg(target_os = "linux")]
use crate::wayland_layer::LayerWindow;

/// Input the layer-shell seat produced. Empty for a winit window.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) enum ShellEvent {
    Close,
    Focused(bool),
    Resized,
    Scale(f64),
    /// Physical pixels.
    Cursor {
        x: f64,
        y: f64,
    },
    /// 0 left, 1 right, 2 middle.
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
        logical: rndr_core::LogicalKey,
        text: Option<String>,
        code: Option<rndr_core::KeyCode>,
        mods: Modifiers,
        released: bool,
    },
    Redraw,
}

#[cfg(target_os = "linux")]
pub(crate) struct LayerHost {
    window: LayerWindow,
    redraw: AtomicBool,
    wake: Arc<dyn Fn() + Send + Sync>,
}

pub(crate) enum Shell {
    Winit(Arc<dyn Window>),
    #[cfg(target_os = "linux")]
    Layer(Box<LayerHost>),
}

impl Shell {
    pub(crate) fn winit(window: Arc<dyn Window>) -> Self {
        Self::Winit(window)
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn layer(window: LayerWindow, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::Layer(Box::new(LayerHost {
            window,
            redraw: AtomicBool::new(false),
            wake,
        }))
    }

    /// Start the layer-shell socket reader, if this is one. A winit window
    /// already receives its events from the winit loop.
    pub(crate) fn start_io(&mut self) {
        #[cfg(target_os = "linux")]
        if let Self::Layer(host) = self {
            let wake = Arc::clone(&host.wake);
            host.window.start_io(wake);
        }
    }

    pub(crate) fn is_layer(&self) -> bool {
        match self {
            Self::Winit(_) => false,
            #[cfg(target_os = "linux")]
            Self::Layer(_) => true,
        }
    }

    #[cfg(feature = "accesskit")]
    pub(crate) fn winit_arc(&self) -> Option<Arc<dyn Window>> {
        match self {
            Self::Winit(window) => Some(Arc::clone(window)),
            #[cfg(target_os = "linux")]
            Self::Layer(_) => None,
        }
    }

    pub(crate) fn winit_window(&self) -> Option<&dyn Window> {
        match self {
            Self::Winit(window) => Some(window.as_ref()),
            #[cfg(target_os = "linux")]
            Self::Layer(_) => None,
        }
    }

    pub(crate) fn scale_factor(&self) -> f64 {
        match self {
            Self::Winit(window) => window.scale_factor(),
            #[cfg(target_os = "linux")]
            Self::Layer(host) => host.window.scale_factor(),
        }
    }

    pub(crate) fn inner_size(&self) -> PhysicalSize<u32> {
        match self {
            Self::Winit(window) => window.surface_size(),
            #[cfg(target_os = "linux")]
            Self::Layer(host) => {
                let (width, height) = host.window.logical_size();
                let scale = host.window.scale_factor().max(1.0);
                PhysicalSize::new(
                    ((width as f64) * scale).round().max(1.0) as u32,
                    ((height as f64) * scale).round().max(1.0) as u32,
                )
            }
        }
    }

    pub(crate) fn request_redraw(&self) {
        match self {
            Self::Winit(window) => window.request_redraw(),
            #[cfg(target_os = "linux")]
            Self::Layer(host) => {
                if host
                    .redraw
                    .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
                    .is_ok()
                {
                    (host.wake)();
                }
            }
        }
    }

    pub(crate) fn set_ime_allowed(&self, allow: bool) {
        let Some(window) = self.winit_window() else {
            return;
        };
        if allow {
            let data = winit::window::ImeRequestData::default().with_cursor_area(
                Position::Logical(LogicalPosition::new(0.0, 0.0)),
                Size::Logical(LogicalSize::new(1.0, 16.0)),
            );
            let Some(enable) = winit::window::ImeEnableRequest::new(
                winit::window::ImeCapabilities::new().with_cursor_area(),
                data,
            ) else {
                return;
            };
            let _ = window.request_ime_update(winit::window::ImeRequest::Enable(enable));
        } else {
            let _ = window.request_ime_update(winit::window::ImeRequest::Disable);
        }
    }

    pub(crate) fn set_ime_cursor_area(
        &self,
        position: LogicalPosition<f64>,
        size: LogicalSize<f64>,
    ) {
        let Some(window) = self.winit_window() else {
            return;
        };
        let _ = window.request_ime_update(winit::window::ImeRequest::Update(
            winit::window::ImeRequestData::default()
                .with_cursor_area(Position::Logical(position), Size::Logical(size)),
        ));
    }

    pub(crate) fn set_cursor_icon(&self, icon: CursorIcon) {
        if let Some(window) = self.winit_window() {
            window.set_cursor(icon.into());
        }
    }

    pub(crate) fn set_custom_cursor(&self, cursor: CustomCursor) {
        if let Some(window) = self.winit_window() {
            window.set_cursor(cursor.clone().into());
        }
    }

    pub(crate) fn apply_effect(&mut self, panel: bool, effect: WindowEffect) {
        match self {
            Self::Winit(window) => platform::apply_effect(window.as_ref(), panel, effect),
            #[cfg(target_os = "linux")]
            Self::Layer(host) => match effect {
                WindowEffect::Show => host.window.set_visible(true),
                WindowEffect::Hide => host.window.set_visible(false),
                WindowEffect::Focus => host.window.set_focused(true),
            },
        }
    }

    pub(crate) fn accepts_frames(&self) -> bool {
        match self {
            Self::Winit(_) => true,
            #[cfg(target_os = "linux")]
            Self::Layer(host) => host.window.accepts_frames(),
        }
    }

    pub(crate) fn note_presented(&self) {
        #[cfg(target_os = "linux")]
        if let Self::Layer(host) = self {
            host.window.note_presented();
        }
        #[cfg(not(target_os = "linux"))]
        let _ = self;
    }

    pub(crate) fn pump(&mut self) -> Vec<ShellEvent> {
        #[cfg(target_os = "linux")]
        if let Self::Layer(host) = self {
            let redraw = host.redraw.swap(false, Ordering::Relaxed);
            let mut events = host
                .window
                .pump()
                .into_iter()
                .map(map_layer_event)
                .collect::<Vec<_>>();
            if redraw {
                events.push(ShellEvent::Redraw);
            }
            return events;
        }
        Vec::new()
    }

    pub(crate) fn create_wgpu(
        &self,
        winit_display: OwnedDisplayHandle,
    ) -> (wgpu::Instance, wgpu::Surface<'static>) {
        match self {
            Self::Winit(window) => {
                let instance = wgpu::Instance::new(
                    wgpu::InstanceDescriptor::new_with_display_handle(Box::new(winit_display)),
                );
                let surface = instance
                    .create_surface(window.clone())
                    .expect("create surface");
                (instance, surface)
            }
            #[cfg(target_os = "linux")]
            Self::Layer(host) => {
                let _ = winit_display;
                let display = host.window.display_ptr();
                let surface_ptr = host.window.surface_ptr();
                let instance =
                    wgpu::Instance::new(wgpu::InstanceDescriptor::new_with_display_handle(
                        Box::new(LayerDisplay(display)),
                    ));
                let display_nn = std::ptr::NonNull::new(display as *mut std::ffi::c_void)
                    .expect("wayland display");
                let surface_nn = std::ptr::NonNull::new(surface_ptr as *mut std::ffi::c_void)
                    .expect("wayland surface");
                // Safety: both pointers belong to the layer-shell connection stored in
                // this shell. The wgpu surface is dropped before that connection.
                let surface = unsafe {
                    instance
                        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                            raw_display_handle: Some(wgpu::rwh::RawDisplayHandle::Wayland(
                                wgpu::rwh::WaylandDisplayHandle::new(display_nn),
                            )),
                            raw_window_handle: wgpu::rwh::RawWindowHandle::Wayland(
                                wgpu::rwh::WaylandWindowHandle::new(surface_nn),
                            ),
                        })
                        .expect("create surface")
                };
                (instance, surface)
            }
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Debug)]
struct LayerDisplay(usize);

#[cfg(target_os = "linux")]
// The pointer is the `wl_display` owned by the layer-shell connection. It is
// only handed to wgpu on the thread that created that connection.
unsafe impl Send for LayerDisplay {}
#[cfg(target_os = "linux")]
// Same pointer as `Send`. wgpu requires the display handle to be shareable.
unsafe impl Sync for LayerDisplay {}

#[cfg(target_os = "linux")]
impl wgpu::rwh::HasDisplayHandle for LayerDisplay {
    fn display_handle(&self) -> Result<wgpu::rwh::DisplayHandle<'_>, wgpu::rwh::HandleError> {
        let ptr = std::ptr::NonNull::new(self.0 as *mut std::ffi::c_void)
            .ok_or(wgpu::rwh::HandleError::Unavailable)?;
        Ok(unsafe {
            wgpu::rwh::DisplayHandle::borrow_raw(wgpu::rwh::RawDisplayHandle::Wayland(
                wgpu::rwh::WaylandDisplayHandle::new(ptr),
            ))
        })
    }
}

#[cfg(target_os = "linux")]
fn map_layer_event(event: crate::wayland_layer::LayerEvent) -> ShellEvent {
    use crate::wayland_layer::LayerEvent;
    match event {
        LayerEvent::Close => ShellEvent::Close,
        LayerEvent::Focused(focused) => ShellEvent::Focused(focused),
        LayerEvent::Resized => ShellEvent::Resized,
        LayerEvent::Scale(scale) => ShellEvent::Scale(scale),
        LayerEvent::Cursor { x, y } => ShellEvent::Cursor { x, y },
        LayerEvent::Button { pressed, button } => ShellEvent::Button { pressed, button },
        LayerEvent::Scroll { x, y } => ShellEvent::Scroll { x, y },
        LayerEvent::Key {
            mapped,
            mods,
            released,
        } => ShellEvent::Key {
            logical: mapped.logical,
            text: mapped.text,
            code: mapped.code,
            mods: Modifiers {
                shift: mods.shift,
                ctrl: mods.ctrl,
                alt: mods.alt,
                super_key: mods.super_key,
            },
            released,
        },
        LayerEvent::Redraw => ShellEvent::Redraw,
    }
}
