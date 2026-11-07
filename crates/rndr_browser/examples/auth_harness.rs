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
//! fail the load. A local HTTP basic challenge must be refused. The process
//! must not abort in the authentication callback. Debug and release are
//! separate runs. This file does not disable certificate validation and does
//! not run on Linux.

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

        let port = serve_basic_auth();
        let credential_url = format!("http://127.0.0.1:{port}/");
        let refused = load(&mut browser, &credential_url);
        println!("basic auth {credential_url} -> {refused:?}");
        assert!(
            matches!(refused, Some(LoadOutcome::Failed | LoadOutcome::Stopped)),
            "a credential challenge must be refused and must not abort the process"
        );
    }

    fn load(browser: &mut Browser, url: &str) -> Option<LoadOutcome> {
        browser.navigate(url).expect("navigate");
        let host = host_of(url);
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while std::time::Instant::now() < deadline {
            let until = unsafe { NSDate::dateWithTimeIntervalSinceNow(0.1) };
            unsafe { NSRunLoop::currentRunLoop().runUntilDate(&until) };
            for event in browser.frame() {
                let BrowserEvent::Navigation(state) = event else {
                    continue;
                };
                let terminal = matches!(
                    state.load,
                    LoadOutcome::Finished | LoadOutcome::Failed | LoadOutcome::Stopped
                );
                if !terminal {
                    continue;
                }
                println!("  {url} saw {} {:?}", state.url, state.load);
                if state.url.contains(host) {
                    return Some(state.load);
                }
            }
        }
        None
    }

    /// One HTTP basic challenge on localhost. The handler must cancel it.
    fn serve_basic_auth() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("local address").port();
        std::thread::spawn(move || {
            let response = b"HTTP/1.0 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"rndr\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else {
                    continue;
                };
                let _ = std::io::Read::read(&mut stream, &mut [0_u8; 2048]);
                let _ = std::io::Write::write_all(&mut stream, response);
            }
        });
        port
    }

    fn host_of(url: &str) -> &str {
        url.trim_start_matches("https://").trim_end_matches('/')
    }
}
