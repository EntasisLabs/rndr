//! A deliberately capability-free surface, separate from ordinary browsing.

use crate::{admit_client_rect, BrowserError, ClientRect, RectError};
use raw_window_handle::HasWindowHandle;
use std::fmt;

/// Maximum UTF-8 source bytes accepted by an artifact preview (2 MiB).
/// The envelope may use up to six times this space while escaping attributes.
pub const MAX_ARTIFACT_HTML_BYTES: usize = 2 * 1024 * 1024;

/// Failure to admit, isolate, or load an artifact. There is no browser fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ArtifactError {
    /// The UTF-8 source exceeded [`MAX_ARTIFACT_HTML_BYTES`].
    TooLarge,
    /// This backend cannot enforce the artifact contract.
    Unsupported,
    /// Native view creation failed.
    Browser(BrowserError),
    /// Invalid bounds; the previous bounds remain unchanged.
    Rect(RectError),
    /// The required platform policy failed or timed out. No HTML was loaded.
    Policy(String),
    /// The native load failed or the web content process terminated.
    LoadFailed,
}

impl fmt::Display for ArtifactError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(
                f,
                "artifact HTML exceeds {MAX_ARTIFACT_HTML_BYTES} UTF-8 bytes"
            ),
            Self::Unsupported => {
                f.write_str("isolated HTML artifacts are unsupported on this backend")
            }
            Self::Browser(error) => write!(f, "artifact view: {error}"),
            Self::Rect(error) => write!(f, "{error}"),
            Self::Policy(error) => write!(f, "artifact policy was not installed: {error}"),
            Self::LoadFailed => {
                f.write_str("artifact load failed or its content process terminated")
            }
        }
    }
}
impl std::error::Error for ArtifactError {}

/// State of the trusted envelope's load, not a claim that guest scripts finished.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactState {
    /// Native deny-all resource policy is compiling. No artifact is loaded yet.
    Preparing,
    /// Policy is installed and the envelope is loading.
    Loading,
    /// The envelope finished loading. Local scripts may continue running.
    Ready,
}

/// Host-owned, isolated, in-memory HTML preview.
///
/// Unlike [`crate::Browser`], this has no navigation, script evaluation, file,
/// download, or guest-message API. Inline JS, inline CSS and data images can
/// work; all other resource types are denied. A platform-enforced sandbox gives
/// the guest an opaque origin, without storage, forms, popups, downloads, native
/// commands or permissions. External navigation and network are denied.
///
/// The strict macOS implementation is off by default. Explicitly enable Cargo
/// feature `macos-private-webkit-artifacts`; it requires **private WebKit SPI** for
/// network and lifecycle enforcement. Missing gates are errors before loading
/// HTML; this is not an App Store-ready/public-API-only backend. See the crate's
/// `docs/isolated-artifacts.md` for the policy and deployment limitations.
///
/// Opted-in macOS uses a new ephemeral WKWebView for every replacement. Feature-off
/// builds and other backends return [`ArtifactError::Unsupported`], never an ordinary browser. Use on the
/// platform UI thread and drive [`Self::frame`] alongside the native event loop.
/// Drop retires the surface; replacement discards the previous JS/DOM/state.
/// This is not a CPU/memory quota or a defense against web-engine vulnerabilities.
pub struct ArtifactPreview {
    #[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
    engine: crate::macos_artifact::ArtifactWebKit,
    bounds: ClientRect,
    visible: bool,
}

impl ArtifactPreview {
    /// Create a hidden, fresh preview. Empty HTML is valid. Admission is bounded
    /// before native side effects; async policy failures surface in `frame`.
    pub fn attach(parent: &impl HasWindowHandle, html: &str) -> Result<Self, ArtifactError> {
        validate(html)?;
        #[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
        {
            Ok(Self {
                engine: crate::macos_artifact::ArtifactWebKit::attach(parent, html)?,
                bounds: ClientRect::try_new(0.0, 0.0, 0.0, 0.0).expect("zero rect"),
                visible: false,
            })
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
        {
            let _ = parent;
            Err(ArtifactError::Unsupported)
        }
    }

    /// Replace with a fresh isolated session, retaining bounds and visibility.
    /// Admission/creation errors leave the previous preview intact. A subsequent
    /// policy/load error leaves the new preview failed, never restores old state.
    pub fn replace(&mut self, html: &str) -> Result<(), ArtifactError> {
        validate(html)?;
        #[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
        {
            self.engine = self.engine.replacement(html)?;
            Ok(())
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
        {
            Err(ArtifactError::Unsupported)
        }
    }

    /// Submit logical top-left bounds. A zero-area rect hides the surface.
    pub fn set_bounds(&mut self, bounds: ClientRect) -> Result<(), ArtifactError> {
        self.bounds = admit_client_rect(bounds.x, bounds.y, bounds.width, bounds.height)
            .map_err(ArtifactError::Rect)?;
        Ok(())
    }

    /// Show on the next frame, provided the submitted bounds have area.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide on the next frame without resetting the current local interaction.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Advance policy setup, apply bounds/visibility and report load state.
    /// A policy error is sticky until replacement; the failed surface is hidden.
    pub fn frame(&mut self) -> Result<ArtifactState, ArtifactError> {
        #[cfg(all(target_os = "macos", feature = "macos-private-webkit-artifacts"))]
        {
            self.engine.frame(
                self.bounds,
                self.visible && self.bounds.width > 0.0 && self.bounds.height > 0.0,
            )
        }
        #[cfg(not(all(target_os = "macos", feature = "macos-private-webkit-artifacts")))]
        {
            Err(ArtifactError::Unsupported)
        }
    }
}

fn validate(html: &str) -> Result<(), ArtifactError> {
    if html.len() > MAX_ARTIFACT_HTML_BYTES {
        Err(ArtifactError::TooLarge)
    } else {
        Ok(())
    }
}

// Trusted policy, outside guest control. A srcdoc inherits the containing CSP;
// a guest meta policy can only tighten it. The sandbox attribute is not in the
// guest document, so document.write, markup repair and JS cannot remove it.
#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
pub(crate) fn envelope(html: &str) -> String {
    let mut escaped = String::with_capacity(html.len());
    for c in html.chars() {
        match c {
            '&' => escaped.push_str("&amp;"),
            '"' => escaped.push_str("&quot;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(c),
        }
    }
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-src 'self'; connect-src 'none'; font-src 'none'; media-src 'none'; object-src 'none'; worker-src 'none'; base-uri 'none'; form-action 'none'"><meta name="referrer" content="no-referrer"><style>html,body{{margin:0;width:100%;height:100%;overflow:hidden}}iframe{{border:0;width:100%;height:100%}}</style></head><body><iframe sandbox="allow-scripts" allow="camera 'none'; microphone 'none'; geolocation 'none'; display-capture 'none'; clipboard-read 'none'; clipboard-write 'none'; fullscreen 'none'; payment 'none'; usb 'none'" referrerpolicy="no-referrer" srcdoc="{escaped}"></iframe></body></html>"#
    )
}

// Only the trusted host may arm a load. Each view accepts one envelope and
// its one initial direct srcdoc child; there are no subsequent navigations.
#[cfg(any(test, target_os = "macos"))]
#[derive(Default)]
pub(crate) struct ArtifactNavigation {
    envelope: bool,
    srcdoc: bool,
}

#[cfg(any(test, target_os = "macos"))]
impl ArtifactNavigation {
    #[cfg(any(test, feature = "macos-private-webkit-artifacts"))]
    pub(crate) fn arm(&mut self) {
        self.envelope = true;
        self.srcdoc = true;
    }
    pub(crate) fn admit(
        &mut self,
        url: &str,
        target_main: Option<bool>,
        source_main: bool,
        download: bool,
    ) -> bool {
        if download {
            return false;
        }
        if url == "about:blank" && target_main == Some(true) && self.envelope {
            self.envelope = false;
            return true;
        }
        if url == "about:srcdoc"
            && target_main == Some(false)
            && source_main
            && !self.envelope
            && self.srcdoc
        {
            self.srcdoc = false;
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_navigation_gate_accepts_only_host_envelope_and_initial_child() {
        let mut gate = ArtifactNavigation::default();
        assert!(!gate.admit("about:blank", Some(true), true, false));
        gate.arm();
        for url in [
            "https://example.com",
            "http://localhost:9999",
            "file:///etc/passwd",
            "data:text/html,hello",
            "blob:opaque",
            "medousa:invoke",
            "javascript:alert(1)",
            "about:blank#hash",
        ] {
            for target in [None, Some(true), Some(false)] {
                assert!(!gate.admit(url, target, true, false), "{url} {target:?}");
            }
        }
        assert!(!gate.admit("about:blank", Some(true), true, true));
        assert!(!gate.admit("about:blank", None, true, false));
        assert!(!gate.admit("about:srcdoc", Some(false), true, false));
        assert!(gate.admit("about:blank", Some(true), true, false));
        assert!(!gate.admit("about:blank", Some(true), true, false));
        assert!(!gate.admit("about:srcdoc", Some(false), false, false));
        assert!(gate.admit("about:srcdoc", Some(false), true, false));
        assert!(!gate.admit("about:srcdoc", Some(false), true, false));
    }
    #[test]
    fn bounds_are_utf8_bytes_and_empty_is_valid() {
        assert_eq!(validate(""), Ok(()));
        assert_eq!(validate(&"x".repeat(MAX_ARTIFACT_HTML_BYTES)), Ok(()));
        assert_eq!(
            validate(&"é".repeat(MAX_ARTIFACT_HTML_BYTES / 2 + 1)),
            Err(ArtifactError::TooLarge)
        );
    }
    #[test]
    fn hostile_markup_cannot_close_or_modify_the_sandbox() {
        let wrapper = envelope("\">< /iframe></iframe><script>parent.hacked=1</script>&quot;");
        assert_eq!(wrapper.matches("<iframe ").count(), 1);
        assert_eq!(wrapper.matches("</iframe>").count(), 1);
        assert_eq!(wrapper.matches("<script>").count(), 0);
        assert!(wrapper.contains("sandbox=\"allow-scripts\""));
        assert!(!wrapper.contains("allow-same-origin"));
        assert!(wrapper.contains("&amp;quot;"));
        assert!(wrapper.contains("connect-src 'none'"));
        assert!(
            wrapper.find("Content-Security-Policy").unwrap() < wrapper.find("srcdoc=").unwrap()
        );
    }
}
