//! Main-thread WebKit load of a valid certificate and an expired one.
//!
//! `cargo test` is not the AppKit main thread, so this is a separate process:
//!
//! ```text
//! cargo run -p rndr_browser --features native-auth-harness --example auth_harness
//! cargo run -p rndr_browser --features native-auth-harness --example auth_harness --release
//! ```
//!
//! A valid `https://example.com` must finish. `https://expired.badssl.com/` must
//! fail the load. The process must not abort in the authentication callback.
//! Debug and release are separate runs. This file does not disable certificate
//! validation and does not run on Linux.

fn main() {
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("the WebKit authentication harness runs on macOS");
        std::process::exit(2);
    }
    #[cfg(target_os = "macos")]
    macos::run();
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::c_void;
    use std::ptr::NonNull;
    use std::time::Duration;

    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use objc2_foundation::{MainThreadMarker, NSDate, NSPoint, NSRect, NSRunLoop, NSSize};
    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    };
    use rndr_browser::{Browser, BrowserEvent, ClientRect, LoadOutcome};

    struct Parent {
        view: *mut c_void,
    }

    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let view = NonNull::new(self.view).ok_or(HandleError::Unavailable)?;
            let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(view));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    pub(super) fn run() {
        let marker = MainThreadMarker::new().expect("the harness process is the main thread");
        let parent = unsafe {
            NSView::initWithFrame(
                marker.alloc(),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
            )
        };
        let mut browser = Browser::attach(&Parent {
            view: Retained::as_ptr(&parent).cast_mut().cast(),
        })
        .expect("attach");
        browser.show();
        browser
            .set_bounds(ClientRect::try_new(0.0, 0.0, 800.0, 600.0).unwrap())
            .unwrap();
        let _ = browser.frame();

        let valid = load(&mut browser, "https://example.com/");
        println!("valid https://example.com/ -> {valid:?}");
        assert_eq!(
            valid,
            Some(LoadOutcome::Finished),
            "valid HTTPS must finish"
        );

        let expired = load(&mut browser, "https://expired.badssl.com/");
        println!("expired https://expired.badssl.com/ -> {expired:?}");
        assert_eq!(
            expired,
            Some(LoadOutcome::Failed),
            "an expired certificate must fail the load and must not abort the process"
        );
    }

    fn load(browser: &mut Browser, url: &str) -> Option<LoadOutcome> {
        browser.navigate(url).expect("navigate");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let until = unsafe { NSDate::dateWithTimeIntervalSinceNow(0.1) };
            unsafe { NSRunLoop::currentRunLoop().runUntilDate(&until) };
            for event in browser.frame() {
                if let BrowserEvent::Navigation(state) = event {
                    if matches!(
                        state.load,
                        LoadOutcome::Finished | LoadOutcome::Failed | LoadOutcome::Stopped
                    ) && state.url.contains(host_of(url))
                    {
                        return Some(state.load);
                    }
                }
            }
        }
        None
    }

    fn host_of(url: &str) -> &str {
        url.trim_start_matches("https://").trim_end_matches('/')
    }
}
