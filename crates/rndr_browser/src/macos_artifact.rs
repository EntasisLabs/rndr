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
use crate::macos_artifact_document::{self as document, DocumentHandler};
use crate::{ArtifactError, ArtifactState, ClientRect, LoadOutcome};
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2::{msg_send_id, ClassType};
use objc2_foundation::NSArray;
use objc2_web_kit::WKPreferences;

const POLICY_TIMEOUT: Duration = Duration::from_secs(15);
type Compiled = Rc<RefCell<Option<Result<Retained<WKContentRuleList>, String>>>>;

pub(crate) struct ArtifactWebKit {
    // Drop the monitor (and its retained view capture) before retiring WebKit.
    probe: crate::macos_artifact_diagnostics::Probe,
    last_diagnostic_geometry: Option<(ClientRect, bool, f64, isize)>,
    last_diagnostic_state: Option<Result<ArtifactState, ArtifactError>>,
    _paste_guard: crate::macos_artifact_input::PasteGuard,
    overlays: crate::macos_artifact_overlay::OverlayPlane,
    view: WebKit,
    compiled: Compiled,
    pending_html: Option<String>,
    document: Retained<DocumentHandler>,
    preflight_started: bool,
    source_started: bool,
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
                require_disabled(
                    preferences.respondsToSelector(sel!($setter:))
                        && preferences.respondsToSelector(sel!($getter)),
                    || unsafe {
                        let _: () = msg_send![&preferences, $setter: false];
                        msg_send![&preferences, $getter]
                    },
                )?;
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
        // Main-document embedding must not expose gesture-activated async
        // clipboard or payment capabilities previously denied by iframe policy.
        // These are security gates, never scroll/rendering preferences.
        disable_security_feature(&preferences, "AsyncClipboardAPIEnabled")?;
        // WebKit's top-document speculative preconnect can open sockets before
        // CSP/content-blocker denial. Disable that security capability outright.
        disable_security_feature(&preferences, "LinkPreconnect")?;
        disable!(_setApplePayEnabled, _applePayEnabled, "payments");
        require_disabled(
            preferences.respondsToSelector(sel!(setElementFullscreenEnabled:))
                && preferences.respondsToSelector(sel!(isElementFullscreenEnabled)),
            || unsafe {
                preferences.setElementFullscreenEnabled(false);
                preferences.isElementFullscreenEnabled()
            },
        )?;
        unsafe {
            configuration.setWebsiteDataStore(&WKWebsiteDataStore::nonPersistentDataStore());
            configuration.setProcessPool(&WKProcessPool::new());
            configuration.setUserContentController(&WKUserContentController::new());
            configuration
                .preferences()
                .setJavaScriptCanOpenWindowsAutomatically(false);
            configuration.setAllowsAirPlayForMediaPlayback(false);
        }
        let document = DocumentHandler::new();
        unsafe {
            configuration.setURLSchemeHandler_forURLScheme(
                Some(ProtocolObject::from_ref(&*document)),
                &NSString::from_str(document::SCHEME),
            );
        }
        let view =
            WebKit::with_configuration(parent, &configuration, true).map_err(
                |error| match error {
                    crate::BrowserError::Unsupported { .. } => ArtifactError::Unsupported,
                    other => ArtifactError::Browser(other),
                },
            )?;
        let probe = crate::macos_artifact_diagnostics::Probe::new(view.web.clone());
        view.set_artifact_trace(probe.trace.clone());
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
        // The shared compiler cache contains only these constant rules, never HTML
        // or website data. Each view/controller/data store remains independent.
        let store =
            unsafe { WKContentRuleListStore::defaultStore() }.ok_or(ArtifactError::Unsupported)?;
        unsafe {
            store.compileContentRuleListForIdentifier_encodedContentRuleList_completionHandler(
                Some(&NSString::from_str(
                    "rndr-artifact-in-memory-main-document-v2",
                )),
                Some(&NSString::from_str(document::RESOURCE_RULES)),
                Some(&completion),
            );
        }
        Ok(Self {
            probe,
            last_diagnostic_geometry: None,
            last_diagnostic_state: None,
            _paste_guard: paste_guard,
            overlays: Default::default(),
            view,
            compiled,
            pending_html: Some(crate::artifact::document_html(html)),
            document,
            preflight_started: false,
            source_started: false,
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
        overlays_revision: u64,
        overlays: &[crate::OverlayBitmap],
    ) -> Result<ArtifactState, ArtifactError> {
        let state = self.advance();
        if let Some(trace) = &self.probe.trace {
            let window = self.view.web.window();
            let metrics = (
                bounds,
                visible,
                window
                    .as_ref()
                    .map(|w| w.backingScaleFactor())
                    .unwrap_or(0.0),
                window
                    .as_ref()
                    .map(|w| unsafe { w.windowNumber() })
                    .unwrap_or(0),
            );
            if self.last_diagnostic_geometry != Some(metrics) {
                trace.geometry(metrics.0, metrics.1, metrics.2, metrics.3);
                self.last_diagnostic_geometry = Some(metrics);
            }
            if self.last_diagnostic_state.as_ref() != Some(&state) {
                trace.lifecycle(match &state {
                    Ok(ArtifactState::Preparing) => "preparing",
                    Ok(ArtifactState::Loading) => "loading",
                    Ok(ArtifactState::Ready) => "ready",
                    Err(_) => "failed",
                });
                self.last_diagnostic_state = Some(state.clone());
            }
        }
        self.view.apply(
            bounds,
            visible && state.is_ok() && state != Ok(ArtifactState::Preparing),
        );
        if state.is_err() {
            self.overlays.clear();
            return state;
        }
        self.overlays.apply(
            &self.view.parent,
            overlays_revision,
            overlays,
            visible && state.is_ok() && state != Ok(ArtifactState::Preparing),
        )?;
        state
    }

    fn advance(&mut self) -> Result<ArtifactState, ArtifactError> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        if !self.preflight_started {
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
                    self.preflight_started = true;
                    self.view.load_artifact_document(document::PREFLIGHT_URL);
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
        if !self.source_started {
            match self.view.artifact_load() {
                LoadOutcome::Finished => {
                    // Only our trusted, source-free preflight can run here.
                    // Without response-header sandbox enforcement the custom
                    // scheme has a nonopaque origin and localStorage succeeds.
                    let title = unsafe { self.view.web.title() }.map(|s| s.to_string());
                    if title.as_deref() != Some(document::VERIFIED_TITLE) {
                        return self.fail(ArtifactError::Unsupported);
                    }
                    if !self
                        .document
                        .arm_document(self.pending_html.take().expect("pending HTML"))
                    {
                        return self.fail(ArtifactError::Policy(
                            "document transport already retired or consumed".into(),
                        ));
                    }
                    self.source_started = true;
                    self.view.load_artifact_document(document::DOCUMENT_URL);
                }
                LoadOutcome::Failed | LoadOutcome::Stopped => {
                    return self.fail(ArtifactError::LoadFailed)
                }
                _ if self.started.elapsed() >= POLICY_TIMEOUT => {
                    return self.fail(ArtifactError::Policy(
                        "sandbox preflight timed out after 15 seconds".into(),
                    ))
                }
                _ => return Ok(ArtifactState::Preparing),
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
        self.document.retire();
        self.overlays.clear();
        self.view.shutdown();
        self.failed = Some(error.clone());
        Err(error)
    }
}

// WebKit exposes the async clipboard switch only in its guarded feature SPI.
// No KVC, JS shadowing, default fallback, or renderer setting is involved.
fn disable_security_feature(preferences: &WKPreferences, key: &str) -> Result<(), ArtifactError> {
    let available: bool =
        unsafe { msg_send![WKPreferences::class(), respondsToSelector: sel!(_features)] };
    if !available
        || !preferences.respondsToSelector(sel!(_setEnabled:forFeature:))
        || !preferences.respondsToSelector(sel!(_isEnabledForFeature:))
    {
        return Err(ArtifactError::Unsupported);
    }
    let features: Retained<NSArray<AnyObject>> =
        unsafe { msg_send_id![WKPreferences::class(), _features] };
    for index in 0..features.len() {
        let feature = unsafe { features.objectAtIndex(index) };
        let has_key: bool = unsafe { msg_send![&feature, respondsToSelector: sel!(key)] };
        if !has_key {
            continue;
        }
        let name: Retained<NSString> = unsafe { msg_send_id![&feature, key] };
        if name.to_string() == key {
            return require_disabled(true, || unsafe {
                let _: () = msg_send![preferences, _setEnabled: false, forFeature: &*feature];
                msg_send![preferences, _isEnabledForFeature: &*feature]
            });
        }
    }
    Err(ArtifactError::Unsupported)
}

// No test/runtime override: production always queries the real controls and
// reads back their state. Missing or ineffective gates never arm an HTML load.
fn require_disabled(
    available: bool,
    disable_and_read: impl FnOnce() -> bool,
) -> Result<(), ArtifactError> {
    if !available || disable_and_read() {
        Err(ArtifactError::Unsupported)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unavailable_control_refuses_before_invoking_native_or_loading_html() {
        assert_eq!(
            require_disabled(false, || panic!("must not invoke unavailable control")),
            Err(ArtifactError::Unsupported)
        );
        assert_eq!(
            require_disabled(true, || true),
            Err(ArtifactError::Unsupported)
        );
        assert_eq!(require_disabled(true, || false), Ok(()));
    }
}
