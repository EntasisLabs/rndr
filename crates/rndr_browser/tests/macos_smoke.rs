//! Native WKWebView smoke. Ignored: `cargo test` does not run on the AppKit
//! main thread, and this change has no native macOS session.
//!
//! The placement check uses a real winit window, whose content view is
//! flipped, and a separate unflipped `NSView`. Expected origins come from
//! `isFlipped` plus the top-left contract, not from a single hard-coded
//! `height - y - height` formula. See `crates/rndr_browser/README.md`.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2_app_kit::NSView;
use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize};
use raw_window_handle::{
    AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
};
use rndr_browser::{Browser, ClientRect};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{WindowAttributes, WindowId};

struct WinitParent<'a>(&'a dyn winit::window::Window);

impl HasWindowHandle for WinitParent<'_> {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        self.0.window_handle()
    }
}

struct ContentParent {
    view: *mut c_void,
}

impl HasWindowHandle for ContentParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = NonNull::new(self.view).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // Safety: `view` is a live NSView for this call. `attach` retains it
        // and does not keep the raw handle.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

struct Placement;

impl ApplicationHandler for Placement {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        place_in_winit_window(event_loop);
        place_in_unflipped_view();
        event_loop.exit();
    }

    fn window_event(
        &mut self,
        _event_loop: &dyn ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }
}

#[test]
#[ignore = "native macOS Aqua session on the main thread; not run for R3.1"]
fn winit_and_unflipped_parents_keep_the_top_left() {
    let Some(_marker) = MainThreadMarker::new() else {
        panic!(
            "WKWebView smoke must run on the AppKit main thread. \
             crates/rndr_browser/README.md describes the session. \
             cargo test does not provide that thread, so this is not a certification."
        );
    };
    let event_loop = EventLoop::new().expect("winit event loop");
    event_loop.run_app(Placement).expect("winit smoke");
}

fn requested() -> ClientRect {
    ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap()
}

fn place_in_winit_window(event_loop: &dyn ActiveEventLoop) {
    let window = event_loop
        .create_window(
            WindowAttributes::default()
                .with_surface_size(LogicalSize::new(800.0, 600.0))
                .with_title("rndr browser smoke"),
        )
        .expect("winit window");
    let mut browser = Browser::attach(&WinitParent(&*window)).expect("WKWebView in the winit view");
    browser.show();
    browser.set_bounds(requested()).unwrap();
    let _ = browser.frame();
    let content = content_view(&*window);
    assert!(
        content.isFlipped(),
        "this fixture is a winit content view, which is top-left"
    );
    assert_top_left(&content, 20.0);

    if let Some(ns_window) = content.window() {
        ns_window.setContentSize(NSSize::new(800.0, 500.0));
    }
    browser.set_bounds(requested()).unwrap();
    let _ = browser.frame();
    let content = content_view(&*window);
    assert!(content.isFlipped());
    assert_top_left(&content, 20.0);
    drop(browser);
}

fn place_in_unflipped_view() {
    let marker = MainThreadMarker::new().expect("main thread");
    let parent = unsafe {
        NSView::initWithFrame(
            marker.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
        )
    };
    assert!(!parent.isFlipped());
    let mut browser = Browser::attach(&ContentParent {
        view: Retained::as_ptr(&parent).cast_mut().cast(),
    })
    .expect("WKWebView in an unflipped view");
    browser.show();
    browser.set_bounds(requested()).unwrap();
    let _ = browser.frame();
    assert_top_left(&parent, 380.0);

    unsafe {
        parent.setFrame(NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(800.0, 500.0),
        ));
    }
    browser.set_bounds(requested()).unwrap();
    let _ = browser.frame();
    assert!(!parent.isFlipped());
    assert_top_left(&parent, 280.0);
    drop(browser);
}

fn content_view(window: &dyn winit::window::Window) -> Retained<NSView> {
    let handle = window.window_handle().expect("window handle");
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        panic!("winit macOS window did not yield an AppKit view");
    };
    unsafe { Retained::<NSView>::retain(appkit.ns_view.as_ptr().cast()) }.expect("content view")
}

fn assert_top_left(parent: &NSView, expected_y: f64) {
    let subviews = unsafe { parent.subviews() };
    assert!(subviews.count() >= 1);
    let guest = unsafe { subviews.objectAtIndex(subviews.count() - 1) };
    let frame = guest.frame();
    assert!((frame.origin.x - 10.0).abs() < 0.5, "x {}", frame.origin.x);
    assert!(
        (frame.origin.y - expected_y).abs() < 0.5,
        "y {} flipped {}",
        frame.origin.y,
        parent.isFlipped()
    );
    assert!((frame.size.width - 300.0).abs() < 0.5);
    assert!((frame.size.height - 200.0).abs() < 0.5);
}
