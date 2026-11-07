//! AccessKit adapter for winit 0.31.
//!
//! Derived from accesskit_winit 0.34.2 (Apache-2.0). That published crate is
//! built on winit 0.30 and its `EventLoopProxy::send_event`. This copy keeps
//! the same platform adapters and delivers events through a callback, which
//! the runner posts onto its own channel and wakes with `EventLoopProxy::wake_up`.

use std::sync::Arc;

use accesskit::{ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate};
use winit::event::WindowEvent as WinitWindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

#[derive(Debug)]
pub(crate) struct Event {
    pub(crate) window_id: WindowId,
    pub(crate) window_event: WindowEvent,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum WindowEvent {
    InitialTreeRequested,
    ActionRequested(ActionRequest),
    AccessibilityDeactivated,
}

type Callback = Arc<dyn Fn(Event) + Send + Sync>;

/// Desktop origin of the client area.
///
/// `surface_offset` is [`winit::window::Window::surface_position`]: pixels
/// from the window's outer origin to the client area, not a desktop point.
/// The inner origin is the outer origin plus that offset.
pub(crate) fn desktop_inner_origin(outer: [f64; 2], surface_offset: [i32; 2]) -> [f64; 2] {
    [
        outer[0] + f64::from(surface_offset[0]),
        outer[1] + f64::from(surface_offset[1]),
    ]
}

struct Activation {
    window_id: WindowId,
    on_event: Callback,
}

impl ActivationHandler for Activation {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        (self.on_event)(Event {
            window_id: self.window_id,
            window_event: WindowEvent::InitialTreeRequested,
        });
        None
    }
}

struct Action {
    window_id: WindowId,
    on_event: Callback,
}

impl ActionHandler for Action {
    fn do_action(&mut self, request: ActionRequest) {
        (self.on_event)(Event {
            window_id: self.window_id,
            window_event: WindowEvent::ActionRequested(request),
        });
    }
}

struct Deactivation {
    window_id: WindowId,
    on_event: Callback,
}

impl DeactivationHandler for Deactivation {
    fn deactivate_accessibility(&mut self) {
        (self.on_event)(Event {
            window_id: self.window_id,
            window_event: WindowEvent::AccessibilityDeactivated,
        });
    }
}

pub(crate) struct Adapter {
    inner: platform::Adapter,
}

impl Adapter {
    /// The window must still be hidden. `on_event` may run on another thread.
    pub(crate) fn new(
        event_loop: &dyn ActiveEventLoop,
        window: &dyn Window,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Self {
        let window_id = window.id();
        let on_event: Callback = Arc::new(on_event);
        let inner = platform::Adapter::new(
            event_loop,
            window,
            Activation {
                window_id,
                on_event: Arc::clone(&on_event),
            },
            Action {
                window_id,
                on_event: Arc::clone(&on_event),
            },
            Deactivation {
                window_id,
                on_event,
            },
        );
        Self { inner }
    }

    pub(crate) fn process_event(&mut self, window: &dyn Window, event: &WinitWindowEvent) {
        self.inner.process_event(window, event);
    }

    pub(crate) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
        self.inner.update_if_active(updater);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, Rect, TreeUpdate};
    use accesskit_unix::Adapter as UnixAdapter;
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::window::Window;

    pub(super) struct Adapter {
        adapter: UnixAdapter,
    }

    impl Adapter {
        pub(super) fn new(
            _event_loop: &dyn ActiveEventLoop,
            _window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler + Send,
            action_handler: impl 'static + ActionHandler + Send,
            deactivation_handler: impl 'static + DeactivationHandler + Send,
        ) -> Self {
            Self {
                adapter: UnixAdapter::new(activation_handler, action_handler, deactivation_handler),
            }
        }

        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            self.adapter.update_if_active(updater);
        }

        pub(super) fn process_event(&mut self, window: &dyn Window, event: &WindowEvent) {
            match event {
                WindowEvent::Moved(outer_position) => {
                    let outer = [outer_position.x as f64, outer_position.y as f64];
                    let surface = window.surface_position();
                    let inner = super::desktop_inner_origin(outer, [surface.x, surface.y]);
                    let outer_size: (_, _) = window.outer_size().cast::<f64>().into();
                    let inner_size: (_, _) = window.surface_size().cast::<f64>().into();
                    self.adapter.set_root_window_bounds(
                        Rect::from_origin_size((outer[0], outer[1]), outer_size),
                        Rect::from_origin_size((inner[0], inner[1]), inner_size),
                    );
                }
                WindowEvent::SurfaceResized(inner_size) => {
                    let outer_point = window.outer_position().unwrap_or_default();
                    let outer = [f64::from(outer_point.x), f64::from(outer_point.y)];
                    let surface = window.surface_position();
                    let inner = super::desktop_inner_origin(outer, [surface.x, surface.y]);
                    let outer_size: (_, _) = window.outer_size().cast::<f64>().into();
                    let inner_size: (_, _) = inner_size.cast::<f64>().into();
                    self.adapter.set_root_window_bounds(
                        Rect::from_origin_size((outer[0], outer[1]), outer_size),
                        Rect::from_origin_size((inner[0], inner[1]), inner_size),
                    );
                }
                WindowEvent::Focused(is_focused) => {
                    self.adapter.update_window_focus_state(*is_focused);
                }
                _ => {}
            }
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_macos::SubclassingAdapter;
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    pub(super) struct Adapter {
        adapter: SubclassingAdapter,
    }

    impl Adapter {
        pub(super) fn new(
            _event_loop: &dyn ActiveEventLoop,
            window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler,
            action_handler: impl 'static + ActionHandler,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            let view = match window.window_handle().unwrap().as_raw() {
                RawWindowHandle::AppKit(handle) => handle.ns_view.as_ptr(),
                _ => unreachable!("accesskit macos adapter on a non-AppKit window"),
            };
            let adapter =
                unsafe { SubclassingAdapter::new(view, activation_handler, action_handler) };
            Self { adapter }
        }

        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(updater) {
                events.raise();
            }
        }

        pub(super) fn process_event(&mut self, _window: &dyn Window, event: &WindowEvent) {
            if let WindowEvent::Focused(is_focused) = event {
                if let Some(events) = self.adapter.update_view_focus_state(*is_focused) {
                    events.raise();
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use accesskit_windows::{SubclassingAdapter, HWND};
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use winit::window::Window;

    pub(super) struct Adapter {
        adapter: SubclassingAdapter,
    }

    impl Adapter {
        pub(super) fn new(
            _event_loop: &dyn ActiveEventLoop,
            window: &dyn Window,
            activation_handler: impl 'static + ActivationHandler,
            action_handler: impl 'static + ActionHandler + Send,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            let hwnd = match window.window_handle().unwrap().as_raw() {
                RawWindowHandle::Win32(handle) => handle.hwnd.get() as *mut _,
                _ => unreachable!("accesskit windows adapter on a non-Win32 window"),
            };
            let adapter = SubclassingAdapter::new(HWND(hwnd), activation_handler, action_handler);
            Self { adapter }
        }

        pub(super) fn update_if_active(&mut self, updater: impl FnOnce() -> TreeUpdate) {
            if let Some(events) = self.adapter.update_if_active(updater) {
                events.raise();
            }
        }

        pub(super) fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    use accesskit::{ActionHandler, ActivationHandler, DeactivationHandler, TreeUpdate};
    use winit::event::WindowEvent;
    use winit::event_loop::ActiveEventLoop;
    use winit::window::Window;

    pub(super) struct Adapter;

    impl Adapter {
        pub(super) fn new(
            _event_loop: &dyn ActiveEventLoop,
            _window: &dyn Window,
            _activation_handler: impl 'static + ActivationHandler,
            _action_handler: impl 'static + ActionHandler,
            _deactivation_handler: impl 'static + DeactivationHandler,
        ) -> Self {
            Self
        }

        pub(super) fn update_if_active(&mut self, _updater: impl FnOnce() -> TreeUpdate) {}

        pub(super) fn process_event(&mut self, _window: &dyn Window, _event: &WindowEvent) {}
    }
}

#[cfg(test)]
mod tests {
    use super::desktop_inner_origin;

    fn bits(point: [f64; 2]) -> [u64; 2] {
        [point[0].to_bits(), point[1].to_bits()]
    }

    #[test]
    fn inner_origin_is_outer_plus_the_window_relative_surface_offset() {
        let origin = [500.0, 300.0];
        let offset = [0, 30];
        assert_eq!(
            bits(desktop_inner_origin(origin, offset)),
            bits([500.0, 330.0])
        );
        // A move changes the outer origin. The surface offset stays window-relative.
        let moved = [800.0, 100.0];
        assert_eq!(
            bits(desktop_inner_origin(moved, offset)),
            bits([800.0, 130.0])
        );
        // A resize keeps that outer origin and can change the frame inset.
        let resized_offset = [2, 40];
        assert_eq!(
            bits(desktop_inner_origin(moved, resized_offset)),
            bits([802.0, 140.0])
        );
    }
}
