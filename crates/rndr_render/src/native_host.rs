//! Application-owned native child surfaces, driven only on the event-loop thread.
use rndr_core::World;
use std::cell::Cell;
use std::time::Duration;
use web_time::Instant;

/// Borrowed native parent and current client metrics for one host pass.
///
/// Coordinates are physical pixels relative to the top-left client area. Divide
/// by `scale_factor` before submitting logical bounds to `rndr_browser`.
/// The window and its raw handle must not escape this callback. In particular,
/// destroy child views in [`NativeHost::detach`] before the parent is retired.
/// `ArtifactPreview::attach(&context.window, html)` accepts this sized reference.
#[derive(Clone, Copy)]
pub struct NativeHostContext<'a> {
    /// Borrowed winit parent, implementing `HasWindowHandle`.
    pub window: &'a dyn winit::window::Window,
    /// Client width and height in physical pixels (can be zero when minimized).
    pub physical_size: [u32; 2],
    /// Current physical pixels per logical point, finite and positive.
    pub scale_factor: f64,
    /// Application visibility, excluding an occluded or zero-area parent.
    pub visible: bool,
    pub(crate) redraw: &'a Cell<bool>,
}

impl NativeHostContext<'_> {
    /// Request one full application update and repaint after mutating UI state.
    /// Call only when state changes; a polling host does not repaint by default.
    pub fn request_redraw(&self) {
        self.redraw.set(true);
    }
}

/// A UI-thread-only owner of native child views for one application window.
///
/// Created by [`crate::Session::native_host`], never moved to a worker. No
/// `Send`/`Sync` bound is required: WebKit and other UI-thread objects may live
/// here rather than in a `World` resource. RNDR does not enable a browser backend
/// or provide a fallback; the consumer must handle unsupported isolation.
///
/// `frame` runs once before the event loop waits, after that batch's app work,
/// also while hidden. Changes to the world that need an app update should call
/// [`NativeHostContext::request_redraw`]. Polling alone does not rerun app systems.
/// Layer-shell windows (which have no winit parent) do not receive `frame`.
/// `detach` runs once, even if no parent was available, on close or loop teardown.
pub trait NativeHost {
    /// Reconcile attachment, bounds, visibility and asynchronous native state.
    /// Return a delay to poll again without continuous redraw, or `None` to sleep
    /// until the next app/native event. Zero delays are clamped to 1 ms.
    fn frame(&mut self, world: &mut World, context: NativeHostContext<'_>) -> Option<Duration>;

    /// Retire all child views before parent destruction. Called on the UI thread.
    /// The host itself is dropped immediately afterward. Must not panic or
    /// enqueue app work, open windows, or edit close-protected application state:
    /// final close validation/guard consumption may already have committed.
    /// Use the world only to locate and release native child ownership.
    fn detach(&mut self, _world: &mut World) {}
}

pub(crate) type Factory = dyn FnMut(&mut World) -> Box<dyn NativeHost>;

fn poll_deadline(now: Instant, delay: Duration) -> Option<Instant> {
    now.checked_add(delay.max(Duration::from_millis(1)))
}

#[derive(Default)]
pub(crate) struct HostSlot {
    host: Option<Box<dyn NativeHost>>,
    pub(crate) deadline: Option<Instant>,
}

impl HostSlot {
    pub(crate) fn install(&mut self, factory: Option<&mut Box<Factory>>, world: &mut World) {
        if let Some(factory) = factory {
            self.host = Some(factory(world));
        }
    }

    pub(crate) fn frame(&mut self, world: &mut World, context: NativeHostContext<'_>) {
        self.deadline = self.host.as_mut().and_then(|host| {
            host.frame(world, context)
                .and_then(|delay| poll_deadline(Instant::now(), delay))
        });
    }

    pub(crate) fn detach(&mut self, world: &mut World) {
        self.deadline = None;
        if let Some(mut host) = self.host.take() {
            host.detach(world);
            // Drop before returning, while the parent is still alive.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct LocalHost(Rc<RefCell<Vec<&'static str>>>);
    impl NativeHost for LocalHost {
        fn frame(&mut self, _: &mut World, _: NativeHostContext<'_>) -> Option<Duration> {
            None
        }
        fn detach(&mut self, world: &mut World) {
            assert_eq!(world.resource::<u32>(), Some(&7));
            self.0.borrow_mut().push("detach");
        }
    }
    impl Drop for LocalHost {
        fn drop(&mut self) {
            self.0.borrow_mut().push("drop");
        }
    }

    #[test]
    fn factory_accepts_ui_thread_only_state_and_detach_drops_once() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let local = Rc::clone(&log);
        let mut factory: Box<Factory> = Box::new(move |world| {
            world.insert_resource(7_u32);
            Box::new(LocalHost(Rc::clone(&local)))
        });
        let mut world = World::new();
        let mut slot = HostSlot::default();
        slot.install(Some(&mut factory), &mut world);
        slot.deadline = Some(Instant::now());
        slot.detach(&mut world);
        slot.detach(&mut world);
        assert_eq!(&*log.borrow(), &["detach", "drop"]);
        assert!(slot.deadline.is_none());
    }

    #[test]
    fn no_host_preserves_idle_and_teardown_is_harmless() {
        let mut slot = HostSlot::default();
        let mut world = World::new();
        slot.install(None, &mut world);
        slot.detach(&mut world);
        assert!(slot.host.is_none());
        assert!(slot.deadline.is_none());
    }

    #[test]
    fn polling_is_future_bounded_and_overflow_does_not_panic() {
        let now = Instant::now();
        assert_eq!(
            poll_deadline(now, Duration::ZERO),
            Some(now + Duration::from_millis(1))
        );
        assert_eq!(
            poll_deadline(now, Duration::from_millis(16)),
            Some(now + Duration::from_millis(16))
        );
        assert_eq!(poll_deadline(now, Duration::MAX), None);
    }
}
