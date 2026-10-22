//! Turn a [`WindowConfig`] into a winit window and apply show / hide / focus.
//!
//! The GPU surface is not created or destroyed here. Hide uses the platform
//! visibility call and leaves the surface alone.
//!
//! Wayland, in winit 0.30, ignores position, `set_visible`, `focus_window`,
//! and window levels. Those commands still update the logical phase; the
//! compositor keeps the window where it put it.

use rndr_core::{Position, WindowConfig, WindowEffect, WindowKind};
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder};
use winit::window::{Window, WindowAttributes, WindowLevel};

pub(crate) fn configure_event_loop<T>(builder: &mut EventLoopBuilder<T>, config: &WindowConfig) {
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        if config.accessory {
            builder.with_activation_policy(ActivationPolicy::Accessory);
        }
        if config.kind == WindowKind::Panel || config.accessory {
            builder.with_activate_ignoring_other_apps(false);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (builder, config);
    }
}

pub(crate) fn attributes(config: &WindowConfig) -> WindowAttributes {
    let mut attrs = Window::default_attributes()
        .with_title(config.title.clone())
        .with_inner_size(LogicalSize::new(
            f64::from(config.size.width),
            f64::from(config.size.height),
        ))
        .with_resizable(config.resizable)
        .with_decorations(config.decorations)
        .with_transparent(config.transparent)
        .with_window_level(
            if config.always_on_top || config.kind == WindowKind::Panel {
                WindowLevel::AlwaysOnTop
            } else {
                WindowLevel::Normal
            },
        );
    if let Some(min) = config.min_size {
        attrs = attrs.with_min_inner_size(LogicalSize::new(
            f64::from(min.width),
            f64::from(min.height),
        ));
    }
    if config.kind == WindowKind::Panel {
        // A panel must not activate the app just by being created.
        attrs = attrs.with_active(false);
    }
    #[cfg(target_os = "windows")]
    if config.skip_taskbar {
        use winit::platform::windows::WindowAttributesExtWindows;
        attrs = attrs.with_skip_taskbar(true);
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    if config.skip_taskbar {
        use winit::platform::x11::{WindowAttributesExtX11, WindowType};
        // Wayland ignores X11 window types. On X11, utility windows stay off the taskbar.
        attrs = attrs.with_x11_window_type(vec![WindowType::Utility]);
    }
    attrs
}

pub(crate) fn finish_create(window: &Window, event_loop: &ActiveEventLoop, config: &WindowConfig) {
    #[cfg(target_os = "macos")]
    {
        if config.kind == WindowKind::Panel {
            configure_ns_panel(window);
        }
        if config.skip_taskbar {
            if let Some(ns) = ns_window(window) {
                unsafe { ns.setExcludedFromWindowsMenu(true) };
            }
        }
    }
    place(window, event_loop, config);
}

pub(crate) fn apply_effect(window: &Window, panel: bool, effect: WindowEffect) {
    match effect {
        WindowEffect::Show => show(window, panel),
        WindowEffect::Hide => window.set_visible(false),
        WindowEffect::Focus => focus(window, panel),
    }
}

fn show(window: &Window, panel: bool) {
    #[cfg(target_os = "macos")]
    if panel {
        show_panel(window);
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = panel;
    window.set_visible(true);
}

fn focus(window: &Window, panel: bool) {
    #[cfg(target_os = "macos")]
    if panel {
        focus_panel(window);
        return;
    }
    #[cfg(not(target_os = "macos"))]
    let _ = panel;
    window.focus_window();
}

fn place(window: &Window, event_loop: &ActiveEventLoop, config: &WindowConfig) {
    if session_is_wayland(event_loop) {
        return;
    }
    match config.position {
        Position::Platform => {}
        Position::At { x, y } => {
            window.set_outer_position(LogicalPosition::new(f64::from(x), f64::from(y)));
        }
        Position::Center => center(window),
    }
}

fn center(window: &Window) {
    let Some(monitor) = window
        .current_monitor()
        .or_else(|| window.primary_monitor())
    else {
        return;
    };
    let screen = monitor.size();
    let outer = window.outer_size();
    let x = (screen.width as i32 - outer.width as i32) / 2;
    let y = (screen.height as i32 - outer.height as i32) / 2;
    window.set_outer_position(PhysicalPosition::new(x.max(0), y.max(0)));
}

fn session_is_wayland(event_loop: &ActiveEventLoop) -> bool {
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    {
        use winit::platform::wayland::ActiveEventLoopExtWayland;
        event_loop.is_wayland()
    }
    #[cfg(not(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    )))]
    {
        let _ = event_loop;
        false
    }
}

#[cfg(target_os = "macos")]
fn show_panel(window: &Window) {
    if let Some(ns) = ns_window(window) {
        // Order in front without activating the app. `set_visible(true)` would
        // call `makeKeyAndOrderFront` and take key focus as a side effect.
        unsafe { ns.orderFrontRegardless() };
    } else {
        window.set_visible(true);
    }
}

#[cfg(target_os = "macos")]
fn focus_panel(window: &Window) {
    if let Some(ns) = ns_window(window) {
        // `Window::focus_window` calls `activateIgnoringOtherApps`. A panel
        // only becomes key.
        unsafe { ns.makeKeyWindow() };
    } else {
        window.focus_window();
    }
}

#[cfg(target_os = "macos")]
fn configure_ns_panel(window: &Window) {
    let Some(ns) = ns_window(window) else {
        return;
    };
    // winit 0.30 builds an NSWindow. The non-activating bit is documented for
    // NSPanel; we set it, raise the window to pop-up level, and join every
    // Space including full-screen ones. Focus does not activate the app.
    ns.setStyleMask(ns.styleMask() | objc2_app_kit::NSWindowStyleMask::NonactivatingPanel);
    ns.setLevel(objc2_app_kit::NSPopUpMenuWindowLevel);
    unsafe {
        ns.setCollectionBehavior(
            objc2_app_kit::NSWindowCollectionBehavior::CanJoinAllSpaces
                | objc2_app_kit::NSWindowCollectionBehavior::FullScreenAuxiliary
                | objc2_app_kit::NSWindowCollectionBehavior::Stationary
                | objc2_app_kit::NSWindowCollectionBehavior::IgnoresCycle,
        );
        ns.setHidesOnDeactivate(false);
    }
}

#[cfg(target_os = "macos")]
fn ns_window(window: &Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    let handle = window.window_handle().ok()?;
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return None;
    };
    // Safety: winit's AppKit handle is a live NSView on the main thread for
    // the duration of this call. `retain` balances the extra retain when the
    // returned window (and this view) drop.
    let view = unsafe { Retained::<NSView>::retain(appkit.ns_view.as_ptr().cast()) }?;
    view.window()
}
