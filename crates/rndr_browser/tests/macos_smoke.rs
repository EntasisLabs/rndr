//! Native WKWebView smoke. Ignored: `cargo test` does not run on the AppKit
//! main thread, and this change has no native macOS session.
//!
//! A passing headless run does not make the matrix cell Supported. The steps
//! are the ones in `crates/rndr_browser/README.md`.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2_app_kit::{NSApplication, NSBackingStoreType, NSView, NSWindow, NSWindowStyleMask};
use objc2_foundation::{
    MainThreadMarker, NSArray, NSDate, NSPoint, NSRect, NSRunLoop, NSSize, NSString,
};
use raw_window_handle::{
    AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
};
use rndr_browser::{Browser, BrowserEvent, ClientRect, LoadOutcome};

struct ContentParent {
    view: *mut c_void,
}

impl HasWindowHandle for ContentParent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        let view = NonNull::new(self.view).ok_or(HandleError::Unavailable)?;
        let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
        // Safety: `view` is the content view of `window`, which outlives this
        // handle. `attach` retains it and does not keep the raw handle.
        Ok(unsafe { WindowHandle::borrow_raw(raw) })
    }
}

#[test]
#[ignore = "native macOS Aqua session on the main thread; not run for R3.1"]
fn wkwebview_navigation_bounds_and_hide() {
    let Some(mtm) = MainThreadMarker::new() else {
        panic!(
            "WKWebView smoke must run on the AppKit main thread. \
             crates/rndr_browser/README.md describes the session. \
             cargo test does not provide that thread, so this is not a certification."
        );
    };

    let _app = NSApplication::sharedApplication(mtm);
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            mtm.alloc(),
            NSRect::new(NSPoint::new(80.0, 80.0), NSSize::new(800.0, 600.0)),
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable,
            NSBackingStoreType::NSBackingStoreBuffered,
            false,
        )
    };
    window.setTitle(&NSString::from_str("rndr browser smoke"));
    window.makeKeyAndOrderFront(None);
    let content = window.contentView().expect("content view");
    let parent = ContentParent {
        view: Retained::as_ptr(&content).cast_mut().cast(),
    };
    let mut browser = Browser::attach(&parent).expect("WKWebView");
    assert_eq!(subview_count(&content), 1);

    browser.show();
    browser
        .set_bounds(ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap())
        .unwrap();
    assert!(browser.frame().is_empty());
    assert_eq!(
        browser.applied_bounds(),
        Some(ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap())
    );
    assert!(browser.surface_visible());
    assert_frame(&content, 10.0, 20.0, 300.0, 200.0, false);

    browser
        .navigate("data:text/html,<title>rndr-browser</title>ok")
        .unwrap();
    let mut outcome = None;
    for _ in 0..40 {
        let until = unsafe { NSDate::dateWithTimeIntervalSinceNow(0.05) };
        unsafe { NSRunLoop::currentRunLoop().runUntilDate(&until) };
        for event in browser.frame() {
            if let BrowserEvent::Navigation(state) = event {
                if matches!(state.load, LoadOutcome::Finished | LoadOutcome::Failed) {
                    outcome = Some(state);
                }
            }
        }
        if outcome.is_some() {
            break;
        }
    }
    let outcome = outcome.expect("load outcome");
    assert!(!outcome.url.is_empty() || outcome.load == LoadOutcome::Failed);

    browser
        .set_bounds(ClientRect::try_new(12.0, 24.0, 280.0, 160.0).unwrap())
        .unwrap();
    let _ = browser.frame();
    assert_frame(&content, 12.0, 24.0, 280.0, 160.0, false);

    browser
        .set_bounds(ClientRect::try_new(12.0, 24.0, 0.0, 160.0).unwrap())
        .unwrap();
    let _ = browser.frame();
    assert!(!browser.surface_visible());
    assert_eq!(subview_count(&content), 1);
    assert_frame(&content, 12.0, 24.0, 0.0, 160.0, true);

    drop(browser);
    assert_eq!(subview_count(&content), 0);
}

fn subview_count(content: &NSView) -> usize {
    unsafe { content.subviews() }.count()
}

fn assert_frame(content: &NSView, x: f64, y: f64, width: f64, height: f64, hidden: bool) {
    let subviews: Retained<NSArray<NSView>> = unsafe { content.subviews() };
    assert_eq!(subviews.count(), 1);
    let view = unsafe { subviews.objectAtIndex(0) };
    assert_eq!(unsafe { view.isHidden() }, hidden);
    let frame = view.frame();
    let origin_y = content.bounds().size.height - y - height;
    assert!((frame.origin.x - x).abs() < 0.5, "x {}", frame.origin.x);
    assert!(
        (frame.origin.y - origin_y).abs() < 0.5,
        "y {}",
        frame.origin.y
    );
    assert!((frame.size.width - width).abs() < 0.5);
    assert!((frame.size.height - height).abs() < 0.5);
}
