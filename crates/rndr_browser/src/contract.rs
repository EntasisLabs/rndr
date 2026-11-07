//! Admission for client rects and the host session's logical state.
//!
//! The platform backend applies what this state admits. It does not invent a
//! second policy.

use std::collections::{HashMap, HashSet};

use crate::{
    BrowserEvent, DownloadIntent, FileInputIntent, IntentToken, LoadOutcome, NavigationState,
};

/// Logical rectangle in points. Origin is the top-left of the host client area.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClientRect {
    /// Left edge, in logical points.
    pub x: f32,
    /// Top edge, in logical points.
    pub y: f32,
    /// Width in logical points. Zero hides the surface and keeps the view.
    pub width: f32,
    /// Height in logical points. Zero hides the surface and keeps the view.
    pub height: f32,
}

impl ClientRect {
    /// Admit a rect. Non-finite components are refused before negative ones.
    ///
    /// ```
    /// use rndr_browser::ClientRect;
    /// assert!(ClientRect::try_new(0.0, 0.0, 10.0, 0.0)
    ///     .unwrap()
    ///     .is_zero_area());
    /// assert!(ClientRect::try_new(0.0, 0.0, -1.0, 1.0).is_err());
    /// assert!(ClientRect::try_new(f32::NAN, 0.0, 1.0, 1.0).is_err());
    /// ```
    pub fn try_new(x: f32, y: f32, width: f32, height: f32) -> Result<Self, RectError> {
        admit_client_rect(x, y, width, height)
    }

    /// Width or height is zero. The view stays; the surface is hidden.
    pub fn is_zero_area(self) -> bool {
        self.width == 0.0 || self.height == 0.0
    }
}

/// Why a client rect was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RectError {
    /// A component was NaN or infinite.
    NonFinite,
    /// A component was negative. Zero is allowed.
    Negative,
}

/// Admit `x`, `y`, `width`, and `height` as a client rect.
pub fn admit_client_rect(x: f32, y: f32, width: f32, height: f32) -> Result<ClientRect, RectError> {
    let components = [x, y, width, height];
    if components.iter().any(|component| !component.is_finite()) {
        return Err(RectError::NonFinite);
    }
    if components.iter().any(|component| *component < 0.0) {
        return Err(RectError::Negative);
    }
    Ok(ClientRect {
        x,
        y,
        width,
        height,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IntentKind {
    Download,
    /// Constructed when a platform view reports a file input. Headless tests
    /// construct it too. A Linux library build has neither.
    #[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
    File {
        multiple: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpenIntent {
    pub kind: IntentKind,
}

pub(crate) struct Contract {
    host_visible: bool,
    pending: Option<ClientRect>,
    applied: Option<ClientRect>,
    navigation: NavigationState,
    events: Vec<BrowserEvent>,
    open: HashMap<IntentToken, OpenIntent>,
    answered: HashSet<IntentToken>,
    #[cfg_attr(not(test), allow(dead_code))]
    next: u64,
}

impl Contract {
    #[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
    pub(crate) fn new() -> Self {
        Self {
            host_visible: false,
            pending: None,
            applied: None,
            navigation: NavigationState::default(),
            events: Vec::new(),
            open: HashMap::new(),
            answered: HashSet::new(),
            next: 0,
        }
    }

    pub(crate) fn show(&mut self) {
        self.host_visible = true;
    }

    pub(crate) fn hide(&mut self) {
        self.host_visible = false;
    }

    pub(crate) fn host_visible(&self) -> bool {
        self.host_visible
    }

    pub(crate) fn set_bounds(&mut self, rect: ClientRect) {
        self.pending = Some(rect);
    }

    pub(crate) fn take_pending(&mut self) -> Option<ClientRect> {
        self.pending.take()
    }

    pub(crate) fn pending(&self) -> Option<ClientRect> {
        self.pending
    }

    pub(crate) fn set_applied(&mut self, rect: ClientRect) {
        self.applied = Some(rect);
    }

    pub(crate) fn applied(&self) -> Option<ClientRect> {
        self.applied
    }

    /// The surface is on screen only when the host asked to show it and the
    /// applied rect has a positive area. A pending rect does not count.
    pub(crate) fn surface_visible(&self) -> bool {
        self.host_visible && self.applied.is_some_and(|rect| !rect.is_zero_area())
    }

    pub(crate) fn navigation(&self) -> &NavigationState {
        &self.navigation
    }

    pub(crate) fn observe(&mut self, state: NavigationState) {
        self.navigation = state.clone();
        self.events.push(BrowserEvent::Navigation(state));
    }

    pub(crate) fn push(&mut self, event: BrowserEvent) {
        self.events.push(event);
    }

    pub(crate) fn drain(&mut self) -> Vec<BrowserEvent> {
        std::mem::take(&mut self.events)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn mint(&mut self) -> IntentToken {
        self.next = self.next.saturating_add(1);
        IntentToken::minted(self.next)
    }

    #[cfg_attr(
        not(any(test, target_os = "macos", target_os = "windows", target_os = "linux")),
        allow(dead_code)
    )]
    pub(crate) fn open_download(&mut self, intent: DownloadIntent) {
        self.open.insert(
            intent.token,
            OpenIntent {
                kind: IntentKind::Download,
            },
        );
        self.events.push(BrowserEvent::Download(intent));
    }

    #[cfg_attr(
        not(any(test, target_os = "macos", target_os = "windows", target_os = "linux")),
        allow(dead_code)
    )]
    pub(crate) fn open_file(&mut self, intent: FileInputIntent) {
        self.open.insert(
            intent.token,
            OpenIntent {
                kind: IntentKind::File {
                    multiple: intent.multiple,
                },
            },
        );
        self.events.push(BrowserEvent::FileInput(intent));
    }

    pub(crate) fn open(&self, token: IntentToken) -> Option<OpenIntent> {
        self.open.get(&token).copied()
    }

    pub(crate) fn close(&mut self, token: IntentToken) -> Option<OpenIntent> {
        let intent = self.open.remove(&token)?;
        self.answered.insert(token);
        Some(intent)
    }

    pub(crate) fn restore(&mut self, token: IntentToken, intent: OpenIntent) {
        self.answered.remove(&token);
        self.open.insert(token, intent);
    }

    /// An answer for a token that is not open.
    pub(crate) fn refusal(&self, token: IntentToken) -> crate::BrowserError {
        if self.answered.contains(&token) {
            crate::BrowserError::IntentClosed
        } else {
            crate::BrowserError::UnknownIntent
        }
    }

    pub(crate) fn take_open(&mut self) -> Vec<(IntentToken, IntentKind)> {
        let open = std::mem::take(&mut self.open);
        open.into_iter()
            .map(|(token, intent)| (token, intent.kind))
            .collect()
    }
}

/// Properties read from the live view, independent of the last delegate callback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LiveDocument {
    /// Document title currently stored on the view.
    pub title: String,
    /// URL currently stored on the view.
    pub url: String,
    /// The view is still loading.
    pub loading: bool,
    /// The view can go back.
    pub can_go_back: bool,
    /// The view can go forward.
    pub can_go_forward: bool,
}

impl LiveDocument {
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn from_navigation(state: &NavigationState) -> Self {
        Self {
            title: state.title.clone(),
            url: state.url.clone(),
            loading: state.load == LoadOutcome::Loading,
            can_go_back: state.can_go_back,
            can_go_forward: state.can_go_forward,
        }
    }
}

/// Fold the view's current properties onto the last published navigation.
///
/// A failure or a host stop is kept. A title, URL, or history flag that
/// changes without a new load callback is published. Identical input returns
/// nothing, so a frame does not repeat the same report.
pub(crate) fn reconcile(
    published: &NavigationState,
    live: &LiveDocument,
) -> Option<NavigationState> {
    let load = if live.loading {
        LoadOutcome::Loading
    } else if published.load == LoadOutcome::Loading {
        LoadOutcome::Finished
    } else {
        published.load
    };
    let next = NavigationState {
        title: live.title.clone(),
        url: live.url.clone(),
        load,
        can_go_back: live.can_go_back,
        can_go_forward: live.can_go_forward,
    };
    if next == *published {
        None
    } else {
        Some(next)
    }
}

#[cfg(test)]
mod reconcile_tests {
    use super::*;

    fn finished(title: &str, url: &str, back: bool, forward: bool) -> NavigationState {
        NavigationState {
            title: title.into(),
            url: url.into(),
            load: LoadOutcome::Finished,
            can_go_back: back,
            can_go_forward: forward,
        }
    }

    #[test]
    fn a_title_that_arrives_after_finish_replaces_the_empty_one() {
        let published = finished("", "data:text/html,page", false, false);
        let live = LiveDocument {
            title: "rndr-browser".into(),
            url: "data:text/html,page".into(),
            loading: false,
            can_go_back: false,
            can_go_forward: false,
        };
        let next = reconcile(&published, &live).unwrap();
        assert_eq!(next.title, "rndr-browser");
        assert_eq!(next.load, LoadOutcome::Finished);
        assert_eq!(next.url, "data:text/html,page");
        assert!(reconcile(&next, &live).is_none());
    }

    #[test]
    fn same_document_url_and_history_update_without_a_new_load() {
        let published = finished("rndr-browser", "https://example.test/page", false, false);
        let live = LiveDocument {
            title: "rndr-browser".into(),
            url: "https://example.test/page#section".into(),
            loading: false,
            can_go_back: true,
            can_go_forward: false,
        };
        let next = reconcile(&published, &live).unwrap();
        assert_eq!(next.url, "https://example.test/page#section");
        assert!(next.can_go_back);
        assert_eq!(next.load, LoadOutcome::Finished);
        assert_eq!(next.title, "rndr-browser");
    }

    #[test]
    fn failure_and_stop_are_not_rewritten_when_the_view_is_idle() {
        for load in [LoadOutcome::Failed, LoadOutcome::Stopped] {
            let published = NavigationState {
                title: "kept".into(),
                url: "https://example.test/".into(),
                load,
                can_go_back: false,
                can_go_forward: false,
            };
            let live = LiveDocument::from_navigation(&published);
            assert!(reconcile(&published, &live).is_none());
        }
    }
}
