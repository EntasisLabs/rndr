//! X11 WebKitGTK smoke. Wayland is not exercised here.
//!
//! ```text
//! cargo run -p rndr_browser --example webkitgtk_smoke
//! ```
//!
//! The parent is an X11 window, the same handle shape winit 0.31 reports.
//! `GDK_BACKEND=x11` is set by the backend before GTK starts. A missing
//! display is [`BrowserError::Prerequisite`]. This file does not evaluate
//! script and does not ignore TLS errors.

fn main() {
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("the WebKitGTK smoke runs on Linux");
        std::process::exit(2);
    }
    #[cfg(target_os = "linux")]
    linux::run();
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::c_void;
    use std::os::raw::{c_uint, c_ulong};
    use std::path::PathBuf;
    use std::time::Duration;

    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, WindowHandle, XlibWindowHandle,
    };
    use rndr_browser::{Browser, BrowserEvent, ClientRect, LoadOutcome};
    use x11::xlib::{
        ButtonPress, ButtonPressMask, Display, IsUnmapped, XButtonEvent, XCloseDisplay,
        XCreateSimpleWindow, XDefaultScreen, XDestroyWindow, XEvent, XFlush, XFree, XGetGeometry,
        XGetWindowAttributes, XMapWindow, XOpenDisplay, XQueryTree, XRootWindow, XSendEvent, XSync,
        XWindowAttributes,
    };

    struct Parent {
        window: c_ulong,
    }

    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw = RawWindowHandle::Xlib(XlibWindowHandle::new(self.window));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    pub(super) fn run() {
        println!(
            "display DISPLAY={} WAYLAND_DISPLAY={}",
            std::env::var("DISPLAY").unwrap_or_else(|_| "<unset>".into()),
            std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "<unset>".into())
        );
        let display = unsafe { XOpenDisplay(std::ptr::null()) };
        assert!(!display.is_null(), "XOpenDisplay failed");
        let parent = unsafe {
            let screen = XDefaultScreen(display);
            let root = XRootWindow(display, screen);
            let window = XCreateSimpleWindow(display, root, 40, 40, 900, 700, 0, 0, 0);
            XMapWindow(display, window);
            XFlush(display);
            window
        };

        let mut browser = match Browser::attach(&Parent { window: parent }) {
            Ok(browser) => browser,
            Err(error) => panic!("attach failed: {error}"),
        };
        browser.show();
        browser
            .set_bounds(ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        let first = child_geometry(display, parent).expect("child window");
        println!("bounds {first:?}");
        assert_eq!(first.0, 10);
        assert_eq!(first.1, 20);
        assert_eq!(first.2, 300);
        assert_eq!(first.3, 200);

        browser
            .set_bounds(ClientRect::try_new(12.0, 24.0, 360.0, 180.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        let resized = child_geometry(display, parent).expect("resized child");
        println!("resized {resized:?}");
        assert_eq!(resized, (12, 24, 360, 180));

        browser
            .set_bounds(ClientRect::try_new(12.0, 24.0, 0.0, 180.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        let hidden = child_window(display, parent).expect("child remains");
        let mut attributes = unsafe { std::mem::zeroed::<XWindowAttributes>() };
        let status = unsafe { XGetWindowAttributes(display, hidden, &mut attributes) };
        println!("zero-area map_state {}", attributes.map_state);
        assert_ne!(status, 0);
        assert_eq!(attributes.map_state, IsUnmapped);

        browser
            .set_bounds(ClientRect::try_new(10.0, 20.0, 640.0, 480.0).unwrap())
            .unwrap();
        let _ = browser.frame();

        let port = serve_basic();
        let basic_url = format!("http://127.0.0.1:{port}/");
        let basic = load(&mut browser, &basic_url);
        println!("basic auth {basic_url} -> {basic:?}");
        assert!(matches!(
            basic,
            Some(LoadOutcome::Failed | LoadOutcome::Stopped)
        ));

        let (download_port, dir) = serve_download();
        let download_url = format!("http://127.0.0.1:{download_port}/rndr-browser.txt");
        let refused = dir.join("refused.txt");
        let accepted = dir.join("accepted.txt");
        let outcome = download(&mut browser, &download_url, &refused, &accepted);
        println!("download {download_url} -> {outcome}");
        assert_eq!(outcome, "accepted");
        assert!(!refused.exists());

        let page = dir.join("file-input.html");
        std::fs::write(
            &page,
            r#"<!doctype html><input type="file" style="position:fixed;inset:0;width:100%;height:100%">"#,
        )
        .unwrap();
        let file_url = format!("file://{}", page.display());
        let chooser = file_input(&mut browser, display, parent, &file_url);
        println!("file input {file_url} -> {chooser}");

        let valid = load(&mut browser, "https://example.com/");
        println!("valid https://example.com/ -> {valid:?}");
        assert_eq!(valid, Some(LoadOutcome::Finished));

        let expired = load(&mut browser, "https://expired.badssl.com/");
        println!("expired https://expired.badssl.com/ -> {expired:?}");
        assert_eq!(expired, Some(LoadOutcome::Failed));

        drop(browser);
        unsafe {
            XDestroyWindow(display, parent);
            XCloseDisplay(display);
        }
        println!("webkitgtk x11 smoke ok");
    }

    fn load(browser: &mut Browser, url: &str) -> Option<LoadOutcome> {
        browser.navigate(url).expect("navigate");
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("file://")
            .trim_end_matches('/');
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        while std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
            for event in browser.frame() {
                let BrowserEvent::Navigation(state) = event else {
                    continue;
                };
                println!("  saw {} {:?}", state.url, state.load);
                let terminal = matches!(
                    state.load,
                    LoadOutcome::Finished | LoadOutcome::Failed | LoadOutcome::Stopped
                );
                if terminal && state.url.contains(host) {
                    println!("  {url} saw {} {:?}", state.url, state.load);
                    return Some(state.load);
                }
            }
        }
        None
    }

    fn download(
        browser: &mut Browser,
        url: &str,
        refused: &std::path::Path,
        accepted: &std::path::Path,
    ) -> &'static str {
        browser.navigate(url).expect("navigate");
        let Some(token) = wait_download(browser) else {
            return "no-intent";
        };
        browser.cancel_download(token).expect("cancel");
        let _ = browser.frame();
        if refused.exists() {
            return "cancel-wrote";
        }
        browser.navigate(url).expect("navigate again");
        let Some(token) = wait_download(browser) else {
            return "no-second-intent";
        };
        browser.accept_download(token, accepted).expect("accept");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            let _ = browser.frame();
            if accepted.is_file()
                && std::fs::metadata(accepted)
                    .map(|meta| meta.len() > 0)
                    .unwrap_or(false)
            {
                return "accepted";
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        "accept-empty"
    }

    fn wait_download(browser: &mut Browser) -> Option<rndr_browser::IntentToken> {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            for event in browser.frame() {
                if let BrowserEvent::Download(intent) = event {
                    println!("  download intent {} {}", intent.suggested_name, intent.url);
                    return Some(intent.token);
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    fn file_input(
        browser: &mut Browser,
        display: *mut Display,
        parent: c_ulong,
        url: &str,
    ) -> &'static str {
        let _ = load(browser, url);
        let Some(child) = child_window(display, parent) else {
            return "no-child";
        };
        let mut button = XButtonEvent {
            type_: ButtonPress,
            serial: 0,
            send_event: 1,
            display,
            window: child,
            root: 0,
            subwindow: 0,
            time: 0,
            x: 30,
            y: 30,
            x_root: 0,
            y_root: 0,
            state: 0,
            button: 1,
            same_screen: 1,
        };
        unsafe {
            XSendEvent(
                display,
                child,
                1,
                ButtonPressMask,
                &mut button as *mut XButtonEvent as *mut XEvent,
            );
            XFlush(display);
            XSync(display, 0);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            for event in browser.frame() {
                if let BrowserEvent::FileInput(intent) = event {
                    println!("  file input multiple={}", intent.multiple);
                    browser
                        .cancel_file_input(intent.token)
                        .expect("cancel file");
                    return "cancelled";
                }
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        "not-observed"
    }

    fn child_geometry(display: *mut Display, parent: c_ulong) -> Option<(i32, i32, u32, u32)> {
        let child = child_window(display, parent)?;
        unsafe {
            let mut root = 0;
            let mut x = 0;
            let mut y = 0;
            let mut width = 0;
            let mut height = 0;
            let mut border = 0;
            let mut depth = 0;
            if XGetGeometry(
                display,
                child,
                &mut root,
                &mut x,
                &mut y,
                &mut width,
                &mut height,
                &mut border,
                &mut depth,
            ) == 0
            {
                return None;
            }
            Some((x, y, width, height))
        }
    }

    fn child_window(display: *mut Display, parent: c_ulong) -> Option<c_ulong> {
        unsafe {
            let mut root = 0;
            let mut parent_return = 0;
            let mut children: *mut c_ulong = std::ptr::null_mut();
            let mut count: c_uint = 0;
            if XQueryTree(
                display,
                parent,
                &mut root,
                &mut parent_return,
                &mut children,
                &mut count,
            ) == 0
                || children.is_null()
                || count == 0
            {
                return None;
            }
            let window = *children;
            XFree(children as *mut c_void);
            Some(window)
        }
    }

    fn serve_basic() -> u16 {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let body = b"authentication required";
            let header = format!(
                "HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"rndr\"\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = stream.set_read_timeout(Some(std::time::Duration::from_millis(300)));
                let read = std::io::Read::read(&mut stream, &mut [0_u8; 4096]);
                println!("basic server read {read:?}");
                let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
                let _ = std::io::Write::write_all(&mut stream, body);
            }
        });
        port
    }

    fn serve_download() -> (u16, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rndr-browser-gtk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            let body = b"hello";
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=\"rndr-browser.txt\"\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                println!("download server accepted");
                let _ = std::io::Read::read(&mut stream, &mut [0_u8; 4096]);
                let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
                let _ = std::io::Write::write_all(&mut stream, body);
            }
        });
        (port, dir)
    }
}
