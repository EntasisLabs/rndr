//! Attached winit/AppKit scrolling probe. Test-only guest observation channel.
//! Run with native-artifact-harness,macos-private-webkit-artifacts.
#[cfg(not(target_os = "macos"))]
fn main() {
    panic!("macOS only");
}
#[cfg(target_os = "macos")]
fn main() {
    macos::run();
}
#[cfg(target_os = "macos")]
mod macos {
    use objc2::mutability::MainThreadOnly;
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{declare_class, msg_send_id, ClassType, DeclaredClass};
    use objc2_app_kit::NSView;
    use objc2_foundation::{MainThreadMarker, NSObject, NSObjectProtocol, NSString};
    use objc2_web_kit::{
        WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKWebView,
    };
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use rndr_browser::{ArtifactPreview, ClientRect};
    use std::{
        cell::RefCell,
        rc::Rc,
        time::{Duration, Instant},
    };
    use winit::{
        application::ApplicationHandler,
        event::WindowEvent,
        event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
        window::{Window, WindowAttributes, WindowId},
    };
    struct Ivars {
        reports: Rc<RefCell<Vec<String>>>,
    }
    declare_class!(
        struct Observer;
        unsafe impl ClassType for Observer { type Super = NSObject; type Mutability = MainThreadOnly; const NAME: &'static str = "RndrScrollObserver"; }
        impl DeclaredClass for Observer { type Ivars = Ivars; }
        unsafe impl NSObjectProtocol for Observer {}
        unsafe impl WKScriptMessageHandler for Observer {
            #[method(userContentController:didReceiveScriptMessage:)]
            unsafe fn report(&self, _: &WKUserContentController, message: &WKScriptMessage) {
                let body = message.body();
                let text: Retained<NSString> = msg_send_id![&body, description];
                eprintln!("scroll probe: {}", text);
                self.ivars().reports.borrow_mut().push(text.to_string());
            }
        }
    );
    #[derive(Default)]
    struct App {
        preview: Option<ArtifactPreview>,
        observer: Option<Retained<Observer>>,
        window: Option<Box<dyn Window>>,
        reports: Rc<RefCell<Vec<String>>>,
        started: Option<Instant>,
    }
    impl ApplicationHandler for App {
        fn can_create_surfaces(&mut self, events: &dyn ActiveEventLoop) {
            let window = events
                .create_window(
                    WindowAttributes::default()
                        .with_title("RNDR native scroll probe")
                        .with_surface_size(winit::dpi::LogicalSize::new(800, 600)),
                )
                .unwrap();
            let html = r#"<style>body{font:22px system-ui;margin:24px;background:#edf4fa}#nested{position:absolute;top:150px;left:24px;width:80%;height:120px;overflow:auto;border:3px solid #456}#nested div{height:900px;background:linear-gradient(#acf,#fca)}main{margin-top:200px;height:2600px;width:1200px;background:linear-gradient(#edf4fa,#a8d8bd)}output{position:fixed;right:20px;top:10px;background:white;padding:10px}</style><output id='position'>TOP</output><h1>Native scrolling</h1><p>Wheel/trackpad here scrolls the document. The bordered panel scrolls independently.</p><section id='nested'><div>NESTED TOP<br><span style='position:relative;top:800px'>NESTED BOTTOM</span></div></section><main>DOCUMENT TOP</main><h2>DOCUMENT BOTTOM</h2><script>const p=s=>webkit.messageHandlers.probe.postMessage(s);p('READY');addEventListener('scroll',()=>{document.getElementById('position').textContent='X '+scrollX+' Y '+scrollY;p('ROOT '+scrollX+' '+scrollY)});nested.addEventListener('scroll',()=>p('NESTED '+nested.scrollTop));</script>"#;
            let legacy = r#"<style id="medousa-artifact-host">:root{--medousa-host-bg:transparent;--medousa-host-fg:inherit;--medousa-host-muted:inherit}:where(html,body){margin:0;padding:0}html,body{overflow:hidden;scrollbar-width:none;-ms-overflow-style:none}html::-webkit-scrollbar,body::-webkit-scrollbar{display:none;width:0;height:0}/* Agent utility: .medousa-fill { min-height:100%; width:100% } */</style>"#;
            // The embedding app strips its own obsolete auto-height host style.
            // Model that exact removal; RNDR itself must never rewrite guest CSS.
            let html = if std::env::args().any(|a| a == "--stripped-legacy-host-style") {
                format!("{legacy}{html}").replacen(legacy, "", 1)
            } else if std::env::args().any(|a| a == "--legacy-host-style") {
                format!("{legacy}{html}")
            } else {
                html.to_owned()
            };
            let mut html = if let Some(path) = std::env::args().skip_while(|a| a != "--html").nth(1)
            {
                let source = std::fs::read_to_string(path).expect("local fixture HTML");
                format!("{source}<script>const report=()=>webkit.messageHandlers.probe.postMessage('EDGE '+scrollY+' MAX '+(document.documentElement.scrollHeight-innerHeight));addEventListener('scroll',report);report();</script>")
            } else {
                html
            };
            if std::env::args().any(|a| a == "--contain-viewport") {
                html.push_str("<style>html{overscroll-behavior:none!important}</style>");
            }
            let mut preview = ArtifactPreview::attach(&window, &html).unwrap();
            let mtm = MainThreadMarker::new().unwrap();
            let RawWindowHandle::AppKit(handle) = window.window_handle().unwrap().as_raw() else {
                panic!()
            };
            let parent = unsafe { &*handle.ns_view.as_ptr().cast::<NSView>() };
            let children = unsafe { parent.subviews() };
            let child = unsafe { children.objectAtIndex(0) };
            let web = unsafe { &*Retained::as_ptr(&child).cast::<WKWebView>() };
            let observer: Retained<Observer> = unsafe {
                msg_send_id![
                    super(mtm.alloc::<Observer>().set_ivars(Ivars {
                        reports: self.reports.clone()
                    })),
                    init
                ]
            };
            unsafe {
                web.configuration()
                    .userContentController()
                    .addScriptMessageHandler_name(
                        ProtocolObject::from_ref(&*observer),
                        &NSString::from_str("probe"),
                    );
            }
            preview.show();
            self.preview = Some(preview);
            self.observer = Some(observer);
            self.window = Some(window);
            self.started = Some(Instant::now());
            eprintln!("scroll probe pid {}", std::process::id());
        }
        fn window_event(&mut self, events: &dyn ActiveEventLoop, _: WindowId, event: WindowEvent) {
            if matches!(event, WindowEvent::CloseRequested) {
                self.preview = None;
                self.window = None;
                events.exit();
            }
            if matches!(event, WindowEvent::MouseWheel { .. }) {
                eprintln!("HOST WHEEL {event:?}");
            }
        }
        fn about_to_wait(&mut self, events: &dyn ActiveEventLoop) {
            let (Some(window), Some(preview)) = (&self.window, &mut self.preview) else {
                return;
            };
            let size = window
                .surface_size()
                .to_logical::<f32>(window.scale_factor());
            preview
                .set_bounds(
                    ClientRect::try_new(100., 70., size.width - 160., size.height - 140.).unwrap(),
                )
                .unwrap();
            preview.frame().unwrap();
            if self.started.unwrap().elapsed() > Duration::from_secs(120) {
                self.preview = None;
                self.window = None;
                events.exit();
                return;
            }
            events.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        }
    }
    pub fn run() {
        EventLoop::new().unwrap().run_app(App::default()).unwrap();
    }
}
