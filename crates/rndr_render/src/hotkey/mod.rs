//! Operating-system hotkey backends.
//!
//! The registry and the binding parser live in `rndr_core`. This module grabs
//! the chord and wakes the winit loop. Nothing here polls on a timer.
//!
//! macOS uses Carbon `RegisterEventHotKey` on the application event target,
//! which the Cocoa run loop already drains. Windows blocks in `GetMessage` on
//! a thread that owns `RegisterHotKey`. X11 blocks in `poll` on the display
//! socket and a wake pipe. Wayland uses the XDG GlobalShortcuts portal when
//! the interface is on the session bus, and otherwise the backend reports
//! [`HotkeyFailure::Unsupported`] without falling back to an X11 grab.

mod press;
#[cfg(any(test, all(unix, not(target_os = "macos"))))]
mod repeat;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(all(unix, not(target_os = "macos")))]
mod portal;
#[cfg(target_os = "windows")]
mod windows;
#[cfg(all(unix, not(target_os = "macos")))]
mod x11;

use std::sync::Arc;

use rndr_core::hotkey::HotkeyBackend;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use rndr_core::hotkey::{HotkeyFailure, HotkeyId, PlatformAccel};
use rndr_core::World;

pub fn ensure(world: &mut World) {
    if rndr_core::hotkey::backend_installed(world) {
        return;
    }
    let wake = rndr_core::hotkey::frame_wake(world);
    rndr_core::hotkey::install_backend(world, open(wake));
}

fn open(wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn HotkeyBackend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(macos::MacHotkeys::open(wake))
    }
    #[cfg(target_os = "windows")]
    {
        Box::new(windows::WinHotkeys::open(wake))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        unix_open(wake)
    }
    #[cfg(not(any(unix, target_os = "windows")))]
    {
        let _ = wake;
        Box::new(Unsupported)
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn unix_open(wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn HotkeyBackend> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return match portal::PortalHotkeys::open(Arc::clone(&wake)) {
            Ok(backend) => Box::new(backend),
            Err(()) => Box::new(Unsupported),
        };
    }
    match x11::X11Hotkeys::open(wake) {
        Ok(backend) => Box::new(backend),
        Err(()) => Box::new(Unsupported),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
struct Unsupported;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
impl HotkeyBackend for Unsupported {
    fn register(&mut self, _id: HotkeyId, _accel: &PlatformAccel) -> Result<(), HotkeyFailure> {
        Err(HotkeyFailure::Unsupported)
    }

    fn unregister(&mut self, _id: HotkeyId) {}

    fn poll(&mut self) -> Vec<HotkeyId> {
        Vec::new()
    }
}
