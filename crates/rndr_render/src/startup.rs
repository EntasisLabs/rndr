//! When device creation runs, and the clock for the first present.
//!
//! A window that is already visible waits on the UI thread so the first
//! painted frame is not a race. A window that starts hidden starts adapter
//! and device creation on another thread before the event loop waits. Show
//! and focus never start that work. The wgpu surface is kept across hide and
//! show, and each window still has its own device.

use std::time::{Duration, Instant};

/// Where adapter and device creation run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceInit {
    /// Visible at creation. The UI thread waits.
    OnUiThread,
    /// Hidden at creation. Creation is already running off the UI thread.
    OffUiThread,
}

/// `visible_at_start` is the logical window phase when the runner creates it.
pub fn device_init_plan(visible_at_start: bool) -> DeviceInit {
    if visible_at_start {
        DeviceInit::OnUiThread
    } else {
        DeviceInit::OffUiThread
    }
}

/// Durations from the start of window resume.
///
/// `first_present` stays empty until a frame is actually presented. A window
/// that is still hidden has not presented. This is not a claim about a
/// particular millisecond budget on a machine this process has not measured.
#[derive(Clone, Debug)]
pub struct StartupTrace {
    /// When resume began, before the window or the device.
    pub started: Instant,
    /// Window or layer surface exists. Device creation may still be running.
    pub window_ready: Option<Duration>,
    /// Adapter, device, and pipelines are installed. Show did not start them.
    pub device_ready: Option<Duration>,
    /// First successful present. Empty while the window has not presented.
    pub first_present: Option<Duration>,
    /// True only for [`DeviceInit::OnUiThread`].
    pub device_init_blocked_ui_thread: bool,
}

impl StartupTrace {
    pub(crate) fn start() -> Self {
        Self {
            started: Instant::now(),
            window_ready: None,
            device_ready: None,
            first_present: None,
            device_init_blocked_ui_thread: false,
        }
    }

    pub(crate) fn note_present(&mut self) {
        if self.first_present.is_none() {
            self.first_present = Some(self.started.elapsed());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hidden_window_does_not_plan_ui_thread_device_init() {
        assert_eq!(device_init_plan(false), DeviceInit::OffUiThread);
        assert_eq!(device_init_plan(true), DeviceInit::OnUiThread);
    }

    #[test]
    fn the_first_present_is_recorded_once() {
        let mut trace = StartupTrace::start();
        assert!(trace.first_present.is_none());
        trace.note_present();
        let first = trace.first_present.unwrap();
        trace.note_present();
        assert_eq!(trace.first_present.unwrap(), first);
    }
}
