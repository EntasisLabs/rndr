//! Release linkage + live ordinary Browser/default artifact refusal regression.
//! Run with default features, then explicitly opt in to private artifacts.
#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("Mach-O/native probe requires macOS");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
fn main() {
    use objc2::rc::Retained;
    use objc2_app_kit::NSView;
    use objc2_foundation::{MainThreadMarker, NSDate, NSPoint, NSRect, NSRunLoop, NSSize};
    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    };
    use rndr_browser::{
        ArtifactError, ArtifactPreview, ArtifactState, Browser, BrowserEvent, LoadOutcome,
    };
    use std::{
        ffi::c_void,
        io::{Read, Write},
        net::TcpListener,
        ptr::NonNull,
        time::{Duration, Instant},
    };
    struct Parent(*mut c_void);
    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw =
                RawWindowHandle::AppKit(AppKitWindowHandle::new(NonNull::new(self.0).unwrap()));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }
    fn pump() {
        unsafe {
            NSRunLoop::mainRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.01));
        }
    }
    let marker = MainThreadMarker::new().expect("main thread");
    let view = unsafe {
        NSView::initWithFrame(
            marker.alloc(),
            NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(640.0, 480.0)),
        )
    };
    let parent = Parent(Retained::as_ptr(&view).cast_mut().cast());
    // Call artifact APIs in BOTH release executables; do not let dead stripping
    // of an unused library masquerade as a successful compile-time gate.
    match ArtifactPreview::attach(
        &parent,
        "<button onclick='this.textContent=42'>local</button>",
    ) {
        Ok(mut artifact) if cfg!(feature = "macos-private-webkit-artifacts") => {
            artifact.replace("<p>fresh release probe</p>").unwrap();
            let deadline = Instant::now() + Duration::from_secs(20);
            while artifact.frame().unwrap() != ArtifactState::Ready {
                assert!(Instant::now() < deadline);
                pump();
            }
            println!("PASS opted-in artifact attach/replace/ready/drop");
        }
        Err(ArtifactError::Unsupported) if !cfg!(feature = "macos-private-webkit-artifacts") => {
            println!("PASS default artifact typed Unsupported (no native child)");
            assert!(unsafe { view.subviews() }.is_empty());
        }
        Ok(_) => panic!("feature-off must not construct an artifact"),
        Err(error) => panic!("unexpected artifact outcome: {error}"),
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let done = stop.clone();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done.load(std::sync::atomic::Ordering::SeqCst) {
            if let Ok((mut stream, _)) = listener.accept() {
                stream
                    .set_read_timeout(Some(Duration::from_millis(100)))
                    .unwrap();
                let mut request = [0; 4096];
                let _ = stream.read(&mut request);
                let body =
                    "<title>ordinary-ok</title><script>document.title='ordinary-js-ok'</script>";
                let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body);
            }
            assert!(Instant::now() < deadline, "ordinary HTTP request missing");
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let mut browser = Browser::attach(&parent).unwrap();
    browser
        .navigate(&format!("http://{address}/ordinary"))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let events = browser.frame();
        if !events.is_empty() {
            eprintln!("ordinary events: {events:?}");
        }
        if events.iter().any(|event| matches!(event, BrowserEvent::Navigation(state) if state.load == LoadOutcome::Finished && state.title == "ordinary-js-ok")) { break; }
        assert!(Instant::now() < deadline, "ordinary URL + JS failed");
        pump();
    }
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    server.join().unwrap();
    println!("PASS ordinary Browser HTTP navigation + JavaScript");
}
