//! macOS WKWebView. This module is the view's owner.
//!
//! The session retains the view, the delegate, and the parent view. WebKit's
//! delegate properties are weak, so dropping the session is what releases them.
//! Download and file-input completion handlers are copied and called once:
//! with a destination only after the host accepts, or with null on cancel or
//! drop. No panel is created. No script handler or scheme handler is installed.
//!
//! Delegate methods keep WebKit's selector names.

#![allow(non_snake_case)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

use block2::RcBlock;
use objc2::mutability::MainThreadOnly;
use objc2::rc::Retained;
use objc2::runtime::{Bool, ProtocolObject};
use objc2::{declare_class, msg_send, msg_send_id, sel, ClassType, DeclaredClass};
use objc2_app_kit::NSView;
use objc2_foundation::{
    MainThreadMarker, NSArray, NSError, NSHTTPURLResponse, NSObject, NSObjectProtocol, NSPoint,
    NSRect, NSSize, NSString, NSURLAuthenticationChallenge, NSURLCredential,
    NSURLErrorFailingURLErrorKey, NSURLErrorFailingURLStringErrorKey, NSURLRequest, NSURLResponse,
    NSURLSessionAuthChallengeDisposition, NSURL,
};
use objc2_web_kit::{
    WKDownload, WKDownloadDelegate, WKDownloadRedirectPolicy, WKFrameInfo, WKNavigation,
    WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate, WKNavigationResponse,
    WKNavigationResponsePolicy, WKOpenPanelParameters, WKPermissionDecision, WKSecurityOrigin,
    WKUIDelegate, WKWebView, WKWebViewConfiguration,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::contract::Contract;
use crate::{
    BrowserError, BrowserEvent, DownloadIntent, FileInputIntent, IntentToken, LoadOutcome,
    NavigationState,
};

const CANCELLED: isize = -999;

pub(crate) struct WebKit {
    web: Retained<WKWebView>,
    /// Kept alive because WebKit's delegate pointers are weak.
    delegate: Retained<WebKitHost>,
    /// Content view the host passed in. Its height converts top-left rects.
    parent: Retained<NSView>,
    host: Rc<HostInner>,
    torn_down: bool,
}

struct HostIvars {
    host: Rc<HostInner>,
}

struct HostInner {
    alive: Cell<bool>,
    stop_requested: Cell<bool>,
    next: Cell<u64>,
    notes: RefCell<Vec<Note>>,
    downloads: RefCell<HashMap<IntentToken, DownloadSlot>>,
    files: RefCell<HashMap<IntentToken, FileSlot>>,
    download_ptrs: RefCell<HashMap<usize, IntentToken>>,
}

enum Note {
    Navigation(NavigationState),
    Download(DownloadIntent),
    File(FileInputIntent),
    Cancelled(IntentToken),
}

struct DownloadSlot {
    handler: RcBlock<dyn Fn(*mut NSURL)>,
    answered: bool,
}

struct FileSlot {
    handler: RcBlock<dyn Fn(*mut NSArray<NSURL>)>,
    answered: bool,
}

impl HostInner {
    fn new() -> Self {
        Self {
            alive: Cell::new(true),
            stop_requested: Cell::new(false),
            next: Cell::new(0),
            notes: RefCell::new(Vec::new()),
            downloads: RefCell::new(HashMap::new()),
            files: RefCell::new(HashMap::new()),
            download_ptrs: RefCell::new(HashMap::new()),
        }
    }

    fn mint(&self) -> IntentToken {
        let next = self.next.get().saturating_add(1);
        self.next.set(next);
        IntentToken::minted(next)
    }

    fn push(&self, note: Note) {
        if self.alive.get() {
            self.notes.borrow_mut().push(note);
        }
    }

    fn cancel_outstanding(&self) {
        let downloads: Vec<DownloadSlot> = self
            .downloads
            .borrow_mut()
            .drain()
            .map(|(_, slot)| slot)
            .collect();
        self.download_ptrs.borrow_mut().clear();
        let files: Vec<FileSlot> = self
            .files
            .borrow_mut()
            .drain()
            .map(|(_, slot)| slot)
            .collect();
        for slot in downloads {
            slot.cancel();
        }
        for slot in files {
            slot.cancel();
        }
    }
}

impl DownloadSlot {
    fn destination(mut self, url: &Retained<NSURL>) {
        self.answered = true;
        // The copied block is the one WebKit handed us. `url` stays alive for
        // the call, and WebKit retains the destination. Nothing was written
        // before this call.
        self.handler.call((Retained::as_ptr(url).cast_mut(),));
    }

    fn cancel(mut self) {
        self.answered = true;
        self.handler.call((std::ptr::null_mut(),));
    }
}

impl Drop for DownloadSlot {
    fn drop(&mut self) {
        if !self.answered {
            self.answered = true;
            self.handler.call((std::ptr::null_mut(),));
        }
    }
}

impl FileSlot {
    fn answer(mut self, urls: &Retained<NSArray<NSURL>>) {
        self.answered = true;
        self.handler.call((Retained::as_ptr(urls).cast_mut(),));
    }

    fn cancel(mut self) {
        self.answered = true;
        self.handler.call((std::ptr::null_mut(),));
    }
}

impl Drop for FileSlot {
    fn drop(&mut self) {
        if !self.answered {
            self.answered = true;
            self.handler.call((std::ptr::null_mut(),));
        }
    }
}

impl WebKit {
    pub(crate) fn attach(parent: &impl HasWindowHandle) -> Result<Self, BrowserError> {
        let handle = parent.window_handle().map_err(|_| BrowserError::Parent)?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return Err(BrowserError::Parent);
        };
        let ns_view = appkit.ns_view;
        let mtm = MainThreadMarker::new().ok_or(BrowserError::Thread)?;
        // Safety: winit's AppKit handle is the content view. `retain` only
        // runs on the main thread, after the handle check above. The extra
        // retain is balanced when this session drops the parent view.
        let parent_view = unsafe { Retained::<NSView>::retain(ns_view.as_ptr().cast()) }
            .ok_or(BrowserError::Parent)?;
        let host = Rc::new(HostInner::new());
        let delegate = WebKitHost::new(mtm, Rc::clone(&host));
        // Safety: `new` and `init` are the designated constructors. The view
        // is hidden and has no navigation until the host asks.
        let configuration = unsafe { WKWebViewConfiguration::new() };
        let web = unsafe {
            WKWebView::initWithFrame_configuration(mtm.alloc(), NSRect::ZERO, &configuration)
        };
        web.setHidden(true);
        unsafe {
            parent_view.addSubview(&web);
            let navigation = ProtocolObject::from_ref(&*delegate);
            web.setNavigationDelegate(Some(navigation));
            let ui = ProtocolObject::from_ref(&*delegate);
            web.setUIDelegate(Some(ui));
        }
        Ok(Self {
            web,
            delegate,
            parent: parent_view,
            host,
            torn_down: false,
        })
    }

    pub(crate) fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let text = NSString::from_str(url);
        // Safety: `URLWithString` returns nil for a string it will not parse.
        // A parsed custom scheme is still only a load request.
        let Some(url) = (unsafe { NSURL::URLWithString(&text) }) else {
            return Err(BrowserError::Url);
        };
        let request = unsafe { NSURLRequest::requestWithURL(&url) };
        unsafe { self.web.loadRequest(&request) };
        Ok(())
    }

    pub(crate) fn back(&self) -> Result<(), BrowserError> {
        unsafe { self.web.goBack() };
        Ok(())
    }

    pub(crate) fn forward(&self) -> Result<(), BrowserError> {
        unsafe { self.web.goForward() };
        Ok(())
    }

    pub(crate) fn reload(&self) -> Result<(), BrowserError> {
        unsafe { self.web.reload() };
        Ok(())
    }

    pub(crate) fn stop(&self) {
        self.host.stop_requested.set(true);
        unsafe { self.web.stopLoading() };
    }

    pub(crate) fn apply(&self, rect: crate::ClientRect, visible: bool) {
        let bounds = self.parent.bounds();
        let placed = crate::geometry::place_in_parent(
            crate::geometry::ParentBounds {
                flipped: self.parent.isFlipped(),
                origin_x: bounds.origin.x,
                origin_y: bounds.origin.y,
                width: bounds.size.width,
                height: bounds.size.height,
            },
            rect,
        );
        let frame = NSRect::new(
            NSPoint::new(placed.x, placed.y),
            NSSize::new(placed.width, placed.height),
        );
        let view: &NSView = &self.web;
        // Safety: `frame` is a finite rect the contract already admitted.
        // The view stays in the hierarchy even when hidden.
        unsafe { view.setFrame(frame) };
        view.setHidden(!visible);
    }

    pub(crate) fn show(&self) {
        let view: &NSView = &self.web;
        view.setHidden(false);
    }

    pub(crate) fn hide(&self) {
        let view: &NSView = &self.web;
        view.setHidden(true);
    }

    pub(crate) fn document(&self) -> crate::contract::LiveDocument {
        crate::contract::LiveDocument {
            title: unsafe { self.web.title() }
                .map(|title| title.to_string())
                .unwrap_or_default(),
            url: unsafe { self.web.URL() }
                .and_then(|url| unsafe { url.absoluteString() })
                .map(|url| url.to_string())
                .unwrap_or_default(),
            loading: unsafe { self.web.isLoading() },
            can_go_back: unsafe { self.web.canGoBack() },
            can_go_forward: unsafe { self.web.canGoForward() },
        }
    }

    pub(crate) fn pull(&self, contract: &mut Contract) {
        let notes: Vec<Note> = self.host.notes.borrow_mut().drain(..).collect();
        for note in notes {
            match note {
                Note::Navigation(state) => contract.observe(state),
                Note::Download(intent) => contract.open_download(intent),
                Note::File(intent) => contract.open_file(intent),
                Note::Cancelled(token) => {
                    if contract.close(token).is_some() {
                        contract.push(BrowserEvent::IntentCancelled { token });
                    }
                }
            }
        }
    }

    pub(crate) fn commit_download(
        &self,
        token: IntentToken,
        destination: &Path,
    ) -> Result<(), BrowserError> {
        let Some(text) = destination.to_str() else {
            return Err(BrowserError::Destination);
        };
        let Some(slot) = self.host.downloads.borrow_mut().remove(&token) else {
            return Err(BrowserError::UnknownIntent);
        };
        self.host
            .download_ptrs
            .borrow_mut()
            .retain(|_, id| *id != token);
        let path = NSString::from_str(text);
        // Safety: `fileURLWithPath` does not create the file. WebKit writes
        // only after this destination is handed to the copied handler.
        let url = unsafe { NSURL::fileURLWithPath(&path) };
        slot.destination(&url);
        Ok(())
    }

    pub(crate) fn cancel_download(&self, token: IntentToken) {
        self.host
            .download_ptrs
            .borrow_mut()
            .retain(|_, id| *id != token);
        if let Some(slot) = self.host.downloads.borrow_mut().remove(&token) {
            slot.cancel();
        }
    }

    pub(crate) fn finish_file(
        &self,
        token: IntentToken,
        paths: Option<&[PathBuf]>,
    ) -> Result<(), BrowserError> {
        let Some(paths) = paths else {
            if let Some(slot) = self.host.files.borrow_mut().remove(&token) {
                slot.cancel();
            }
            return Ok(());
        };
        let mut urls = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(text) = path.to_str() else {
                return Err(BrowserError::Destination);
            };
            let path = NSString::from_str(text);
            urls.push(unsafe { NSURL::fileURLWithPath(&path) });
        }
        let Some(slot) = self.host.files.borrow_mut().remove(&token) else {
            return Err(BrowserError::UnknownIntent);
        };
        let array = NSArray::from_vec(urls);
        slot.answer(&array);
        Ok(())
    }

    pub(crate) fn shutdown(&mut self) {
        if self.torn_down {
            return;
        }
        self.torn_down = true;
        self.host.alive.set(false);
        self.host.cancel_outstanding();
        let _delegate = &self.delegate;
        unsafe {
            self.web.setNavigationDelegate(None);
            self.web.setUIDelegate(None);
            let view: &NSView = &self.web;
            view.removeFromSuperview();
        }
    }
}

impl Drop for WebKit {
    fn drop(&mut self) {
        self.shutdown();
    }
}

declare_class!(
    struct WebKitHost;

    // Safety: NSObject has no subclassing requirements. The delegate is used
    // on the main thread because WKWebView invokes it there. This type does
    // not implement Drop; the session releases it.
    unsafe impl ClassType for WebKitHost {
        type Super = NSObject;
        type Mutability = MainThreadOnly;
        const NAME: &'static str = "RndrBrowserWebKitHost";
    }

    impl DeclaredClass for WebKitHost {
        type Ivars = HostIvars;
    }

    unsafe impl NSObjectProtocol for WebKitHost {}

    unsafe impl WKNavigationDelegate for WebKitHost {
        #[method(webView:decidePolicyForNavigationAction:decisionHandler:)]
        unsafe fn webView_decidePolicyForNavigationAction_decisionHandler(
            &self,
            _web_view: &WKWebView,
            navigation_action: &WKNavigationAction,
            decision_handler: &block2::Block<dyn Fn(WKNavigationActionPolicy)>,
        ) {
            // A download attribute becomes a download. Every other URL,
            // including a custom scheme, is allowed through as a navigation.
            // Nothing here reads a command name.
            let policy = if unsafe { navigation_action.shouldPerformDownload() } {
                WKNavigationActionPolicy::Download
            } else {
                WKNavigationActionPolicy::Allow
            };
            decision_handler.call((policy,));
        }

        #[method(webView:decidePolicyForNavigationResponse:decisionHandler:)]
        unsafe fn webView_decidePolicyForNavigationResponse_decisionHandler(
            &self,
            _web_view: &WKWebView,
            navigation_response: &WKNavigationResponse,
            decision_handler: &block2::Block<dyn Fn(WKNavigationResponsePolicy)>,
        ) {
            // 401 and 407 are credential failures. Cancelling the challenge does
            // not stop WebKit from committing that response as a finished
            // document, so the response itself is refused. No credential is sent.
            let policy = if http_status_requires_credentials(&navigation_response.response()) {
                WKNavigationResponsePolicy::Cancel
            } else if unsafe { navigation_response.canShowMIMEType() } {
                WKNavigationResponsePolicy::Allow
            } else {
                WKNavigationResponsePolicy::Download
            };
            decision_handler.call((policy,));
        }

        #[method(webView:didStartProvisionalNavigation:)]
        unsafe fn webView_didStartProvisionalNavigation(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.note_navigation(web_view, LoadOutcome::Loading);
        }

        #[method(webView:didReceiveServerRedirectForProvisionalNavigation:)]
        unsafe fn webView_didReceiveServerRedirectForProvisionalNavigation(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.note_navigation(web_view, LoadOutcome::Loading);
        }

        #[method(webView:didCommitNavigation:)]
        unsafe fn webView_didCommitNavigation(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.note_navigation(web_view, LoadOutcome::Loading);
        }

        #[method(webView:didFinishNavigation:)]
        unsafe fn webView_didFinishNavigation(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
        ) {
            self.note_navigation(web_view, LoadOutcome::Finished);
        }

        #[method(webView:didFailProvisionalNavigation:withError:)]
        unsafe fn webView_didFailProvisionalNavigation_withError(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.note_failure(web_view, error);
        }

        #[method(webView:didFailNavigation:withError:)]
        unsafe fn webView_didFailNavigation_withError(
            &self,
            web_view: &WKWebView,
            _navigation: Option<&WKNavigation>,
            error: &NSError,
        ) {
            self.note_failure(web_view, error);
        }

        #[method(webView:didReceiveAuthenticationChallenge:completionHandler:)]
        unsafe fn webView_didReceiveAuthenticationChallenge_completionHandler(
            &self,
            _web_view: &WKWebView,
            _challenge: &NSURLAuthenticationChallenge,
            completion_handler: &block2::Block<
                dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential),
            >,
        ) {
            answer_authentication(_challenge, completion_handler);
        }

        #[method(webView:navigationAction:didBecomeDownload:)]
        unsafe fn webView_navigationAction_didBecomeDownload(
            &self,
            _web_view: &WKWebView,
            _navigation_action: &WKNavigationAction,
            download: &WKDownload,
        ) {
            self.adopt_download(download);
        }

        #[method(webView:navigationResponse:didBecomeDownload:)]
        unsafe fn webView_navigationResponse_didBecomeDownload(
            &self,
            _web_view: &WKWebView,
            _navigation_response: &WKNavigationResponse,
            download: &WKDownload,
        ) {
            self.adopt_download(download);
        }

        #[method(webViewWebContentProcessDidTerminate:)]
        unsafe fn webViewWebContentProcessDidTerminate(&self, web_view: &WKWebView) {
            self.note_navigation(web_view, LoadOutcome::Failed);
        }
    }

    unsafe impl WKUIDelegate for WebKitHost {
        #[method(webView:runJavaScriptAlertPanelWithMessage:initiatedByFrame:completionHandler:)]
        unsafe fn webView_runJavaScriptAlertPanelWithMessage_initiatedByFrame_completionHandler(
            &self,
            _web_view: &WKWebView,
            _message: &NSString,
            _frame: &WKFrameInfo,
            completion_handler: &block2::Block<dyn Fn()>,
        ) {
            // The page text is not delivered to the host and no panel is shown.
            completion_handler.call(());
        }

        #[method(webView:runJavaScriptConfirmPanelWithMessage:initiatedByFrame:completionHandler:)]
        unsafe fn webView_runJavaScriptConfirmPanelWithMessage_initiatedByFrame_completionHandler(
            &self,
            _web_view: &WKWebView,
            _message: &NSString,
            _frame: &WKFrameInfo,
            completion_handler: &block2::Block<dyn Fn(Bool)>,
        ) {
            completion_handler.call((Bool::NO,));
        }

        #[method(webView:runJavaScriptTextInputPanelWithPrompt:defaultText:initiatedByFrame:completionHandler:)]
        unsafe fn webView_runJavaScriptTextInputPanelWithPrompt_defaultText_initiatedByFrame_completionHandler(
            &self,
            _web_view: &WKWebView,
            _prompt: &NSString,
            _default_text: Option<&NSString>,
            _frame: &WKFrameInfo,
            completion_handler: &block2::Block<dyn Fn(*mut NSString)>,
        ) {
            completion_handler.call((std::ptr::null_mut(),));
        }

        #[method(webView:requestMediaCapturePermissionForOrigin:initiatedByFrame:type:decisionHandler:)]
        unsafe fn webView_requestMediaCapturePermissionForOrigin_initiatedByFrame_type_decisionHandler(
            &self,
            _web_view: &WKWebView,
            _origin: &WKSecurityOrigin,
            _frame: &WKFrameInfo,
            _kind: objc2_web_kit::WKMediaCaptureType,
            decision_handler: &block2::Block<dyn Fn(WKPermissionDecision)>,
        ) {
            decision_handler.call((WKPermissionDecision::Deny,));
        }

        #[method(webView:requestDeviceOrientationAndMotionPermissionForOrigin:initiatedByFrame:decisionHandler:)]
        unsafe fn webView_requestDeviceOrientationAndMotionPermissionForOrigin_initiatedByFrame_decisionHandler(
            &self,
            _web_view: &WKWebView,
            _origin: &WKSecurityOrigin,
            _frame: &WKFrameInfo,
            decision_handler: &block2::Block<dyn Fn(WKPermissionDecision)>,
        ) {
            decision_handler.call((WKPermissionDecision::Deny,));
        }

        #[method(webView:runOpenPanelWithParameters:initiatedByFrame:completionHandler:)]
        unsafe fn webView_runOpenPanelWithParameters_initiatedByFrame_completionHandler(
            &self,
            _web_view: &WKWebView,
            parameters: &WKOpenPanelParameters,
            _frame: &WKFrameInfo,
            completion_handler: &block2::Block<dyn Fn(*mut NSArray<NSURL>)>,
        ) {
            let host = &self.ivars().host;
            if !host.alive.get() {
                completion_handler.call((std::ptr::null_mut(),));
                return;
            }
            let token = host.mint();
            let multiple = unsafe { parameters.allowsMultipleSelection() };
            host.files.borrow_mut().insert(
                token,
                FileSlot {
                    handler: completion_handler.copy(),
                    answered: false,
                },
            );
            host.push(Note::File(FileInputIntent { token, multiple }));
        }
    }

    unsafe impl WKDownloadDelegate for WebKitHost {
        #[method(download:decideDestinationUsingResponse:suggestedFilename:completionHandler:)]
        unsafe fn download_decideDestinationUsingResponse_suggestedFilename_completionHandler(
            &self,
            download: &WKDownload,
            response: &NSURLResponse,
            suggested_filename: &NSString,
            completion_handler: &block2::Block<dyn Fn(*mut NSURL)>,
        ) {
            let host = &self.ivars().host;
            if !host.alive.get() {
                completion_handler.call((std::ptr::null_mut(),));
                return;
            }
            let token = host.mint();
            let pointer = std::ptr::from_ref(download) as usize;
            host.download_ptrs.borrow_mut().insert(pointer, token);
            host.downloads.borrow_mut().insert(
                token,
                DownloadSlot {
                    handler: completion_handler.copy(),
                    answered: false,
                },
            );
            host.push(Note::Download(DownloadIntent {
                token,
                url: response_url(response),
                suggested_name: suggested_filename.to_string(),
                mime: response_mime(response),
            }));
        }

        #[method(download:willPerformHTTPRedirection:newRequest:decisionHandler:)]
        unsafe fn download_willPerformHTTPRedirection_newRequest_decisionHandler(
            &self,
            _download: &WKDownload,
            _response: &objc2_foundation::NSHTTPURLResponse,
            _request: &NSURLRequest,
            decision_handler: &block2::Block<dyn Fn(WKDownloadRedirectPolicy)>,
        ) {
            decision_handler.call((WKDownloadRedirectPolicy::Allow,));
        }

        #[method(download:didReceiveAuthenticationChallenge:completionHandler:)]
        unsafe fn download_didReceiveAuthenticationChallenge_completionHandler(
            &self,
            _download: &WKDownload,
            _challenge: &NSURLAuthenticationChallenge,
            completion_handler: &block2::Block<
                dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential),
            >,
        ) {
            answer_authentication(_challenge, completion_handler);
        }

        #[method(downloadDidFinish:)]
        unsafe fn downloadDidFinish(&self, _download: &WKDownload) {}

        #[method(download:didFailWithError:resumeData:)]
        unsafe fn download_didFailWithError_resumeData(
            &self,
            download: &WKDownload,
            _error: &NSError,
            _resume_data: Option<&objc2_foundation::NSData>,
        ) {
            let host = &self.ivars().host;
            let pointer = std::ptr::from_ref(download) as usize;
            let Some(token) = host.download_ptrs.borrow_mut().remove(&pointer) else {
                return;
            };
            if let Some(slot) = host.downloads.borrow_mut().remove(&token) {
                slot.cancel();
            }
            host.push(Note::Cancelled(token));
        }
    }
);

impl WebKitHost {
    fn new(mtm: MainThreadMarker, host: Rc<HostInner>) -> Retained<Self> {
        let this = mtm.alloc::<Self>().set_ivars(HostIvars { host });
        unsafe { msg_send_id![super(this), init] }
    }

    fn note_navigation(&self, web_view: &WKWebView, load: LoadOutcome) {
        self.ivars()
            .host
            .push(Note::Navigation(snapshot(web_view, load)));
    }

    fn note_failure(&self, web_view: &WKWebView, error: &NSError) {
        let host = &self.ivars().host;
        let stopped = host.stop_requested.replace(false) || error.code() == CANCELLED;
        let load = if stopped {
            LoadOutcome::Stopped
        } else {
            LoadOutcome::Failed
        };
        let mut state = snapshot(web_view, load);
        // A provisional failure leaves the view on the last committed document.
        // The error names the URL that failed, including an expired certificate.
        if let Some(url) = failing_url(error) {
            state.url = url;
        }
        host.push(Note::Navigation(state));
    }

    fn adopt_download(&self, download: &WKDownload) {
        let delegate = ProtocolObject::from_ref(self);
        unsafe { download.setDelegate(Some(delegate)) };
    }
}

fn answer_authentication(
    challenge: &impl objc2::Message,
    completion_handler: &block2::Block<
        dyn Fn(NSURLSessionAuthChallengeDisposition, *mut NSURLCredential),
    >,
) {
    // WebKit does not pass an NSURLAuthenticationChallenge. It passes
    // WKNSURLAuthenticationChallenge, an NSProxy that forwards the selector.
    // objc2's debug check requires the method to be in the class method list,
    // so a checked `protectionSpace` send panics before the message is sent.
    // Release builds skip that check. The credential pointer stays null either
    // way: default handling is the system trust evaluation, and cancel sends
    // no password or certificate.
    let challenge = unsafe { &*(challenge as *const _ as *const objc2::runtime::AnyObject) };
    let method = authentication_method(challenge);
    if std::env::var_os("RNDR_BROWSER_AUTH_LOG").is_some() {
        eprintln!("webkit authentication method={method:?}");
    }
    let disposition = match method.as_deref().map(crate::auth::decide_authentication) {
        Some(crate::auth::AuthDecision::Cancel) => {
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengeCancelAuthenticationChallenge
        }
        Some(crate::auth::AuthDecision::PerformDefaultHandling) | None => {
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengePerformDefaultHandling
        }
    };
    completion_handler.call((disposition, std::ptr::null_mut()));
}

fn authentication_method(challenge: &objc2::runtime::AnyObject) -> Option<String> {
    let space = forwarded_object(challenge, sel!(protectionSpace))?;
    let method = forwarded_object(&space, sel!(authenticationMethod))?;
    let is_string: bool = unsafe { msg_send![&*method, isKindOfClass: NSString::class()] };
    if !is_string {
        return None;
    }
    let string =
        unsafe { Retained::<NSString>::retain(Retained::as_ptr(&method).cast_mut().cast()) }?;
    Some(string.to_string())
}

/// Send a no-argument object getter, including one an NSProxy only forwards.
///
/// `class_getInstanceMethod` does not see a forwarded selector, and objc2 0.5
/// panics on that in debug. `objc_msgSend` is the send WebKit's proxy accepts.
/// Other messages stay on the checked path. The caller retains the result.
fn forwarded_object(
    target: &objc2::runtime::AnyObject,
    selector: objc2::runtime::Sel,
) -> Option<Retained<objc2::runtime::AnyObject>> {
    let send: unsafe extern "C" fn(
        *const objc2::runtime::AnyObject,
        objc2::runtime::Sel,
    ) -> *mut objc2::runtime::AnyObject =
        unsafe { std::mem::transmute(objc2::ffi::objc_msgSend as *const ()) };
    let result = unsafe { send(target, selector) };
    if result.is_null() {
        None
    } else {
        unsafe { Retained::retain(result) }
    }
}

fn http_status_requires_credentials(response: &NSURLResponse) -> bool {
    let is_http: bool = unsafe { msg_send![response, isKindOfClass: NSHTTPURLResponse::class()] };
    if !is_http {
        return false;
    }
    let http = unsafe { &*(response as *const NSURLResponse).cast::<NSHTTPURLResponse>() };
    matches!(unsafe { http.statusCode() }, 401 | 407)
}

fn failing_url(error: &NSError) -> Option<String> {
    let info = error.userInfo();
    let string_key = unsafe { NSURLErrorFailingURLStringErrorKey };
    if let Some(url) = dictionary_string(&info, string_key) {
        return Some(url);
    }
    let url_key = unsafe { NSURLErrorFailingURLErrorKey };
    let value = unsafe { info.objectForKey(url_key) }?;
    let is_url: bool = unsafe { msg_send![&*value, isKindOfClass: NSURL::class()] };
    if !is_url {
        return None;
    }
    let url = unsafe { Retained::<NSURL>::retain(Retained::as_ptr(&value).cast_mut().cast()) }?;
    unsafe { url.absoluteString() }.map(|url| url.to_string())
}

fn dictionary_string(
    info: &objc2_foundation::NSDictionary<NSString, objc2::runtime::AnyObject>,
    key: &NSString,
) -> Option<String> {
    let value = unsafe { info.objectForKey(key) }?;
    let is_string: bool = unsafe { msg_send![&*value, isKindOfClass: NSString::class()] };
    if !is_string {
        return None;
    }
    let string =
        unsafe { Retained::<NSString>::retain(Retained::as_ptr(&value).cast_mut().cast()) }?;
    let text = string.to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn snapshot(web_view: &WKWebView, load: LoadOutcome) -> NavigationState {
    NavigationState {
        title: unsafe { web_view.title() }
            .map(|title| title.to_string())
            .unwrap_or_default(),
        url: unsafe { web_view.URL() }
            .and_then(|url| unsafe { url.absoluteString() })
            .map(|url| url.to_string())
            .unwrap_or_default(),
        load,
        can_go_back: unsafe { web_view.canGoBack() },
        can_go_forward: unsafe { web_view.canGoForward() },
    }
}

fn response_url(response: &NSURLResponse) -> String {
    unsafe { response.URL() }
        .and_then(|url| unsafe { url.absoluteString() })
        .map(|url| url.to_string())
        .unwrap_or_default()
}

fn response_mime(response: &NSURLResponse) -> String {
    unsafe { response.MIMEType() }
        .map(|mime| mime.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod challenge_dispatch {
    use std::cell::Cell;
    use std::rc::Rc;

    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, Sel};
    use objc2::{declare_class, msg_send_id, sel, ClassType, DeclaredClass};
    use objc2_foundation::{
        NSInvocation, NSMethodSignature, NSObject, NSObjectProtocol, NSString,
        NSURLAuthenticationChallenge, NSURLAuthenticationChallengeSender,
        NSURLAuthenticationMethodHTTPBasic, NSURLAuthenticationMethodServerTrust, NSURLCredential,
        NSURLProtectionSpace, NSURLSessionAuthChallengeDisposition,
    };

    use super::answer_authentication;

    struct ProbeIvars {
        space: Retained<NSURLProtectionSpace>,
    }

    declare_class!(
        struct AuthProbe;

        unsafe impl ClassType for AuthProbe {
            type Super = NSObject;
            type Mutability = objc2::mutability::InteriorMutable;
            const NAME: &'static str = "RndrBrowserAuthProbe";
        }

        impl DeclaredClass for AuthProbe {
            type Ivars = ProbeIvars;
        }

        unsafe impl NSObjectProtocol for AuthProbe {}

        unsafe impl AuthProbe {
            #[method_id(methodSignatureForSelector:)]
            fn method_signature(&self, selector: Sel) -> Option<Retained<NSMethodSignature>> {
                if selector == sel!(protectionSpace) {
                    foundation_signature(selector)
                } else {
                    None
                }
            }

            #[method(forwardInvocation:)]
            unsafe fn forward_invocation(&self, invocation: &NSInvocation) {
                if invocation.selector() != sel!(protectionSpace) {
                    return;
                }
                let pointer: *mut AnyObject =
                    Retained::as_ptr(&self.ivars().space).cast_mut().cast();
                let mut slot = pointer;
                invocation.setReturnValue(std::ptr::NonNull::from(&mut slot).cast());
            }
        }
    );

    struct SenderIvars;

    declare_class!(
        struct AuthSender;

        unsafe impl ClassType for AuthSender {
            type Super = NSObject;
            type Mutability = objc2::mutability::InteriorMutable;
            const NAME: &'static str = "RndrBrowserAuthSender";
        }

        impl DeclaredClass for AuthSender {
            type Ivars = SenderIvars;
        }

        unsafe impl NSObjectProtocol for AuthSender {}

        unsafe impl NSURLAuthenticationChallengeSender for AuthSender {
            #[method(useCredential:forAuthenticationChallenge:)]
            unsafe fn use_credential(
                &self,
                _credential: &NSURLCredential,
                _challenge: &NSURLAuthenticationChallenge,
            ) {
            }

            #[method(continueWithoutCredentialForAuthenticationChallenge:)]
            unsafe fn continue_without_credential(
                &self,
                _challenge: &NSURLAuthenticationChallenge,
            ) {
            }

            #[method(cancelAuthenticationChallenge:)]
            unsafe fn cancel_challenge(&self, _challenge: &NSURLAuthenticationChallenge) {}
        }
    );

    fn foundation_signature(selector: Sel) -> Option<Retained<NSMethodSignature>> {
        unsafe {
            msg_send_id![
                NSURLAuthenticationChallenge::class(),
                instanceMethodSignatureForSelector: selector
            ]
        }
    }

    fn protection_space(method: &NSString) -> Retained<NSURLProtectionSpace> {
        unsafe {
            NSURLProtectionSpace::initWithHost_port_protocol_realm_authenticationMethod(
                NSURLProtectionSpace::alloc(),
                &NSString::from_str("example.com"),
                443,
                Some(&NSString::from_str("https")),
                None,
                Some(method),
            )
        }
    }

    fn probe(method: &NSString) -> Retained<AuthProbe> {
        let this = AuthProbe::alloc().set_ivars(ProbeIvars {
            space: protection_space(method),
        });
        unsafe { msg_send_id![super(this), init] }
    }

    fn sender() -> Retained<AuthSender> {
        let this = AuthSender::alloc().set_ivars(SenderIvars);
        unsafe { msg_send_id![super(this), init] }
    }

    fn dispatch(challenge: &impl objc2::Message) -> NSURLSessionAuthChallengeDisposition {
        let seen = Rc::new(Cell::new(
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengeUseCredential,
        ));
        let record = Rc::clone(&seen);
        let block = RcBlock::new(
            move |disposition: NSURLSessionAuthChallengeDisposition, _: *mut NSURLCredential| {
                record.set(disposition);
            },
        );
        answer_authentication(challenge, &block);
        seen.get()
    }

    #[test]
    fn proxy_challenge_is_dispatched_without_a_debug_abort() {
        let server_trust = unsafe { NSURLAuthenticationMethodServerTrust };
        let proxy = probe(server_trust);
        assert!(
            AuthProbe::class()
                .instance_method(sel!(protectionSpace))
                .is_none(),
            "the probe must not list protectionSpace, matching WebKit's NSProxy"
        );
        assert_eq!(
            dispatch(&*proxy),
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengePerformDefaultHandling
        );

        let basic = unsafe { NSURLAuthenticationMethodHTTPBasic };
        assert_eq!(
            dispatch(&*probe(basic)),
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengeCancelAuthenticationChallenge
        );
    }

    #[test]
    fn foundation_challenge_uses_the_same_helper() {
        let server_trust = unsafe { NSURLAuthenticationMethodServerTrust };
        let space = protection_space(server_trust);
        let owner = sender();
        let challenge = unsafe {
            NSURLAuthenticationChallenge::initWithProtectionSpace_proposedCredential_previousFailureCount_failureResponse_error_sender(
                NSURLAuthenticationChallenge::alloc(),
                &space,
                None,
                0,
                None,
                None,
                objc2::runtime::ProtocolObject::from_ref(&*owner),
            )
        };
        assert_eq!(
            dispatch(&*challenge),
            NSURLSessionAuthChallengeDisposition::NSURLSessionAuthChallengePerformDefaultHandling
        );
    }
}
