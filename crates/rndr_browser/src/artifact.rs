//! A deliberately capability-free surface, separate from ordinary browsing.

use crate::{admit_client_rect, BrowserError, ClientRect, RectError};
use raw_window_handle::HasWindowHandle;
use std::fmt;

/// Maximum UTF-8 source bytes accepted by an artifact preview (2 MiB).
/// The preview copy adds only a small root-viewport style.
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
    /// Host overlay admission or native image allocation failed.
    Overlay(&'static str),
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
            Self::Overlay(error) => write!(f, "artifact host overlay: {error}"),
            Self::Policy(error) => write!(f, "artifact policy was not installed: {error}"),
            Self::LoadFailed => {
                f.write_str("artifact load failed or its content process terminated")
            }
        }
    }
}
impl std::error::Error for ArtifactError {}

/// State of the isolated document load; neither script completion nor a paint acknowledgment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactState {
    /// Resource policy and source-free sandbox preflight are preparing. No artifact is loaded yet.
    Preparing,
    /// Policy and sandbox are verified and the artifact document is loading.
    Loading,
    /// The document finished navigation. Local scripts may continue running.
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
    overlays: Vec<crate::OverlayBitmap>,
    overlays_revision: u64,
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
                overlays: Vec::new(),
                overlays_revision: 0,
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

    /// Submit trusted host popup images, ordered back-to-front, above WebKit.
    ///
    /// Images have true alpha: guest content remains visible under rounded
    /// corners and shadows. This is host-only input, never guest readback.
    /// Changes apply on [`Self::frame`]; an empty slice clears all images.
    /// Admission is atomic and bounded by [`crate::MAX_OVERLAY_BYTES`],
    /// [`crate::MAX_OVERLAYS`] and [`crate::MAX_OVERLAY_SIDE`]. Identical
    /// submissions are no-ops. Cache raster work on the host; this method does
    /// not capture or evaluate HTML. Hidden/failed/dropped previews hide or
    /// retire their overlays, and replacement retains the submitted images.
    pub fn set_overlays(&mut self, overlays: &[crate::OverlayBitmap]) -> Result<(), ArtifactError> {
        crate::overlay::validate(overlays)?;
        if self.overlays != overlays {
            self.overlays = overlays.to_vec();
            self.overlays_revision = self.overlays_revision.wrapping_add(1);
        }
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
                self.overlays_revision,
                &self.overlays,
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

// Source preparation is layout-only. Immutable response CSP and the native
// transport enforce isolation; no security boundary depends on authored markup.
#[cfg(any(
    test,
    all(target_os = "macos", feature = "macos-private-webkit-artifacts")
))]
pub(crate) fn document_html(html: &str) -> String {
    // Preserve doctype/quirks, authored layout and nested scroll semantics.
    format!("{html}\n<style data-rndr-artifact-viewport>html:root{{overscroll-behavior:none!important}}</style>")
}

// One exact native-armed MAIN document, then one response. No guest-created
// subframes, new windows, reloads, redirects, or downloads are armed.
// Engine-handled same-document anchors cannot deliver new bytes.
#[cfg(any(test, target_os = "macos"))]
#[derive(Default)]
pub(crate) struct ArtifactNavigation {
    action: Option<&'static str>,
    response: Option<&'static str>,
}
#[cfg(any(test, target_os = "macos"))]
impl ArtifactNavigation {
    #[cfg(any(test, feature = "macos-private-webkit-artifacts"))]
    pub(crate) fn arm(&mut self, url: &'static str) {
        self.action = Some(url);
        self.response = None;
    }
    pub(crate) fn admit(
        &mut self,
        url: &str,
        target_main: Option<bool>,
        source_main: bool,
        download: bool,
    ) -> bool {
        if !download && source_main && target_main == Some(true) && self.action == Some(url) {
            self.response = self.action.take();
            true
        } else {
            false
        }
    }
    pub(crate) fn admit_response(&mut self, url: &str, main: bool) -> bool {
        if main && self.response == Some(url) {
            self.response = None;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn viewport_presentation_contains_only_root_edges_without_rewriting_source() {
        let original = "<!doctype html><style>.nested{overflow:auto;overscroll-behavior:contain}</style><body>ok</body>";
        let wrapper = document_html(original);
        assert!(wrapper.starts_with(original));
        assert!(wrapper.contains(".nested{overflow:auto;overscroll-behavior:contain}"));
        assert!(wrapper.contains("html:root{overscroll-behavior:none!important}"));
        assert!(!wrapper.contains("*{overscroll"));
        assert!(original.ends_with("<body>ok</body>"));
    }

    #[test]
    fn native_navigation_gate_accepts_only_armed_main_action_and_response() {
        const URL: &str = "rndr-artifact://preview/document";
        let mut gate = ArtifactNavigation::default();
        assert!(!gate.admit(URL, Some(true), true, false));
        gate.arm(URL);
        for url in [
            "about:blank",
            "about:srcdoc",
            "http://localhost",
            "file:///etc/passwd",
            "data:text/html,hello",
            "blob:opaque",
            "medousa:invoke",
            "rndr-artifact://preview/document#hash",
            "rndr-artifact://preview/document?x",
        ] {
            assert!(!gate.admit(url, Some(true), true, false));
            assert!(!gate.admit_response(url, true));
        }
        assert!(!gate.admit(URL, None, true, false));
        assert!(!gate.admit(URL, Some(false), true, false));
        assert!(!gate.admit(URL, Some(true), false, false));
        assert!(!gate.admit(URL, Some(true), true, true));
        assert!(!gate.admit_response(URL, true));
        assert!(gate.admit(URL, Some(true), true, false));
        assert!(!gate.admit(URL, Some(true), true, false));
        assert!(!gate.admit_response(URL, false));
        assert!(gate.admit_response(URL, true));
        assert!(!gate.admit_response(URL, true));
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
    fn document_source_is_not_rewritten_or_wrapped_in_a_scrolling_subframe() {
        let html = "<!doctype html><script>document.write('authored')</script></iframe>&quot;";
        assert!(document_html(html).starts_with(html));
        assert!(!document_html("hello").contains("iframe"));
    }
}
