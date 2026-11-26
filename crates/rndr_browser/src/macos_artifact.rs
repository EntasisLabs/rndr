//! macOS-only artifact construction. Ordinary Browser never uses these settings.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{msg_send, sel};
use objc2_app_kit::NSView;
use objc2_foundation::NSObjectProtocol;
use objc2_foundation::{NSError, NSString};
use objc2_web_kit::{
    WKContentRuleList, WKContentRuleListStore, WKProcessPool, WKUserContentController,
    WKWebViewConfiguration, WKWebsiteDataStore,
};
use raw_window_handle::HasWindowHandle;

use crate::macos::WebKit;
use crate::{ArtifactError, ArtifactState, ClientRect, LoadOutcome};

// Independent of markup and CSP, WebKit rejects every resource request. Srcdoc
// and the host's HTML string do not require resource requests. Do not add URL
// exceptions (including localhost, custom schemes, data or blob).
const DENY_RESOURCES: &str = r#"[{"trigger":{"url-filter":".*"},"action":{"type":"block"}}]"#;
const POLICY_TIMEOUT: Duration = Duration::from_secs(15);
type Compiled = Rc<RefCell<Option<Result<Retained<WKContentRuleList>, String>>>>;

pub(crate) struct ArtifactWebKit {
    // Drop the monitor (and its retained view capture) before retiring WebKit.
    _paste_guard: crate::macos_artifact_input::PasteGuard,
    view: WebKit,
    compiled: Compiled,
    pending_html: Option<String>,
    started: Instant,
    failed: Option<ArtifactError>,
}

impl ArtifactWebKit {
    pub(crate) fn attach(parent: &impl HasWindowHandle, html: &str) -> Result<Self, ArtifactError> {
        Self::new(
            WebKit::retain_parent(parent).map_err(ArtifactError::Browser)?,
            html,
        )
    }

    fn new(parent: Retained<NSView>, html: &str) -> Result<Self, ArtifactError> {
        let configuration = unsafe { WKWebViewConfiguration::new() };
        // WebRTC's ICE/STUN traffic is not a resource request and is not
        // covered by CSP connect-src or content rules. WebKit currently exposes
        // no public per-view switch. Require its engine preference SPI and
        // refuse the entire feature if either accessor is absent or ineffective.
        // This deliberately does NOT fall back to JS API shadowing.
        let preferences = unsafe { configuration.preferences() };
        macro_rules! disable {
            ($setter:ident, $getter:ident, $name:literal) => {{
                if !preferences.respondsToSelector(sel!($setter:))
                    || !preferences.respondsToSelector(sel!($getter)) {
                    return Err(ArtifactError::Policy(concat!("WebKit lacks required native gate: ", $name).into()));
                }
                let enabled: bool = unsafe {
                    let _: () = msg_send![&preferences, $setter: false];
                    msg_send![&preferences, $getter]
                };
                if enabled {
                    return Err(ArtifactError::Policy(concat!("WebKit refused to disable ", $name).into()));
                }
            }};
        }
        disable!(_setPeerConnectionEnabled, _peerConnectionEnabled, "WebRTC");
        disable!(
            _setDNSPrefetchingEnabled,
            _dnsPrefetchingEnabled,
            "DNS prefetch"
        );
        disable!(
            _setFileSystemAccessEnabled,
            _fileSystemAccessEnabled,
            "filesystem API"
        );
        disable!(
            _setJavaScriptCanAccessClipboard,
            _javaScriptCanAccessClipboard,
            "script clipboard access"
        );
        disable!(_setDOMPasteAllowed, _domPasteAllowed, "DOM paste");
        unsafe {
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore());
            configuration.setProcessPool(&WKProcessPool::new());
            configuration.setUserContentController(&WKUserContentController::new());
            configuration
                .preferences()
                .setJavaScriptCanOpenWindowsAutomatically(false);
            configuration.setAllowsAirPlayForMediaPlayback(false);
        }
        let view = WebKit::with_configuration(parent, &configuration, true)
            .map_err(ArtifactError::Browser)?;
        let paste_guard = crate::macos_artifact_input::PasteGuard::new(view.web.clone())?;
        let compiled: Compiled = Rc::new(RefCell::new(None));
        // The async callback owns neither view nor HTML. Dropped/replaced
        // surfaces cannot later load from a late compiler completion.
        let weak = Rc::downgrade(&compiled);
        let completion = RcBlock::new(move |rule: *mut WKContentRuleList, error: *mut NSError| {
            let Some(result) = weak.upgrade() else {
                return;
            };
            let value = if !error.is_null() {
                Err(unsafe { &*error }.localizedDescription().to_string())
            } else if let Some(rule) = unsafe { Retained::retain(rule) } {
                Ok(rule)
            } else {
                Err("WebKit returned no content rule list".into())
            };
            *result.borrow_mut() = Some(value);
        });
        // The shared compiler cache contains only this constant rule, never HTML
        // or website data. Each view/controller/data store remains independent.
        let store = unsafe { WKContentRuleListStore::defaultStore() }
            .ok_or_else(|| ArtifactError::Policy("content rule compiler unavailable".into()))?;
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&NSString::from_str("rndr-artifact-deny-all-v1")),
                Some(&NSString::from_str(DENY_RESOURCES)),
                Some(&completion),
            );
        }
        Ok(Self {
            _paste_guard: paste_guard,
            view,
            compiled,
            pending_html: Some(crate::artifact::envelope(html)),
            started: Instant::now(),
            failed: None,
        })
    }

    pub(crate) fn replacement(&self, html: &str) -> Result<Self, ArtifactError> {
        Self::new(self.view.parent.clone(), html)
    }

    pub(crate) fn frame(
        &mut self,
        bounds: ClientRect,
        visible: bool,
    ) -> Result<ArtifactState, ArtifactError> {
        let state = self.advance();
        self.view.apply(
            bounds,
            visible && state.is_ok() && state != Ok(ArtifactState::Preparing),
        );
        state
    }

    fn advance(&mut self) -> Result<ArtifactState, ArtifactError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if self.pending_html.is_some() {
            let compiled = self.compiled.borrow_mut().take();
            match compiled {
                Some(Ok(rule)) => {
                    unsafe {
                        self.view
                            .web
                            .configuration()
                            .userContentController()
                            .addContentRuleList(&rule);
                    }
                    let html = self.pending_html.take().expect("pending HTML");
                    self.view.load_artifact_envelope(&html);
                }
                Some(Err(message)) => return self.fail(ArtifactError::Policy(message)),
                None if self.started.elapsed() >= POLICY_TIMEOUT => {
                    return self.fail(ArtifactError::Policy(
                        "compiler timed out after 15 seconds".into(),
                    ))
                }
                None => return Ok(ArtifactState::Preparing),
            }
        }
        match self.view.artifact_load() {
            LoadOutcome::Finished => Ok(ArtifactState::Ready),
            LoadOutcome::Failed | LoadOutcome::Stopped => self.fail(ArtifactError::LoadFailed),
            _ => Ok(ArtifactState::Loading),
        }
    }

    fn fail(&mut self, error: ArtifactError) -> Result<ArtifactState, ArtifactError> {
        self.pending_html = None;
        self.view.shutdown();
        self.failed = Some(error.clone());
        Err(error)
    }
}
