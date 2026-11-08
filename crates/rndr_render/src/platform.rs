//! Turn a [`WindowConfig`] into a winit window and apply show / hide / focus.
//!
//! The GPU surface is not created or destroyed here. Hide uses the platform
//! visibility call and leaves the surface alone.
//!
//! An ordinary Wayland toplevel still ignores position, `set_visible`,
//! `focus_window`, and window levels. A [`WindowKind::Panel`] on Linux Wayland
//! is not created here: the layer-shell client owns that surface. This module
//! still places the winit fallback when that protocol is missing.
//!
//! A Windows panel show uses `SW_SHOWNOACTIVATE` and `HWND_TOPMOST`. Explicit
//! focus takes the foreground. Virtual-desktop pinning is not requested.

use rndr_core::{Position, WindowConfig, WindowEffect, WindowKind};
use winit::dpi::{LogicalPosition, LogicalSize, PhysicalPosition};
use winit::event_loop::{ActiveEventLoop, EventLoopBuilder};
use winit::window::{Window, WindowAttributes, WindowLevel};

pub(crate) fn configure_event_loop(builder: &mut EventLoopBuilder, config: &WindowConfig) {
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
    let mut attrs = WindowAttributes::default()
        .with_title(config.title.clone())
        .with_surface_size(LogicalSize::new(
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
        attrs = attrs.with_min_surface_size(LogicalSize::new(
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
        use winit::platform::windows::WindowAttributesWindows;
        attrs = attrs.with_platform_attributes(Box::new(
            WindowAttributesWindows::default().with_skip_taskbar(true),
        ));
    }
    #[cfg(any(
        target_os = "linux",
        target_os = "dragonfly",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd"
    ))]
    if config.skip_taskbar {
        use winit::platform::x11::{WindowAttributesX11, WindowType};
        // Wayland ignores X11 window types. On X11, utility windows stay off the taskbar.
        attrs = attrs.with_platform_attributes(Box::new(
            WindowAttributesX11::default().with_x11_window_type(vec![WindowType::Utility]),
        ));
    }
    #[cfg(target_arch = "wasm32")]
    {
        attrs = attrs.with_platform_attributes(Box::new(web_canvas()));
    }
    attrs
}

/// Draw into the page's `<canvas id="rndr">` when there is one, otherwise
/// append a new canvas to `<body>`.
#[cfg(target_arch = "wasm32")]
fn web_canvas() -> winit::platform::web::WindowAttributesWeb {
    use wasm_bindgen::JsCast;
    let canvas = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(WEB_CANVAS_ID))
        .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok());
    let append = canvas.is_none();
    winit::platform::web::WindowAttributesWeb::default()
        .with_canvas(canvas)
        .with_append(append)
}

/// Element id of the canvas a browser page can provide for rndr to draw into.
#[cfg(target_arch = "wasm32")]
pub const WEB_CANVAS_ID: &str = "rndr";

pub(crate) fn finish_create(
    window: &dyn Window,
    event_loop: &dyn ActiveEventLoop,
    config: &WindowConfig,
) {
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
    #[cfg(target_os = "windows")]
    if config.kind == WindowKind::Panel {
        set_panel_exstyle(window, true);
        raise_topmost(window);
    }
    place(window, event_loop, config);
}

pub(crate) fn apply_effect(window: &dyn Window, panel: bool, effect: WindowEffect) {
    match effect {
        WindowEffect::Show => show(window, panel),
        WindowEffect::Hide => window.set_visible(false),
        WindowEffect::Focus => focus(window, panel),
    }
}

fn show(window: &dyn Window, panel: bool) {
    #[cfg(target_os = "macos")]
    if panel {
        show_panel(window);
        return;
    }
    #[cfg(target_os = "windows")]
    if panel {
        show_panel_win32(window);
        return;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = panel;
    window.set_visible(true);
}

fn focus(window: &dyn Window, panel: bool) {
    #[cfg(target_os = "macos")]
    if panel {
        focus_panel(window);
        return;
    }
    #[cfg(target_os = "windows")]
    if panel {
        focus_panel_win32(window);
        return;
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let _ = panel;
    window.focus_window();
}

fn place(window: &dyn Window, event_loop: &dyn ActiveEventLoop, config: &WindowConfig) {
    if session_is_wayland(event_loop) {
        return;
    }
    match config.position {
        Position::Platform => {}
        Position::At { x, y } => {
            window.set_outer_position(winit::dpi::Position::Logical(LogicalPosition::new(
                f64::from(x),
                f64::from(y),
            )));
        }
        Position::Center => center(window),
    }
}

fn center(window: &dyn Window) {
    let Some(monitor) = window
        .current_monitor()
        .or_else(|| window.primary_monitor())
    else {
        return;
    };
    let Some(screen) = monitor.current_video_mode().map(|mode| mode.size()) else {
        return;
    };
    let outer = window.outer_size();
    let x = (screen.width as i32 - outer.width as i32) / 2;
    let y = (screen.height as i32 - outer.height as i32) / 2;
    window.set_outer_position(winit::dpi::Position::Physical(PhysicalPosition::new(
        x.max(0),
        y.max(0),
    )));
}

pub(crate) fn session_is_wayland(event_loop: &dyn ActiveEventLoop) -> bool {
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
fn show_panel(window: &dyn Window) {
    if let Some(ns) = ns_window(window) {
        // Order in front without activating the app. `set_visible(true)` would
        // call `makeKeyAndOrderFront` and take key focus as a side effect.
        unsafe { ns.orderFrontRegardless() };
    } else {
        window.set_visible(true);
    }
}

#[cfg(target_os = "macos")]
fn focus_panel(window: &dyn Window) {
    if let Some(ns) = ns_window(window) {
        // `Window::focus_window` calls `activateIgnoringOtherApps`. A panel
        // only becomes key.
        unsafe { ns.makeKeyWindow() };
    } else {
        window.focus_window();
    }
}

#[cfg(target_os = "macos")]
fn configure_ns_panel(window: &dyn Window) {
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

#[cfg(target_os = "windows")]
fn hwnd(window: &dyn Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(win) = handle.as_raw() else {
        return None;
    };
    Some(win.hwnd.get() as windows_sys::Win32::Foundation::HWND)
}

#[cfg(target_os = "windows")]
fn set_panel_exstyle(window: &dyn Window, noactivate: bool) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
    };
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    let bit = WS_EX_NOACTIVATE as isize;
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let updated = if noactivate {
            style | bit
        } else {
            style & !bit
        };
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, updated);
    }
}

#[cfg(target_os = "windows")]
fn raise_topmost(window: &dyn Window) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    };
    let Some(hwnd) = hwnd(window) else {
        return;
    };
    unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(target_os = "windows")]
fn show_panel_win32(window: &dyn Window) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
    set_panel_exstyle(window, true);
    raise_topmost(window);
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
    } else {
        window.set_visible(true);
    }
}

#[cfg(target_os = "windows")]
fn focus_panel_win32(window: &dyn Window) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
    // Windows delivers keys to the foreground window. Clear the no-activate
    // bit for this explicit focus. The next show sets it again.
    set_panel_exstyle(window, false);
    raise_topmost(window);
    if let Some(hwnd) = hwnd(window) {
        unsafe {
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
        }
    } else {
        window.focus_window();
    }
}

#[cfg(all(test, target_os = "windows"))]
#[test]
fn windows_panel_constants_match_win32() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
        SW_SHOWNOACTIVATE, WS_EX_NOACTIVATE,
    };
    assert_eq!(rndr_core::WIN32_EX_NOACTIVATE, WS_EX_NOACTIVATE);
    assert_eq!(rndr_core::WIN32_GWL_EXSTYLE, GWL_EXSTYLE);
    assert_eq!(rndr_core::WIN32_SW_SHOWNOACTIVATE, SW_SHOWNOACTIVATE);
    assert_eq!(rndr_core::WIN32_HWND_TOPMOST, HWND_TOPMOST as isize);
    assert_eq!(rndr_core::WIN32_SWP_NOSIZE, SWP_NOSIZE);
    assert_eq!(rndr_core::WIN32_SWP_NOMOVE, SWP_NOMOVE);
    assert_eq!(rndr_core::WIN32_SWP_NOACTIVATE, SWP_NOACTIVATE);
    assert_eq!(rndr_core::WIN32_SWP_SHOWWINDOW, SWP_SHOWWINDOW);
}

#[cfg(target_os = "macos")]
fn ns_window(window: &dyn Window) -> Option<objc2::rc::Retained<objc2_app_kit::NSWindow>> {
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
