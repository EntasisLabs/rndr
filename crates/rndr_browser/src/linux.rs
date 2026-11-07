//! Linux WebKitGTK on an X11 parent.
//!
//! GTK opens its own display connection. On X11 the server still lets that
//! connection create a child of the host window, and GDK can wrap the child.
//! A Wayland surface cannot be a parent of a GTK widget: `wl_subsurface`
//! has to belong to the same client as the parent, and winit and GTK are
//! different clients. That session is [`BrowserError::Unsupported`].
//!
//! The host loop does not pump GLib. [`WebKitGtk::pull`] drains pending GTK
//! iterations so observations arrive on [`crate::Browser::frame`].

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::CStr;
use std::os::raw::{c_int, c_ulong};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gdkx11::X11Display;
use gdkx11::X11Window;
use gtk::glib;
use gtk::glib::prelude::*;
use gtk::prelude::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use webkit2gtk::{
    AuthenticationRequestExt, Download, DownloadExt, FileChooserRequest, FileChooserRequestExt,
    HardwareAccelerationPolicy, LoadEvent, PolicyDecisionExt, PolicyDecisionType,
    ResponsePolicyDecision, ResponsePolicyDecisionExt, SettingsExt, URIRequestExt, URIResponseExt,
    WebContextExt, WebView, WebViewExt,
};
use x11::xlib::{
    Display, XCreateSimpleWindow, XErrorEvent, XFlush, XGetDefault, XMapWindow, XMoveResizeWindow,
    XSetErrorHandler, XUnmapWindow,
};

use crate::contract::{Contract, LiveDocument};
use crate::policy::logical_to_pixels;
use crate::{
    BrowserError, DownloadIntent, FileInputIntent, IntentToken, LoadOutcome, NavigationState,
};

struct Host {
    notes: RefCell<Vec<Note>>,
    downloads: RefCell<HashMap<IntentToken, Download>>,
    file: RefCell<Option<(IntentToken, FileChooserRequest)>>,
    next: Cell<u64>,
    pending_url: RefCell<String>,
    stop_requested: Cell<bool>,
    alive: Cell<bool>,
}

enum Note {
    Navigation(NavigationState),
    Download(DownloadIntent),
    File(FileInputIntent),
}

pub(crate) struct WebKitGtk {
    gtk_window: gtk::Window,
    webview: WebView,
    xdisplay: *mut Display,
    child: c_ulong,
    scale: f64,
    host: Rc<Host>,
}

impl WebKitGtk {
    pub(crate) fn attach(parent: &impl HasWindowHandle) -> Result<Self, BrowserError> {
        let parent_xid = parent_xid(parent)?;
        ensure_gtk()?;
        install_error_handler();
        let display = gtk::gdk::Display::default().ok_or(BrowserError::Prerequisite {
            backend: "WebKitGTK X11",
        })?;
        let x11_display =
            display
                .downcast::<X11Display>()
                .map_err(|_| BrowserError::Prerequisite {
                    backend: "WebKitGTK X11",
                })?;
        let xdisplay = unsafe { gdkx11::ffi::gdk_x11_display_get_xdisplay(x11_display.as_ptr()) }
            as *mut Display;
        if xdisplay.is_null() {
            return Err(BrowserError::Prerequisite {
                backend: "WebKitGTK X11",
            });
        }
        let scale = xft_scale(xdisplay);
        let child = unsafe { XCreateSimpleWindow(xdisplay, parent_xid, 0, 0, 1, 1, 0, 0, 0) };
        if child == 0 {
            return Err(BrowserError::Parent);
        }
        let foreign = X11Window::foreign_new_for_display(&x11_display, child);
        let gdk_window: gtk::gdk::Window = foreign.upcast();
        let gtk_window = gtk::Window::new(gtk::WindowType::Toplevel);
        gtk_window.connect_realize(glib::clone!(@strong gdk_window => move |window| {
            window.set_window(gdk_window.clone());
        }));
        gtk_window.set_has_window(true);
        gtk_window.realize();
        let webview = WebView::new();
        if let Some(settings) = WebViewExt::settings(&webview) {
            settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Never);
        }
        gtk_window.add(&webview);
        webview.set_visible(false);
        let host = Rc::new(Host {
            notes: RefCell::new(Vec::new()),
            downloads: RefCell::new(HashMap::new()),
            file: RefCell::new(None),
            next: Cell::new(0),
            pending_url: RefCell::new(String::new()),
            stop_requested: Cell::new(false),
            alive: Cell::new(true),
        });
        wire(&webview, &host);
        gtk_window.show();
        Ok(Self {
            gtk_window,
            webview,
            xdisplay,
            child,
            scale,
            host,
        })
    }

    pub(crate) fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.webview.load_uri(url);
        Ok(())
    }

    pub(crate) fn back(&self) -> Result<(), BrowserError> {
        self.webview.go_back();
        Ok(())
    }

    pub(crate) fn forward(&self) -> Result<(), BrowserError> {
        self.webview.go_forward();
        Ok(())
    }

    pub(crate) fn reload(&self) -> Result<(), BrowserError> {
        self.webview.reload();
        Ok(())
    }

    pub(crate) fn stop(&self) {
        self.host.stop_requested.set(true);
        self.webview.stop_loading();
    }

    pub(crate) fn apply(&self, rect: crate::ClientRect, visible: bool) {
        let x = logical_to_pixels(rect.x, self.scale) as i32;
        let y = logical_to_pixels(rect.y, self.scale) as i32;
        let width = logical_to_pixels(rect.width, self.scale);
        let height = logical_to_pixels(rect.height, self.scale);
        unsafe {
            if visible && width > 0 && height > 0 {
                XMoveResizeWindow(self.xdisplay, self.child, x, y, width, height);
                XMapWindow(self.xdisplay, self.child);
                self.webview.set_visible(true);
                self.webview.set_size_request(width as i32, height as i32);
            } else {
                XUnmapWindow(self.xdisplay, self.child);
                self.webview.set_visible(false);
            }
            XFlush(self.xdisplay);
        }
    }

    pub(crate) fn show(&self) {
        unsafe {
            XMapWindow(self.xdisplay, self.child);
            XFlush(self.xdisplay);
        }
        self.webview.set_visible(true);
    }

    pub(crate) fn hide(&self) {
        unsafe {
            XUnmapWindow(self.xdisplay, self.child);
            XFlush(self.xdisplay);
        }
        self.webview.set_visible(false);
    }

    pub(crate) fn document(&self) -> LiveDocument {
        LiveDocument {
            title: self
                .webview
                .title()
                .map(|title| title.to_string())
                .unwrap_or_default(),
            url: {
                let live = self
                    .webview
                    .uri()
                    .map(|uri| uri.to_string())
                    .unwrap_or_default();
                if live.is_empty() {
                    self.host.pending_url.borrow().clone()
                } else {
                    live
                }
            },
            loading: self.webview.is_loading(),
            can_go_back: self.webview.can_go_back(),
            can_go_forward: self.webview.can_go_forward(),
        }
    }

    pub(crate) fn pull(&self, contract: &mut Contract) {
        for _ in 0..64 {
            if !gtk::events_pending() {
                break;
            }
            gtk::main_iteration_do(false);
        }
        let notes: Vec<Note> = self.host.notes.borrow_mut().drain(..).collect();
        for note in notes {
            match note {
                Note::Navigation(state) => contract.observe(state),
                Note::Download(intent) => contract.open_download(intent),
                Note::File(intent) => contract.open_file(intent),
            }
        }
    }

    pub(crate) fn commit_download(
        &self,
        token: IntentToken,
        destination: &Path,
    ) -> Result<(), BrowserError> {
        let Some(download) = self.host.downloads.borrow_mut().remove(&token) else {
            return Err(BrowserError::UnknownIntent);
        };
        let uri = file_uri(destination);
        download.set_destination(&uri);
        Ok(())
    }

    pub(crate) fn cancel_download(&self, token: IntentToken) {
        if let Some(download) = self.host.downloads.borrow_mut().remove(&token) {
            download.cancel();
        }
    }

    pub(crate) fn finish_file(
        &self,
        token: IntentToken,
        paths: Option<&[PathBuf]>,
    ) -> Result<(), BrowserError> {
        let Some((open, request)) = self.host.file.borrow_mut().take() else {
            return Err(BrowserError::UnknownIntent);
        };
        if open != token {
            *self.host.file.borrow_mut() = Some((open, request));
            return Err(BrowserError::UnknownIntent);
        }
        match paths {
            Some(paths) if !paths.is_empty() => {
                let owned: Vec<String> = paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                let refs: Vec<&str> = owned.iter().map(String::as_str).collect();
                request.select_files(&refs);
            }
            _ => request.cancel(),
        }
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        self.host.alive.set(false);
        if let Some((_, request)) = self.host.file.borrow_mut().take() {
            request.cancel();
        }
        let downloads: Vec<_> = self
            .host
            .downloads
            .borrow_mut()
            .drain()
            .map(|(_, download)| download)
            .collect();
        for download in downloads {
            download.cancel();
        }
        self.gtk_window.close();
        unsafe { XUnmapWindow(self.xdisplay, self.child) };
    }
}

impl Drop for WebKitGtk {
    fn drop(&mut self) {
        if self.host.alive.get() {
            self.shutdown();
        }
    }
}

fn parent_xid(parent: &impl HasWindowHandle) -> Result<c_ulong, BrowserError> {
    let handle = parent.window_handle().map_err(|_| BrowserError::Parent)?;
    match handle.as_raw() {
        RawWindowHandle::Xlib(window) => Ok(window.window),
        RawWindowHandle::Xcb(window) => Ok(c_ulong::from(window.window.get())),
        RawWindowHandle::Wayland(_) => Err(BrowserError::Unsupported {
            backend: "WebKitGTK Wayland",
        }),
        _ => Err(BrowserError::Parent),
    }
}

fn ensure_gtk() -> Result<(), BrowserError> {
    if gtk::is_initialized() {
        if glib::MainContext::default().is_owner() {
            Ok(())
        } else {
            Err(BrowserError::Thread)
        }
    } else {
        // The parent is an X window. GTK must use X11 even if a Wayland
        // socket is also present, or the foreign window wrap fails.
        std::env::set_var("GDK_BACKEND", "x11");
        gtk::init().map_err(|_| BrowserError::Prerequisite {
            backend: "WebKitGTK X11",
        })
    }
}

fn install_error_handler() {
    unsafe {
        XSetErrorHandler(Some(ignore_x_error));
    }
}

unsafe extern "C" fn ignore_x_error(_display: *mut Display, _error: *mut XErrorEvent) -> c_int {
    0
}

fn xft_scale(display: *mut Display) -> f64 {
    let dpi = unsafe { XGetDefault(display, c"Xft".as_ptr(), c"dpi".as_ptr()) };
    if dpi.is_null() {
        return 1.0;
    }
    let text = unsafe { CStr::from_ptr(dpi) }.to_string_lossy();
    text.parse::<f64>()
        .ok()
        .filter(|dpi| dpi.is_finite() && *dpi > 0.0)
        .map(|dpi| dpi / 96.0)
        .unwrap_or(1.0)
}

fn wire(webview: &WebView, host: &Rc<Host>) {
    let started = host.clone();
    webview.connect_load_changed(move |view, event| {
        if let Some(uri) = view.uri() {
            *started.pending_url.borrow_mut() = uri.to_string();
        }
        let load = match event {
            LoadEvent::Started | LoadEvent::Redirected | LoadEvent::Committed => {
                LoadOutcome::Loading
            }
            LoadEvent::Finished => LoadOutcome::Finished,
            _ => return,
        };
        started.push_navigation(load);
    });
    let failed = host.clone();
    webview.connect_load_failed(move |_view, _event, uri, _error| {
        *failed.pending_url.borrow_mut() = uri.to_string();
        let load = if failed.stop_requested.replace(false) {
            LoadOutcome::Stopped
        } else {
            LoadOutcome::Failed
        };
        failed.push_navigation(load);
        false
    });
    let tls = host.clone();
    webview.connect_load_failed_with_tls_errors(move |_view, uri, _certificate, _flags| {
        // Do not accept the certificate. The default failure stands.
        *tls.pending_url.borrow_mut() = uri.to_string();
        tls.push_navigation(LoadOutcome::Failed);
        false
    });
    webview.connect_authenticate(move |_view, request| {
        request.cancel();
        true
    });
    let policy = host.clone();
    webview.connect_decide_policy(move |_view, decision, kind| {
        if kind == PolicyDecisionType::NewWindowAction {
            decision.ignore();
            return true;
        }
        if kind == PolicyDecisionType::Response {
            if let Some(response) = decision.downcast_ref::<ResponsePolicyDecision>() {
                if let Some(http) = response.response() {
                    // Cancelling an authentication request still commits the
                    // 401 document. Refuse that response instead.
                    if matches!(http.status_code(), 401 | 407) {
                        if let Some(uri) = http.uri() {
                            *policy.pending_url.borrow_mut() = uri.to_string();
                        }
                        policy.push_navigation(LoadOutcome::Failed);
                        decision.ignore();
                        return true;
                    }
                }
                if !response.is_mime_type_supported() {
                    decision.download();
                    return true;
                }
            }
        }
        false
    });
    let downloads = host.clone();
    if let Some(context) = webview.context() {
        context.connect_download_started(move |_context, download| {
            let host = downloads.clone();
            download.connect_decide_destination(move |download, suggested| {
                if !host.alive.get() {
                    download.cancel();
                    return true;
                }
                let url = download
                    .request()
                    .and_then(|request| request.uri())
                    .map(|uri| uri.to_string())
                    .unwrap_or_default();
                let mime = download
                    .response()
                    .and_then(|response| response.mime_type())
                    .map(|mime| mime.to_string())
                    .unwrap_or_default();
                let token = host.mint();
                host.downloads.borrow_mut().insert(token, download.clone());
                host.notes.borrow_mut().push(Note::Download(DownloadIntent {
                    token,
                    url,
                    suggested_name: suggested.to_string(),
                    mime,
                }));
                // Keep the class handler from failing the download before the
                // host chooses a path. `set_destination` runs on accept.
                glib::signal_stop_emission_by_name(download, "decide-destination");
                true
            });
        });
    }
    let files = host.clone();
    webview.connect_run_file_chooser(move |_view, request| {
        if !files.alive.get() {
            request.cancel();
            return true;
        }
        if files.file.borrow().is_some() {
            request.cancel();
            return true;
        }
        let token = files.mint();
        let multiple = request.selects_multiple();
        *files.file.borrow_mut() = Some((token, request.clone()));
        files
            .notes
            .borrow_mut()
            .push(Note::File(FileInputIntent { token, multiple }));
        true
    });
}

impl Host {
    fn mint(&self) -> IntentToken {
        let next = self.next.get().saturating_add(1);
        self.next.set(next);
        IntentToken::minted(next)
    }

    fn push_navigation(&self, load: LoadOutcome) {
        if !self.alive.get() {
            return;
        }
        self.notes
            .borrow_mut()
            .push(Note::Navigation(NavigationState {
                title: String::new(),
                url: self.pending_url.borrow().clone(),
                load,
                can_go_back: false,
                can_go_forward: false,
            }));
    }
}

fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy();
    format!("file://{text}")
}
