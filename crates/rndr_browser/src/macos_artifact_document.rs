//! Single-use, in-memory main-document delivery with a response-header sandbox.
//!
//! There is no dispatcher, URL/file/network resolver, or guest command channel.
//! Even the two internal URLs may each deliver bytes only once per fresh view.

use std::cell::RefCell;

use objc2::mutability::MainThreadOnly;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{declare_class, msg_send_id, ClassType, DeclaredClass};
use objc2_foundation::{
    MainThreadMarker, NSData, NSDictionary, NSError, NSHTTPURLResponse, NSObject, NSObjectProtocol,
    NSString,
};
use objc2_web_kit::{WKURLSchemeHandler, WKURLSchemeTask, WKWebView};

pub(crate) const SCHEME: &str = "rndr-artifact";
pub(crate) const PREFLIGHT_URL: &str = "rndr-artifact://preview/preflight";
pub(crate) const DOCUMENT_URL: &str = "rndr-artifact://preview/document";
pub(crate) const VERIFIED_TITLE: &str = "rndr-artifact-sandbox-verified";
// A sandbox directive in a meta element is NOT effective. This policy is always
// supplied as an immutable response header, before any authored bytes parse.
pub(crate) const CSP: &str = "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-src 'none'; connect-src 'none'; font-src 'none'; media-src 'none'; object-src 'none'; worker-src 'none'; base-uri 'none'; form-action 'none'";
// Only these two in-memory responses pass the engine's resource block. All
// subsequent requests, including fetches of these exact URLs, fail in Transport.
pub(crate) const RESOURCE_RULES: &str = r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}},{"trigger":{"url-filter":"^rndr-artifact://preview/preflight$"},"action":{"type":"ignore-previous-rules"}},{"trigger":{"url-filter":"^rndr-artifact://preview/document$"},"action":{"type":"ignore-previous-rules"}}]"#;
const PREFLIGHT: &str = r#"<!doctype html><meta charset="utf-8"><script>
(()=>{try{let blocked=false;try{void localStorage}catch(e){blocked=e.name==='SecurityError'}if(self.origin==='null'&&blocked)document.title='rndr-artifact-sandbox-verified'}catch(e){}})();
</script>"#;

#[derive(Default)]
struct Transport {
    preflight: bool,
    document: Option<String>,
    armed: bool,
    retired: bool,
}
impl Transport {
    fn arm(&mut self, html: String) -> bool {
        if self.retired || !self.preflight || self.armed {
            return false;
        }
        self.armed = true;
        self.document = Some(html);
        true
    }
    fn take(&mut self, url: &str, method: &str) -> Option<String> {
        if self.retired || method != "GET" {
            return None;
        }
        match url {
            PREFLIGHT_URL if !self.preflight => {
                self.preflight = true;
                Some(PREFLIGHT.into())
            }
            DOCUMENT_URL if self.preflight => self.document.take(),
            _ => None,
        }
    }
}
pub(crate) struct Ivars {
    transport: RefCell<Transport>,
}

declare_class!(
    pub(crate) struct DocumentHandler;
    unsafe impl ClassType for DocumentHandler {
        type Super = NSObject;
        type Mutability = MainThreadOnly;
        const NAME: &'static str = "RndrArtifactDocumentHandler";
    }
    impl DeclaredClass for DocumentHandler { type Ivars = Ivars; }
    unsafe impl NSObjectProtocol for DocumentHandler {}
    unsafe impl WKURLSchemeHandler for DocumentHandler {
        #[method(webView:startURLSchemeTask:)]
        unsafe fn start(&self, _web: &WKWebView, task: &ProtocolObject<dyn WKURLSchemeTask>) {
            let request = unsafe { task.request() };
            let url = unsafe { request.URL() };
            let text = url.as_ref().and_then(|u| unsafe { u.absoluteString() }).map(|s|s.to_string()).unwrap_or_default();
            let method = unsafe { request.HTTPMethod() }.map(|s|s.to_string()).unwrap_or_default();
            // Take before any callback. No retained task or asynchronous response
            // exists for stop/drop/replacement to resurrect or deliver twice.
            let bytes = self.ivars().transport.borrow_mut().take(&text, &method);
            if let (Some(bytes), Some(url)) = (bytes, url) {
                let keys = [NSString::from_str("Content-Type"), NSString::from_str("Content-Security-Policy"), NSString::from_str("Referrer-Policy"), NSString::from_str("Cache-Control"), NSString::from_str("Permissions-Policy")];
                let values = [NSString::from_str("text/html; charset=utf-8"), NSString::from_str(CSP), NSString::from_str("no-referrer"), NSString::from_str("no-store"), NSString::from_str("camera=(), microphone=(), geolocation=(), display-capture=(), clipboard-read=(), clipboard-write=(), fullscreen=(), payment=(), usb=()")];
                let headers = NSDictionary::from_id_slice(&keys.iter().map(|v|&**v).collect::<Vec<_>>(), &values);
                if let Some(response) = unsafe { NSHTTPURLResponse::initWithURL_statusCode_HTTPVersion_headerFields(NSHTTPURLResponse::alloc(), &url, 200, Some(&NSString::from_str("HTTP/1.1")), Some(&headers)) } {
                    unsafe {
                        task.didReceiveResponse(&response);
                        task.didReceiveData(&NSData::with_bytes(bytes.as_bytes()));
                        task.didFinish();
                    }
                    return;
                }
            }
            unsafe { task.didFailWithError(&NSError::errorWithDomain_code_userInfo(&NSString::from_str("RndrArtifactDocument"), 1, None)); }
        }
        #[method(webView:stopURLSchemeTask:)]
        unsafe fn stop(&self, _web: &WKWebView, _task: &ProtocolObject<dyn WKURLSchemeTask>) {
            // Responses are synchronous; there is no work left to cancel.
        }
    }
);
impl DocumentHandler {
    pub(crate) fn new() -> Retained<Self> {
        let marker = MainThreadMarker::new().expect("WebKit UI thread");
        unsafe {
            msg_send_id![
                super(marker.alloc::<Self>().set_ivars(Ivars {
                    transport: RefCell::new(Transport::default())
                })),
                init
            ]
        }
    }
    // Called only after the trusted preflight finished and proved header sandbox
    // enforcement. Until then the transport owns NO authored source bytes.
    pub(crate) fn arm_document(&self, html: String) -> bool {
        self.ivars().transport.borrow_mut().arm(html)
    }
    pub(crate) fn retire(&self) {
        let mut transport = self.ivars().transport.borrow_mut();
        transport.retired = true;
        transport.document = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_is_exact_get_only_single_use_and_source_requires_native_arm() {
        let mut t = Transport::default();
        for url in [
            DOCUMENT_URL,
            "file:///etc/passwd",
            "http://localhost",
            "rndr-artifact://preview/preflight?x",
            "rndr-artifact://preview/preflight#x",
        ] {
            assert!(t.take(url, "GET").is_none());
        }
        assert!(t.take(PREFLIGHT_URL, "POST").is_none());
        assert!(t.take(PREFLIGHT_URL, "GET").is_some());
        assert!(t.take(PREFLIGHT_URL, "GET").is_none());
        assert!(t.take(DOCUMENT_URL, "GET").is_none());
        assert!(t.arm("authored".into()));
        assert!(!t.arm("second".into()));
        assert!(t.take(DOCUMENT_URL, "POST").is_none());
        assert_eq!(t.take(DOCUMENT_URL, "GET").as_deref(), Some("authored"));
        assert!(t.take(DOCUMENT_URL, "GET").is_none());
        assert!(!t.arm("rearmed".into()));
        t.retired = true;
        assert!(t.take(PREFLIGHT_URL, "GET").is_none());
    }
}
