//! Windows WebView2 smoke. This process is the UI thread.
//!
//! ```text
//! cargo run -p rndr_browser --example windows_smoke
//! ```
//!
//! A missing runtime exits with [`BrowserError::RuntimeMissing`] and does not
//! download one. The checks are bounds, a resize, a valid HTTPS load, an
//! expired certificate, a refused basic challenge, and a host-accepted
//! download. File input is not triggered here: opening it needs a user
//! gesture, and this example does not evaluate script.

fn main() {
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("the WebView2 smoke runs on Windows");
        std::process::exit(2);
    }
    #[cfg(target_os = "windows")]
    windows::run();
}

#[cfg(target_os = "windows")]
mod windows {
    use std::ffi::c_void;
    use std::num::NonZeroIsize;
    use std::path::PathBuf;
    use std::time::Duration;

    use raw_window_handle::{
        HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
    };
    use rndr_browser::{Browser, BrowserError, BrowserEvent, ClientRect, LoadOutcome};
    use windows::core::w;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::GetDpiForWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumChildWindows,
        GetClientRect, GetWindowRect, IsWindow, IsWindowVisible, PeekMessageW, RegisterClassW,
        ShowWindow, TranslateMessage, CW_USEDEFAULT, MSG, PM_REMOVE, SW_SHOW, WINDOW_EX_STYLE,
        WNDCLASSW, WS_OVERLAPPEDWINDOW,
    };

    struct Parent {
        hwnd: *mut c_void,
    }

    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let hwnd = NonZeroIsize::new(self.hwnd as isize).ok_or(HandleError::Unavailable)?;
            let raw = RawWindowHandle::Win32(Win32WindowHandle::new(hwnd));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }

    pub(super) fn run() {
        let hwnd = create_window();
        let mut browser = match Browser::attach(&Parent { hwnd: hwnd.0 }) {
            Ok(browser) => browser,
            Err(BrowserError::RuntimeMissing { code }) => {
                eprintln!("WebView2 runtime is missing (0x{code:08X}); nothing was downloaded");
                std::process::exit(3);
            }
            Err(error) => panic!("attach failed: {error}"),
        };
        browser.show();
        browser
            .set_bounds(ClientRect::try_new(10.0, 20.0, 300.0, 200.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        pump(Duration::from_millis(200));
        let first = child_client_rect(hwnd).expect("the view should have a child window");
        println!("bounds {first:?}");
        assert_eq!(
            first,
            expected_bounds(hwnd, 10.0, 20.0, 300.0, 200.0),
            "logical rect in parent-client pixels"
        );

        browser
            .set_bounds(ClientRect::try_new(12.0, 24.0, 360.0, 180.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        pump(Duration::from_millis(200));
        let resized = child_client_rect(hwnd).expect("resize keeps the child");
        println!("resized {resized:?}");
        assert_eq!(resized, expected_bounds(hwnd, 12.0, 24.0, 360.0, 180.0));

        browser
            .set_bounds(ClientRect::try_new(12.0, 24.0, 0.0, 180.0).unwrap())
            .unwrap();
        let _ = browser.frame();
        pump(Duration::from_millis(100));
        assert!(
            child_hwnd(hwnd).is_some_and(|child| unsafe { !IsWindowVisible(child).as_bool() }),
            "zero area hides the view and keeps it"
        );

        browser
            .set_bounds(ClientRect::try_new(10.0, 20.0, 640.0, 480.0).unwrap())
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
            "an expired certificate must fail the load"
        );

        let port = serve_basic();
        let basic_url = format!("http://127.0.0.1:{port}/");
        let basic = load(&mut browser, &basic_url);
        println!("basic auth {basic_url} -> {basic:?}");
        assert!(
            matches!(basic, Some(LoadOutcome::Failed | LoadOutcome::Stopped)),
            "a credential challenge must be refused"
        );

        let (download_port, dir) = serve_download();
        let download_url = format!("http://127.0.0.1:{download_port}/rndr-browser.txt");
        let refused = dir.join("refused.txt");
        let accepted = dir.join("accepted.txt");
        let outcome = download(&mut browser, &download_url, &refused, &accepted);
        println!("download {download_url} -> {outcome}");
        assert_eq!(outcome, "accepted");
        assert!(!refused.exists(), "a cancelled download writes nothing");
        let bytes = std::fs::read(&accepted).expect("accepted download");
        assert!(!bytes.is_empty(), "the host destination receives the bytes");

        drop(browser);
        pump(Duration::from_millis(200));
        assert!(
            unsafe { IsWindow(Some(hwnd)).as_bool() },
            "the parent window outlives the view"
        );
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        println!("windows smoke ok");
    }

    fn load(browser: &mut Browser, url: &str) -> Option<LoadOutcome> {
        browser.navigate(url).expect("navigate");
        let host = url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/');
        let deadline = std::time::Instant::now() + Duration::from_secs(45);
        while std::time::Instant::now() < deadline {
            pump(Duration::from_millis(50));
            for event in browser.frame() {
                let BrowserEvent::Navigation(state) = event else {
                    continue;
                };
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
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut token = None;
        while std::time::Instant::now() < deadline && token.is_none() {
            pump(Duration::from_millis(50));
            for event in browser.frame() {
                if let BrowserEvent::Download(intent) = event {
                    println!("  download intent {} {}", intent.suggested_name, intent.url);
                    token = Some(intent.token);
                }
            }
        }
        let Some(token) = token else {
            return "no-intent";
        };
        browser.cancel_download(token).expect("cancel");
        pump(Duration::from_millis(300));
        if refused.exists() {
            return "cancel-wrote";
        }
        browser.navigate(url).expect("navigate again");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        let mut token = None;
        while std::time::Instant::now() < deadline && token.is_none() {
            pump(Duration::from_millis(50));
            for event in browser.frame() {
                if let BrowserEvent::Download(intent) = event {
                    token = Some(intent.token);
                }
            }
        }
        let Some(token) = token else {
            return "no-second-intent";
        };
        browser.accept_download(token, accepted).expect("accept");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            pump(Duration::from_millis(50));
            let _ = browser.frame();
            if accepted.is_file()
                && std::fs::metadata(accepted)
                    .map(|meta| meta.len() > 0)
                    .unwrap_or(false)
            {
                return "accepted";
            }
        }
        "accept-empty"
    }

    fn pump(slice: Duration) {
        let deadline = std::time::Instant::now() + slice;
        while std::time::Instant::now() < deadline {
            let mut message = MSG::default();
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() } {
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn create_window() -> HWND {
        unsafe extern "system" fn proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        unsafe {
            let class = WNDCLASSW {
                lpfnWndProc: Some(proc),
                lpszClassName: w!("RndrBrowserSmoke"),
                hInstance: GetModuleHandleW(None).unwrap_or_default().into(),
                ..Default::default()
            };
            let _ = RegisterClassW(&class);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("RndrBrowserSmoke"),
                w!("rndr-browser"),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                900,
                700,
                None,
                None,
                None,
                None,
            )
            .expect("window");
            let _ = ShowWindow(hwnd, SW_SHOW);
            hwnd
        }
    }

    fn child_hwnd(parent: HWND) -> Option<HWND> {
        let mut found = None;
        unsafe {
            let _ = EnumChildWindows(
                Some(parent),
                Some(enum_child),
                LPARAM(&mut found as *mut Option<HWND> as isize),
            );
        }
        found
    }

    unsafe extern "system" fn enum_child(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        let found = unsafe { &mut *(param.0 as *mut Option<HWND>) };
        if found.is_none() {
            *found = Some(hwnd);
        }
        windows::core::BOOL(1)
    }

    fn expected_bounds(
        parent: HWND,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> (i32, i32, i32, i32) {
        let dpi = unsafe { GetDpiForWindow(parent) };
        let px = |logical: f32| {
            let dpi = if dpi == 0 { 96 } else { dpi };
            (f64::from(logical) * f64::from(dpi) / 96.0).round() as i32
        };
        let left = px(x);
        let top = px(y);
        (left, top, left + px(width), top + px(height))
    }

    fn child_client_rect(parent: HWND) -> Option<(i32, i32, i32, i32)> {
        let child = child_hwnd(parent)?;
        unsafe {
            let mut window = RECT::default();
            GetWindowRect(child, &mut window).ok()?;
            let mut origin = POINT {
                x: window.left,
                y: window.top,
            };
            let mut corner = POINT {
                x: window.right,
                y: window.bottom,
            };
            let _ = ScreenToClient(parent, &mut origin);
            let _ = ScreenToClient(parent, &mut corner);
            let _ = GetClientRect(parent, &mut RECT::default());
            Some((origin.x, origin.y, corner.x, corner.y))
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
                let _ = std::io::Read::read(&mut stream, &mut [0_u8; 4096]);
                let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
                let _ = std::io::Write::write_all(&mut stream, body);
            }
        });
        port
    }

    fn serve_download() -> (u16, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rndr-browser-smoke-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("download dir");
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
                let _ = std::io::Read::read(&mut stream, &mut [0_u8; 4096]);
                let _ = std::io::Write::write_all(&mut stream, header.as_bytes());
                let _ = std::io::Write::write_all(&mut stream, body);
            }
        });
        (port, dir)
    }
}
