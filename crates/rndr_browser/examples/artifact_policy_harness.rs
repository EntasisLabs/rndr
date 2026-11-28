//! Native policy probe. This executable installs a test-only observation handler
//! on the actual preview controller. The library has no guest-message channel.
//! Run in an Aqua main-thread process; it uses real WebKit, not a detached mock.

fn main() {
    #[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
    macos::run();
    #[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
    {
        eprintln!("artifact policy probe requires macOS");
        std::process::exit(2);
    }
}

#[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
mod macos {
    use objc2::mutability::MainThreadOnly;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{declare_class, msg_send, msg_send_id, ClassType, DeclaredClass};
    use objc2_app_kit::NSView;
    use objc2_foundation::{
        MainThreadMarker, NSDate, NSObject, NSObjectProtocol, NSPoint, NSRect, NSRunLoop, NSSize,
        NSString,
    };
    use objc2_web_kit::{
        WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKWebView,
        WKWebsiteDataStore,
    };
    use raw_window_handle::{
        AppKitWindowHandle, HandleError, HasWindowHandle, RawWindowHandle, WindowHandle,
    };
    use rndr_browser::{
        ArtifactError, ArtifactPreview, ArtifactState, Browser, BrowserEvent, ClientRect,
        LoadOutcome, OverlayBitmap, MAX_ARTIFACT_HTML_BYTES,
    };
    use std::cell::RefCell;
    use std::ffi::c_void;
    use std::io::{Read, Write};
    use std::net::{TcpListener, UdpSocket};
    use std::ptr::NonNull;
    use std::rc::Rc;
    use std::sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    };
    use std::time::{Duration, Instant};

    struct Parent(*mut c_void);
    impl HasWindowHandle for Parent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            let raw = RawWindowHandle::AppKit(AppKitWindowHandle::new(
                NonNull::new(self.0).ok_or(HandleError::Unavailable)?,
            ));
            Ok(unsafe { WindowHandle::borrow_raw(raw) })
        }
    }
    struct Ivars {
        reports: Rc<RefCell<Vec<String>>>,
    }
    declare_class!(
        struct Observer;
        unsafe impl ClassType for Observer {
            type Super = NSObject;
            type Mutability = MainThreadOnly;
            const NAME: &'static str = "RndrArtifactPolicyTestObserver";
        }
        impl DeclaredClass for Observer { type Ivars = Ivars; }
        unsafe impl NSObjectProtocol for Observer {}
        unsafe impl WKScriptMessageHandler for Observer {
            #[method(userContentController:didReceiveScriptMessage:)]
            unsafe fn report(&self, _controller: &WKUserContentController, message: &WKScriptMessage) {
                let body = message.body();
                let text: Retained<NSString> = msg_send_id![&body, description];
                self.ivars().reports.borrow_mut().push(text.to_string());
            }
        }
    );

    struct HttpProbe {
        address: std::net::SocketAddr,
        requests: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }
    impl HttpProbe {
        fn new() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(AtomicUsize::new(0));
            let stop = Arc::new(AtomicBool::new(false));
            let count = requests.clone();
            let done = stop.clone();
            let worker = std::thread::spawn(move || {
                while !done.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            count.fetch_add(1, Ordering::SeqCst);
                            stream
                                .set_read_timeout(Some(Duration::from_millis(100)))
                                .unwrap();
                            let _ = stream.read(&mut [0; 4096]);
                            let body = "<title>ordinary-ok</title><script>document.title='ordinary-js-ok'</script>";
                            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5))
                        }
                        Err(e) => panic!("fixture accept: {e}"),
                    }
                }
            });
            Self {
                address,
                requests,
                stop,
                worker: Some(worker),
            }
        }
        fn count(&self) -> usize {
            self.requests.load(Ordering::SeqCst)
        }
    }
    impl Drop for HttpProbe {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::SeqCst);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn web(parent: &NSView) -> Retained<WKWebView> {
        let children = unsafe { parent.subviews() };
        assert_eq!(children.len(), 1, "exactly one native surface");
        let view = unsafe { children.objectAtIndex(0) };
        // The only child was created by the public browser/preview API.
        unsafe { Retained::retain(Retained::as_ptr(&view).cast_mut().cast::<WKWebView>()) }.unwrap()
    }
    fn store(web: &WKWebView) -> Retained<WKWebsiteDataStore> {
        unsafe { web.configuration().websiteDataStore() }
    }
    fn observe(
        parent: &NSView,
        marker: MainThreadMarker,
    ) -> (Retained<Observer>, Rc<RefCell<Vec<String>>>) {
        let reports = Rc::new(RefCell::new(Vec::new()));
        let observer: Retained<Observer> = unsafe {
            msg_send_id![
                super(marker.alloc::<Observer>().set_ivars(Ivars {
                    reports: reports.clone()
                })),
                init
            ]
        };
        unsafe {
            web(parent)
                .configuration()
                .userContentController()
                .addScriptMessageHandler_name(
                    ProtocolObject::from_ref(&*observer),
                    &NSString::from_str("probe"),
                );
        }
        (observer, reports)
    }
    fn pump() {
        unsafe {
            NSRunLoop::mainRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.02));
        }
    }
    fn settle(preview: &mut ArtifactPreview, reports: &RefCell<Vec<String>>) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let state = preview.frame().expect("policy/load");
            if state == ArtifactState::Ready && reports.borrow().iter().any(|r| r == "DONE") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "timed out with reports {:?}",
                reports.borrow()
            );
            pump();
        }
        println!("native guest reports: {:?}", reports.borrow());
        assert!(
            !reports.borrow().iter().any(|r| r.starts_with("FAIL")),
            "defensive policy check failed"
        );
    }

    // Native fault injection lives ONLY in this executable. It hides one real
    // control from respondsToSelector; the unmodified production constructor
    // must refuse. No library feature/env switch can bypass a required gate.
    fn unavailable_controls(parent: &Parent, view: &NSView) {
        use objc2::runtime::{AnyClass, AnyObject, Bool, Sel};
        use objc2::sel;
        use objc2_web_kit::WKPreferences;
        use std::cell::Cell;
        use std::ffi::{c_char, c_void};
        extern "C" {
            fn class_getInstanceMethod(class: *const AnyClass, selector: Sel) -> *mut c_void;
            fn method_getImplementation(method: *mut c_void) -> *const c_void;
            fn method_getTypeEncoding(method: *mut c_void) -> *const c_char;
            fn class_replaceMethod(
                class: *const AnyClass,
                selector: Sel,
                implementation: *const c_void,
                types: *const c_char,
            ) -> *const c_void;
        }
        thread_local! {
            static ORIGINAL: Cell<*const c_void> = const { Cell::new(std::ptr::null()) };
            static DENIED: Cell<Option<Sel>> = const { Cell::new(None) };
        }
        unsafe extern "C" fn responds(object: *mut AnyObject, command: Sel, query: Sel) -> Bool {
            if DENIED.with(|d| d.get() == Some(query)) {
                return Bool::NO;
            }
            let original: unsafe extern "C" fn(*mut AnyObject, Sel, Sel) -> Bool =
                std::mem::transmute(ORIGINAL.with(Cell::get));
            original(object, command, query)
        }
        struct Restore {
            class: &'static AnyClass,
            original: *const c_void,
            types: *const c_char,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    class_replaceMethod(
                        self.class,
                        sel!(respondsToSelector:),
                        self.original,
                        self.types,
                    );
                }
                DENIED.with(|d| d.set(None));
                ORIGINAL.with(|o| o.set(std::ptr::null()));
            }
        }
        for (class, selector) in [
            (WKPreferences::class(), sel!(_setPeerConnectionEnabled:)),
            (WKPreferences::class(), sel!(_peerConnectionEnabled)),
            (WKPreferences::class(), sel!(_setDNSPrefetchingEnabled:)),
            (WKPreferences::class(), sel!(_dnsPrefetchingEnabled)),
            (WKPreferences::class(), sel!(_setFileSystemAccessEnabled:)),
            (WKPreferences::class(), sel!(_fileSystemAccessEnabled)),
            (
                WKPreferences::class(),
                sel!(_setJavaScriptCanAccessClipboard:),
            ),
            (WKPreferences::class(), sel!(_javaScriptCanAccessClipboard)),
            (WKPreferences::class(), sel!(_setEnabled:forFeature:)),
            (WKPreferences::class(), sel!(_isEnabledForFeature:)),
            (WKPreferences::class(), sel!(_setApplePayEnabled:)),
            (WKPreferences::class(), sel!(_applePayEnabled)),
            (WKPreferences::class(), sel!(setElementFullscreenEnabled:)),
            (WKPreferences::class(), sel!(isElementFullscreenEnabled)),
            (WKPreferences::class(), sel!(_setDOMPasteAllowed:)),
            (WKPreferences::class(), sel!(_domPasteAllowed)),
            (WKWebView::class(), sel!(_close)),
        ] {
            assert!(unsafe { view.subviews() }.is_empty());
            let method = unsafe { class_getInstanceMethod(class, sel!(respondsToSelector:)) };
            assert!(!method.is_null());
            let original = unsafe { method_getImplementation(method) };
            let types = unsafe { method_getTypeEncoding(method) };
            ORIGINAL.with(|o| o.set(original));
            DENIED.with(|d| d.set(Some(selector)));
            let restore = Restore {
                class,
                original,
                types,
            };
            unsafe {
                class_replaceMethod(
                    class,
                    sel!(respondsToSelector:),
                    responds as *const c_void,
                    types,
                );
            }
            assert!(
                matches!(
                    ArtifactPreview::attach(parent, "<script>throw 'must never load'</script>"),
                    Err(ArtifactError::Unsupported)
                ),
                "missing {selector:?} must return typed Unsupported"
            );
            assert!(
                unsafe { view.subviews() }.is_empty(),
                "refusal must not attach a child"
            );
            drop(restore);
            pump(); // There must be no pending async HTML load after refusal.
            assert!(unsafe { view.subviews() }.is_empty());
            println!("PASS missing runtime control {selector:?}: typed Unsupported before HTML/view attachment");
        }
    }

    // Test-only Objective-C fault injection: remove ONLY the sandbox directive
    // from the real production transport's preflight response. No library hook.
    fn missing_response_sandbox(handle: &Parent, parent: &NSView, marker: MainThreadMarker) {
        use objc2::runtime::{AnyClass, AnyObject, Sel};
        use objc2::sel;
        use objc2_foundation::{NSDictionary, NSHTTPURLResponse, NSURL};
        use std::{
            cell::Cell,
            ffi::{c_char, c_void},
        };
        extern "C" {
            fn class_getInstanceMethod(class: *const AnyClass, selector: Sel) -> *mut c_void;
            fn method_getImplementation(method: *mut c_void) -> *const c_void;
            fn method_getTypeEncoding(method: *mut c_void) -> *const c_char;
            fn class_replaceMethod(
                class: *const AnyClass,
                selector: Sel,
                implementation: *const c_void,
                types: *const c_char,
            ) -> *const c_void;
        }
        type Headers = NSDictionary<NSString, NSString>;
        type Initialize = unsafe extern "C" fn(
            *mut AnyObject,
            Sel,
            &NSURL,
            isize,
            Option<&NSString>,
            Option<&Headers>,
        ) -> *mut AnyObject;
        thread_local! { static INITIALIZE: Cell<*const c_void> = const { Cell::new(std::ptr::null()) }; static STRIPPED: Cell<bool> = const { Cell::new(false) }; }
        unsafe extern "C" fn initialize(
            object: *mut AnyObject,
            command: Sel,
            url: &NSURL,
            status: isize,
            version: Option<&NSString>,
            headers: Option<&Headers>,
        ) -> *mut AnyObject {
            let original: Initialize = std::mem::transmute(INITIALIZE.with(Cell::get));
            if url.absoluteString().map(|u| u.to_string()).as_deref()
                != Some("rndr-artifact://preview/preflight")
            {
                return original(object, command, url, status, version, headers);
            }
            let headers = headers.unwrap();
            let keys = headers.allKeys();
            let keys = (0..keys.len())
                .map(|i| keys.objectAtIndex(i))
                .collect::<Vec<_>>();
            let values = keys
                .iter()
                .map(|key| {
                    let value = headers.objectForKey(key).unwrap();
                    if key.to_string() == "Content-Security-Policy" {
                        let policy = value.to_string();
                        assert!(policy.starts_with("sandbox allow-scripts; "));
                        STRIPPED.with(|s| s.set(true));
                        NSString::from_str(policy.strip_prefix("sandbox allow-scripts; ").unwrap())
                    } else {
                        value
                    }
                })
                .collect::<Vec<_>>();
            let replacement =
                Headers::from_id_slice(&keys.iter().map(|v| &**v).collect::<Vec<_>>(), &values);
            original(object, command, url, status, version, Some(&replacement))
        }
        struct Restore {
            original: *const c_void,
            types: *const c_char,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                unsafe {
                    class_replaceMethod(
                        NSHTTPURLResponse::class(),
                        sel!(initWithURL:statusCode:HTTPVersion:headerFields:),
                        self.original,
                        self.types,
                    );
                }
            }
        }
        let method = unsafe {
            class_getInstanceMethod(
                NSHTTPURLResponse::class(),
                sel!(initWithURL:statusCode:HTTPVersion:headerFields:),
            )
        };
        assert!(!method.is_null());
        let restore = Restore {
            original: unsafe { method_getImplementation(method) },
            types: unsafe { method_getTypeEncoding(method) },
        };
        INITIALIZE.with(|o| o.set(restore.original));
        unsafe {
            class_replaceMethod(
                NSHTTPURLResponse::class(),
                sel!(initWithURL:statusCode:HTTPVersion:headerFields:),
                initialize as *const c_void,
                restore.types,
            );
        }
        let mut preview = ArtifactPreview::attach(
            handle,
            "<script>webkit.messageHandlers.probe.postMessage('FAIL authored source ran')</script>",
        )
        .unwrap();
        let (_observer, reports) = observe(parent, marker);
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            pump();
            match preview.frame() {
                Err(ArtifactError::Unsupported) => break,
                other => {
                    assert!(other.is_ok(), "unexpected preflight result: {other:?}");
                    assert!(
                        Instant::now() < deadline,
                        "missing sandbox preflight did not refuse"
                    );
                }
            }
        }
        assert!(STRIPPED.with(Cell::get));
        assert!(
            reports.borrow().is_empty(),
            "authored source must never execute on missing sandbox"
        );
        assert!(
            unsafe { parent.subviews() }.is_empty(),
            "sandbox refusal must detach source-free native view"
        );
        drop(preview);
        drop(restore);
        println!("PASS missing response sandbox: actual production preflight Unsupported, authored source never loaded/executed");
    }

    pub(super) fn run() {
        let marker = MainThreadMarker::new().expect("main thread");
        let parent = unsafe {
            NSView::initWithFrame(
                marker.alloc(),
                NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(800.0, 600.0)),
            )
        };
        let handle = Parent(Retained::as_ptr(&parent).cast_mut().cast());
        unavailable_controls(&handle, &parent);
        missing_response_sandbox(&handle, &parent, marker);
        let http = HttpProbe::new();
        let udp = UdpSocket::bind("127.0.0.1:0").unwrap();
        udp.set_nonblocking(true).unwrap();
        let html = include_str!("artifact_policy_probe.html")
            .replace("{{HTTP}}", &format!("http://{}", http.address))
            .replace("{{WS}}", &format!("ws://{}", http.address))
            .replace("{{STUN}}", &format!("stun:{}", udp.local_addr().unwrap()));
        let mut preview = ArtifactPreview::attach(&handle, &html).expect("attach");
        let old_web = web(&parent);
        let old_store = store(&old_web);
        assert!(
            !unsafe { old_store.isPersistent() },
            "artifact must not use an ordinary profile"
        );
        assert_eq!(
            unsafe {
                old_web
                    .configuration()
                    .userContentController()
                    .userScripts()
            }
            .len(),
            0,
            "no injected library script"
        );
        let delegate = unsafe { old_web.UIDelegate() }.unwrap();
        let mask: usize = unsafe {
            msg_send![&delegate, _webView: &*old_web dragDestinationActionMaskForDraggingInfo: &*NSObject::new()]
        };
        assert_eq!(mask, 0, "native file-drop veto");
        let (_observer, reports) = observe(&parent, marker);
        let bounds = ClientRect::try_new(10.0, 20.0, 600.0, 400.0).unwrap();
        preview.set_bounds(bounds).unwrap();
        preview.show();
        settle(&mut preview, &reports);
        assert_eq!(
            http.count(),
            0,
            "artifact must make zero HTTP/resource/preconnect connections"
        );
        assert_eq!(
            udp.recv_from(&mut [0; 4096]).unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock,
            "no STUN datagram"
        );
        assert_eq!(
            unsafe { old_web.URL().unwrap().absoluteString().unwrap() }
                .to_string()
                .split('#')
                .next()
                .unwrap(),
            "rndr-artifact://preview/document",
            "only an inert spent in-memory URL, never file/daemon origin"
        );
        assert!(matches!(
            preview.replace(&"x".repeat(MAX_ARTIFACT_HTML_BYTES + 1)),
            Err(ArtifactError::TooLarge)
        ));
        assert_eq!(
            Retained::as_ptr(&web(&parent)),
            Retained::as_ptr(&old_web),
            "refused replacement preserves surface"
        );
        assert!(preview
            .set_bounds(ClientRect {
                x: f32::NAN,
                ..bounds
            })
            .is_err());
        preview.hide();
        preview.frame().unwrap();
        assert!(unsafe { old_web.isHidden() });
        preview.show();
        preview.frame().unwrap();
        assert!(!unsafe { old_web.isHidden() });

        // Do not retain the old view: that would be a privileged host bypass of
        // the owning API's lifecycle. Store identity alone is safe to retain.
        drop(old_web);
        drop(delegate);
        preview.replace(r#"<button id="fresh" onclick="this.textContent='new local state'">fresh</button><script>
          const p = s => webkit.messageHandlers.probe.postMessage(s);
          p(typeof window.count === 'undefined' && window.name === '' ? 'OK fresh JS and window state' : 'FAIL old state');
          fresh.click(); p(fresh.textContent === 'new local state' ? 'OK replacement local interaction' : 'FAIL interaction');
          document.open(); document.write('<!doctype html><meta http-equiv=\"Content-Security-Policy\" content=\"default-src * data: blob: \'unsafe-inline\'\"><p>rewritten</p>'); document.close();
          const blocked=f=>{try{f();return false}catch{return true}};
          p(self.origin==='null' && blocked(()=>localStorage.setItem('x','1')) && blocked(()=>indexedDB.open('x')) ? 'OK sandbox persists after document.open/write' : 'FAIL sandbox lost after rewrite');
          p(!navigator.clipboard && !document.fullscreenEnabled && !window.ApplePaySession ? 'OK capability denial persists after rewrite' : 'FAIL rewrite capabilities');
          const form=document.createElement('form');form.action='http://127.0.0.1:1/rewrite-form';document.body.append(form);form.submit();
          p(window.open('http://127.0.0.1:1/rewrite-popup')===null ? 'OK popup denied after rewrite' : 'FAIL rewrite popup');
          try{location.href='http://127.0.0.1:1/rewrite-navigation'}catch{}
          fetch('http://127.0.0.1:1/rewrite').then(()=>p('FAIL rewrite fetch'),()=>p('OK CSP persists after document.write'));
          const replacementButton = document.createElement('button'); replacementButton.textContent='rewritten'; document.body.append(replacementButton);

          setInterval(() => p('HEARTBEAT'), 50);
          p('DONE');
        </script>"#).unwrap();
        let new_web = web(&parent);
        assert_ne!(
            Retained::as_ptr(&store(&new_web)),
            Retained::as_ptr(&old_store),
            "fresh ephemeral data store per replacement"
        );
        assert_eq!(
            unsafe {
                new_web
                    .configuration()
                    .userContentController()
                    .userScripts()
            }
            .len(),
            0
        );
        let (_new_observer, new_reports) = observe(&parent, marker);
        settle(&mut preview, &new_reports);
        let retired_deadline = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < retired_deadline {
            pump();
        }
        assert!(
            !reports.borrow().iter().any(|r| r == "OLD-TIMER"),
            "retired document must not keep running timers"
        );
        assert_eq!(
            new_web.frame(),
            old_frame(bounds, &parent),
            "replacement preserves logical bounds"
        );
        let overlay = OverlayBitmap {
            bounds: ClientRect::try_new(130., 90., 60., 30.).unwrap(),
            width: 2,
            height: 2,
            rgba: [220, 50, 100, 128].repeat(4),
            pointer_bounds: None,
        };
        preview
            .set_overlays(std::slice::from_ref(&overlay))
            .unwrap();
        preview.frame().unwrap();
        assert_eq!(
            unsafe { parent.subviews() }.len(),
            2,
            "native alpha sibling above guest"
        );
        let image_identity = unsafe { Retained::as_ptr(&parent.subviews().objectAtIndex(1)) };
        preview
            .set_overlays(std::slice::from_ref(&overlay))
            .unwrap();
        preview.frame().unwrap();
        assert_eq!(
            unsafe { Retained::as_ptr(&parent.subviews().objectAtIndex(1)) },
            image_identity,
            "unchanged overlays do not rebuild native images"
        );
        let mut invalid = overlay.clone();
        invalid.rgba.pop();
        assert!(preview.set_overlays(&[invalid]).is_err());
        preview.frame().unwrap();
        assert_eq!(
            unsafe { Retained::as_ptr(&parent.subviews().objectAtIndex(1)) },
            image_identity,
            "invalid submission preserves previous plane"
        );
        preview.hide();
        preview.frame().unwrap();
        let children = unsafe { parent.subviews() };
        assert!((0..children.len()).all(|i| unsafe { children.objectAtIndex(i).isHidden() }));
        preview.show();
        preview.frame().unwrap();
        let children = unsafe { parent.subviews() };
        assert!((0..children.len()).all(|i| !unsafe { children.objectAtIndex(i).isHidden() }));
        preview.set_overlays(&[]).unwrap();
        preview.frame().unwrap();
        assert_eq!(
            unsafe { parent.subviews() }.len(),
            1,
            "empty submission removes sibling"
        );
        preview.set_overlays(&[overlay]).unwrap();
        preview.frame().unwrap();
        drop(new_web);
        let reports_at_drop = new_reports.borrow().len();
        drop(preview);
        assert_eq!(
            unsafe { parent.subviews() }.len(),
            0,
            "drop removes native surface"
        );
        let preparing =
            ArtifactPreview::attach(&handle, "<script>throw 'must not run after drop'</script>")
                .unwrap();
        drop(preparing);
        for _ in 0..20 {
            pump();
        }
        assert_eq!(
            unsafe { parent.subviews() }.len(),
            0,
            "late policy completion cannot resurrect a dropped view"
        );
        assert_eq!(
            new_reports.borrow().len(),
            reports_at_drop,
            "dropping an active preview stops its timer callbacks"
        );
        assert_eq!(http.count(), 0);
        ordinary_browsing(&handle, &http);
        println!("PASS real WKWebView policy, zero loopback HTTP/STUN, local interaction, replacement, drop and ordinary browsing");
    }

    fn old_frame(bounds: ClientRect, parent: &NSView) -> NSRect {
        NSRect::new(
            NSPoint::new(
                bounds.x as f64,
                parent.bounds().size.height - bounds.y as f64 - bounds.height as f64,
            ),
            NSSize::new(bounds.width as f64, bounds.height as f64),
        )
    }
    fn ordinary_browsing(parent: &Parent, http: &HttpProbe) {
        let mut browser = Browser::attach(parent).unwrap();
        browser
            .navigate(&format!("http://{}/ordinary", http.address))
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let events = browser.frame();
            if events.iter().any(|e| matches!(e, BrowserEvent::Navigation(s) if s.load == LoadOutcome::Finished && s.title == "ordinary-js-ok")) { break; }
            assert!(
                Instant::now() < deadline,
                "ordinary URL browsing must still load resources and run JS"
            );
            pump();
        }
        assert!(
            http.count() >= 1,
            "resource policy must not leak into ordinary browser"
        );
    }
}
