//! macOS tablet samples.
//!
//! A local event monitor copies pressure, tilt, and twist from tablet-point
//! events. The runner applies the latest sample to [`rndr_core::PEN_ID`] at
//! the window's cursor. Windows pen input arrives as touch force instead, and
//! this monitor is not installed there. Nothing here is checked against a
//! physical tablet in CI.

use std::sync::{Arc, Mutex};

use rndr_core::PointerKind;

#[derive(Clone, Copy, Debug)]
pub(crate) struct PenReading {
    pub pressure: f32,
    pub tilt: Option<[f32; 2]>,
    pub twist: Option<f32>,
    pub kind: PointerKind,
    pub down: bool,
    pub generation: u64,
}

#[derive(Default)]
struct PenState {
    latest: Option<PenReading>,
}

pub(crate) struct PenFeed {
    state: Arc<Mutex<PenState>>,
    #[cfg(target_os = "macos")]
    monitor: Arc<Mutex<Option<MacMonitor>>>,
}

impl Clone for PenFeed {
    fn clone(&self) -> Self {
        Self {
            state: Arc::clone(&self.state),
            #[cfg(target_os = "macos")]
            monitor: Arc::clone(&self.monitor),
        }
    }
}

impl PenFeed {
    pub(crate) fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(PenState::default())),
            #[cfg(target_os = "macos")]
            monitor: Arc::new(Mutex::new(None)),
        }
    }

    /// Install the macOS monitor. On other platforms this is a no-op.
    ///
    /// Safe to call more than once. Clones share the monitor.
    pub(crate) fn install(&self) {
        #[cfg(target_os = "macos")]
        {
            let mut slot = self.monitor.lock().unwrap_or_else(|err| err.into_inner());
            if slot.is_none() {
                *slot = MacMonitor::install(Arc::clone(&self.state));
            }
        }
    }

    /// The newest sample with a generation greater than `seen`.
    pub(crate) fn since(&self, seen: u64) -> Option<PenReading> {
        let state = self.state.lock().unwrap_or_else(|err| err.into_inner());
        state.latest.filter(|reading| reading.generation > seen)
    }
}

#[cfg(target_os = "macos")]
struct MacMonitor {
    _monitor: Option<objc2::rc::Retained<objc2::runtime::AnyObject>>,
    _block: block2::RcBlock<
        dyn Fn(std::ptr::NonNull<objc2_app_kit::NSEvent>) -> *mut objc2_app_kit::NSEvent,
    >,
}

#[cfg(target_os = "macos")]
impl MacMonitor {
    fn install(state: Arc<Mutex<PenState>>) -> Option<Self> {
        use objc2_app_kit::{
            NSEvent, NSEventMask, NSEventSubtype, NSEventType, NSPointingDeviceType,
        };
        use std::ptr::NonNull;

        let block = block2::RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
            let event_ref = unsafe { event.as_ref() };
            if let Some(reading) = read_tablet(event_ref) {
                let mut guard = state.lock().unwrap_or_else(|err| err.into_inner());
                let generation = guard
                    .latest
                    .map(|previous| previous.generation.saturating_add(1))
                    .unwrap_or(1);
                guard.latest = Some(PenReading {
                    generation,
                    ..reading
                });
            }
            event.as_ptr()
        });
        let mask = NSEventMask::LeftMouseDown
            | NSEventMask::LeftMouseUp
            | NSEventMask::RightMouseDown
            | NSEventMask::RightMouseUp
            | NSEventMask::OtherMouseDown
            | NSEventMask::OtherMouseUp
            | NSEventMask::MouseMoved
            | NSEventMask::LeftMouseDragged
            | NSEventMask::RightMouseDragged
            | NSEventMask::OtherMouseDragged
            | NSEventMask::TabletPoint;
        let monitor =
            unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &block) };
        let _ = (
            NSEventType::TabletPoint,
            NSEventSubtype::TabletPoint,
            NSPointingDeviceType::Pen,
        );
        Some(Self {
            _monitor: monitor,
            _block: block,
        })
    }
}

#[cfg(target_os = "macos")]
fn read_tablet(event: &objc2_app_kit::NSEvent) -> Option<PenReading> {
    use objc2_app_kit::{NSEvent, NSEventSubtype, NSEventType, NSPointingDeviceType};

    let kind_of_event = unsafe { event.r#type() };
    let mouse = matches!(
        kind_of_event,
        NSEventType::LeftMouseDown
            | NSEventType::LeftMouseUp
            | NSEventType::RightMouseDown
            | NSEventType::RightMouseUp
            | NSEventType::OtherMouseDown
            | NSEventType::OtherMouseUp
            | NSEventType::MouseMoved
            | NSEventType::LeftMouseDragged
            | NSEventType::RightMouseDragged
            | NSEventType::OtherMouseDragged
    );
    let subtype = unsafe { event.subtype() };
    let tablet = kind_of_event == NSEventType::TabletPoint
        || (mouse && subtype == NSEventSubtype::TabletPoint);
    if !tablet {
        return None;
    }
    let pressure = unsafe { event.pressure() };
    let pressure = if pressure.is_finite() {
        pressure.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let tilt = unsafe { event.tilt() };
    let tilt = if tilt.x.is_finite() && tilt.y.is_finite() {
        Some([tilt.x as f32 * 90.0, tilt.y as f32 * 90.0])
    } else {
        None
    };
    let twist = unsafe { event.rotation() };
    let twist = twist.is_finite().then_some(twist as f32);
    let device = unsafe { event.pointingDeviceType() };
    let kind = if device == NSPointingDeviceType::Eraser {
        PointerKind::Eraser
    } else {
        PointerKind::Pen
    };
    let down = unsafe { NSEvent::pressedMouseButtons() } != 0;
    Some(PenReading {
        pressure,
        tilt,
        twist,
        kind,
        down,
        generation: 0,
    })
}
