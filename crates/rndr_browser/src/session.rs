//! Host session. The platform engine is the only code that touches a webview.

use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{Arc, Mutex};

use raw_window_handle::HasWindowHandle;

use crate::contract::{Contract, IntentKind};
use crate::{ClientRect, DownloadIntent, FileInputIntent, LoadOutcome, NavigationState, RectError};

/// Failure from the host session.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BrowserError {
    /// This slice has no backend that can parent a view here.
    Unsupported {
        /// Backend the call asked for.
        backend: &'static str,
    },
    /// The handle is not an AppKit view this slice can parent.
    Parent,
    /// WKWebView is only created on the AppKit main thread.
    Thread,
    /// The rect was refused. The previous submission is unchanged.
    Rect(RectError),
    /// The URL is empty, or the view refused to parse it.
    Url,
    /// No open intent uses that token.
    UnknownIntent,
    /// The intent was already answered or cancelled.
    IntentClosed,
    /// The destination is not an absolute file path whose parent exists.
    /// Nothing was written.
    Destination,
    /// The page asked for a single file and the host supplied more than one.
    /// The intent stays open.
    FileCount,
}

/// What the host observes from [`Browser::frame`].
///
/// A navigation is not an application command. A download or file input is an
/// intent, not a write and not a dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BrowserEvent {
    /// Title, URL, load outcome, and back/forward availability.
    Navigation(NavigationState),
    /// The view wants a download. Nothing is written yet.
    Download(DownloadIntent),
    /// The view wants files. No panel is opened.
    FileInput(FileInputIntent),
    /// An open intent was cancelled. No path was selected and nothing was written.
    IntentCancelled {
        /// The intent that is now closed.
        token: IntentToken,
    },
}

/// Opaque identity of a download or file-input intent.
///
/// The toolkit mints this. The page does not choose it, and it is not a
/// command name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IntentToken(u64);

impl IntentToken {
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn minted(value: u64) -> Self {
        Self(value)
    }

    #[cfg(test)]
    pub(crate) fn unused() -> Self {
        Self(0)
    }
}

/// Host-owned browser session.
///
/// On macOS this owns a WKWebView. Elsewhere [`Browser::attach`] returns
/// [`BrowserError::Unsupported`]. Dropping the session cancels unanswered
/// intents and releases the view.
pub struct Browser {
    contract: Contract,
    engine: Engine,
    /// Last visibility actually given to the engine. Pending rects do not change it.
    surface_shown: bool,
}

enum Engine {
    #[cfg(test)]
    Detached(Detached),
    #[cfg(target_os = "macos")]
    WebKit(crate::macos::WebKit),
    #[cfg(not(any(test, target_os = "macos")))]
    #[allow(dead_code)]
    Unavailable,
}

#[cfg(test)]
struct Detached {
    probe: Arc<Mutex<Probe>>,
}

/// Records what the headless engine was asked to do. It is not a webview.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProbeEvent {
    Navigate(String),
    Back,
    Forward,
    Reload,
    Stop,
    Apply(ClientRect, bool),
    Hide,
    Show,
    Write(PathBuf),
    CancelDownload(IntentToken),
    Files(IntentToken, Option<Vec<PathBuf>>),
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct Probe {
    pub events: Vec<ProbeEvent>,
}

#[cfg(test)]
impl Probe {
    fn push(probe: &Mutex<Self>, event: ProbeEvent) {
        probe.lock().expect("probe").events.push(event);
    }
}

impl Browser {
    /// Parent a view in `parent`.
    ///
    /// macOS creates a hidden WKWebView in the AppKit content view. The call
    /// is refused off the main thread, and when the handle is not that view.
    /// Other platforms return [`BrowserError::Unsupported`] and do not link a
    /// webview. The session starts hidden, with no navigation and no dialog.
    pub fn attach(parent: &impl HasWindowHandle) -> Result<Self, BrowserError> {
        #[cfg(target_os = "macos")]
        {
            let engine = crate::macos::WebKit::attach(parent)?;
            Ok(Self {
                contract: Contract::new(),
                engine: Engine::WebKit(engine),
                surface_shown: false,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = parent;
            Err(BrowserError::Unsupported {
                backend: "WKWebView",
            })
        }
    }

    /// Show the surface on the next applied positive rect, or immediately when
    /// one is already applied.
    pub fn show(&mut self) {
        self.contract.show();
        self.sync_visibility();
    }

    /// Hide the surface. The view and its navigation state stay.
    pub fn hide(&mut self) {
        self.contract.hide();
        self.sync_visibility();
    }

    /// Submit a client rect. It is applied on the next [`Self::frame`].
    ///
    /// A non-finite or negative component returns [`RectError`] and leaves the
    /// pending rect unchanged. Zero width or height is accepted and hides the
    /// surface when that frame is applied.
    pub fn set_bounds(&mut self, rect: ClientRect) -> Result<(), RectError> {
        let rect = ClientRect::try_new(rect.x, rect.y, rect.width, rect.height)?;
        self.contract.set_bounds(rect);
        Ok(())
    }

    /// Rect submitted and not yet applied.
    pub fn pending_bounds(&self) -> Option<ClientRect> {
        self.contract.pending()
    }

    /// Rect applied to the guest surface. This lags [`Self::set_bounds`] by one frame.
    pub fn applied_bounds(&self) -> Option<ClientRect> {
        self.contract.applied()
    }

    /// Whether the guest surface is showing. A pending rect does not count.
    pub fn surface_visible(&self) -> bool {
        self.contract.surface_visible()
    }

    /// Last observed navigation. A URL here is not a command.
    pub fn navigation(&self) -> &NavigationState {
        self.contract.navigation()
    }

    /// Load `url` in the view.
    ///
    /// An empty URL is refused. Any other URL, including a custom scheme, is a
    /// navigation request. This method does not dispatch on the scheme.
    pub fn navigate(&mut self, url: &str) -> Result<(), BrowserError> {
        if url.trim().is_empty() {
            return Err(BrowserError::Url);
        }
        self.engine.navigate(url)
    }

    /// Go back. The observed back/forward flags update when the view reports them.
    pub fn back(&mut self) -> Result<(), BrowserError> {
        self.engine.back()
    }

    /// Go forward.
    pub fn forward(&mut self) -> Result<(), BrowserError> {
        self.engine.forward()
    }

    /// Reload the current document.
    pub fn reload(&mut self) -> Result<(), BrowserError> {
        self.engine.reload()
    }

    /// Stop the load and record [`LoadOutcome::Stopped`].
    pub fn stop(&mut self) {
        self.engine.stop();
        let mut state = self.contract.navigation().clone();
        if state.load == LoadOutcome::Loading || state.load == LoadOutcome::Idle {
            state.load = LoadOutcome::Stopped;
            self.contract.observe(state);
        }
    }

    /// Apply a pending rect, then return observations since the previous frame.
    ///
    /// Download and file-input intents appear here. An intent with no later
    /// host answer writes nothing and selects no path.
    pub fn frame(&mut self) -> Vec<BrowserEvent> {
        if let Some(rect) = self.contract.take_pending() {
            let visible = self.contract.host_visible() && !rect.is_zero_area();
            self.contract.set_applied(rect);
            self.engine.apply(rect, visible);
            self.surface_shown = visible;
        }
        self.engine.pull(&mut self.contract);
        self.contract.drain()
    }

    /// Write the download to `destination`.
    ///
    /// The path must be absolute, not a directory, and its parent must already
    /// exist. This crate does not create the parent and does not write any
    /// byte before this call returns `Ok`. A refused path leaves the intent open.
    pub fn accept_download(
        &mut self,
        token: IntentToken,
        destination: &Path,
    ) -> Result<(), BrowserError> {
        admit_destination(destination)?;
        match self.contract.open(token).map(|intent| intent.kind) {
            Some(IntentKind::Download) => {}
            _ => return Err(self.contract.refusal(token)),
        }
        self.engine.commit_download(token, destination)?;
        self.contract.close(token);
        Ok(())
    }

    /// Cancel a download. Nothing is written.
    pub fn cancel_download(&mut self, token: IntentToken) -> Result<(), BrowserError> {
        match self.contract.close(token) {
            Some(intent) if intent.kind == IntentKind::Download => {}
            Some(intent) => {
                self.contract.restore(token, intent);
                return Err(BrowserError::UnknownIntent);
            }
            None => return Err(self.contract.refusal(token)),
        }
        self.engine.cancel_download(token);
        self.contract.push(BrowserEvent::IntentCancelled { token });
        Ok(())
    }

    /// Give `paths` to the file input.
    ///
    /// An empty list cancels and selects no path. More than one path is
    /// refused when the page asked for a single file, and the intent stays open.
    pub fn provide_file_input(
        &mut self,
        token: IntentToken,
        paths: &[PathBuf],
    ) -> Result<(), BrowserError> {
        if paths.is_empty() {
            return self.cancel_file_input(token);
        }
        let multiple = match self.contract.open(token).map(|intent| intent.kind) {
            Some(IntentKind::File { multiple }) => multiple,
            _ => return Err(self.contract.refusal(token)),
        };
        if !multiple && paths.len() > 1 {
            return Err(BrowserError::FileCount);
        }
        self.engine.finish_file(token, Some(paths))?;
        self.contract.close(token);
        Ok(())
    }

    /// Cancel a file input. No path is selected.
    pub fn cancel_file_input(&mut self, token: IntentToken) -> Result<(), BrowserError> {
        match self.contract.close(token) {
            Some(intent) if matches!(intent.kind, IntentKind::File { .. }) => {}
            Some(intent) => {
                self.contract.restore(token, intent);
                return Err(BrowserError::UnknownIntent);
            }
            None => return Err(self.contract.refusal(token)),
        }
        self.engine.finish_file(token, None)?;
        self.contract.push(BrowserEvent::IntentCancelled { token });
        Ok(())
    }

    fn sync_visibility(&mut self) {
        let visible = self.contract.surface_visible();
        if visible == self.surface_shown {
            return;
        }
        self.surface_shown = visible;
        if visible {
            self.engine.show();
        } else {
            self.engine.hide();
        }
    }

    #[cfg(test)]
    pub(crate) fn detached() -> (Self, Arc<Mutex<Probe>>) {
        let probe = Arc::new(Mutex::new(Probe::default()));
        let browser = Self {
            contract: Contract::new(),
            engine: Engine::Detached(Detached {
                probe: Arc::clone(&probe),
            }),
            surface_shown: false,
        };
        (browser, probe)
    }

    #[cfg(test)]
    pub(crate) fn guest_navigation(&mut self, state: NavigationState) {
        self.contract.observe(state);
    }

    #[cfg(test)]
    pub(crate) fn guest_download(&mut self, url: &str, name: &str, mime: &str) -> IntentToken {
        let token = self.contract.mint();
        self.contract.open_download(DownloadIntent {
            token,
            url: url.to_string(),
            suggested_name: name.to_string(),
            mime: mime.to_string(),
        });
        token
    }

    #[cfg(test)]
    pub(crate) fn guest_file(&mut self, multiple: bool) -> IntentToken {
        let token = self.contract.mint();
        self.contract.open_file(FileInputIntent { token, multiple });
        token
    }
}

impl Drop for Browser {
    fn drop(&mut self) {
        let open = self.contract.take_open();
        self.engine.shutdown(&open);
    }
}

fn admit_destination(path: &Path) -> Result<(), BrowserError> {
    if path.as_os_str().is_empty() || !path.is_absolute() || path.to_str().is_none() {
        return Err(BrowserError::Destination);
    }
    if path.is_dir() {
        return Err(BrowserError::Destination);
    }
    match path.parent() {
        Some(parent) if parent.is_dir() => Ok(()),
        _ => Err(BrowserError::Destination),
    }
}

impl Engine {
    fn navigate(&mut self, url: &str) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Navigate(url.to_string()));
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.navigate(url),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = url;
                Err(BrowserError::Unsupported {
                    backend: "WKWebView",
                })
            }
        }
    }

    fn back(&mut self) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Back);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.back(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => Err(BrowserError::Unsupported {
                backend: "WKWebView",
            }),
        }
    }

    fn forward(&mut self) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Forward);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.forward(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => Err(BrowserError::Unsupported {
                backend: "WKWebView",
            }),
        }
    }

    fn reload(&mut self) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Reload);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.reload(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => Err(BrowserError::Unsupported {
                backend: "WKWebView",
            }),
        }
    }

    fn stop(&mut self) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => Probe::push(&engine.probe, ProbeEvent::Stop),
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.stop(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {}
        }
    }

    fn apply(&mut self, rect: ClientRect, visible: bool) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Apply(rect, visible));
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.apply(rect, visible),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = (rect, visible);
            }
        }
    }

    fn show(&mut self) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => Probe::push(&engine.probe, ProbeEvent::Show),
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.show(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {}
        }
    }

    fn hide(&mut self) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => Probe::push(&engine.probe, ProbeEvent::Hide),
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.hide(),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {}
        }
    }

    fn pull(&mut self, contract: &mut Contract) {
        match self {
            #[cfg(test)]
            Self::Detached(_) => {
                let _ = contract;
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.pull(contract),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = contract;
            }
        }
    }

    fn commit_download(
        &mut self,
        token: IntentToken,
        destination: &Path,
    ) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::Write(destination.to_path_buf()));
                let _ = token;
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.commit_download(token, destination),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = (token, destination);
                Err(BrowserError::Unsupported {
                    backend: "WKWebView",
                })
            }
        }
    }

    fn cancel_download(&mut self, token: IntentToken) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(&engine.probe, ProbeEvent::CancelDownload(token));
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.cancel_download(token),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = token;
            }
        }
    }

    fn finish_file(
        &mut self,
        token: IntentToken,
        paths: Option<&[PathBuf]>,
    ) -> Result<(), BrowserError> {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                Probe::push(
                    &engine.probe,
                    ProbeEvent::Files(token, paths.map(|paths| paths.to_vec())),
                );
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => engine.finish_file(token, paths),
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = (token, paths);
                Err(BrowserError::Unsupported {
                    backend: "WKWebView",
                })
            }
        }
    }

    fn shutdown(&mut self, open: &[(IntentToken, IntentKind)]) {
        match self {
            #[cfg(test)]
            Self::Detached(engine) => {
                for (token, kind) in open {
                    match kind {
                        IntentKind::Download => {
                            Probe::push(&engine.probe, ProbeEvent::CancelDownload(*token));
                        }
                        IntentKind::File { .. } => {
                            Probe::push(&engine.probe, ProbeEvent::Files(*token, None));
                        }
                    }
                }
            }
            #[cfg(target_os = "macos")]
            Self::WebKit(engine) => {
                let _ = open;
                engine.shutdown();
            }
            #[cfg(not(any(test, target_os = "macos")))]
            Self::Unavailable => {
                let _ = open;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

    /// A parent whose handle cannot be read. Attach must not invent a view.
    struct ClosedParent;

    impl HasWindowHandle for ClosedParent {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }

    fn rect(x: f32, y: f32, width: f32, height: f32) -> ClientRect {
        ClientRect::try_new(x, y, width, height).unwrap()
    }

    fn probe_events(probe: &Mutex<Probe>) -> Vec<ProbeEvent> {
        probe.lock().expect("probe").events.clone()
    }

    #[test]
    fn attach_reports_the_platform_boundary() {
        let Err(error) = Browser::attach(&ClosedParent) else {
            panic!("attach should fail without a platform view");
        };
        #[cfg(target_os = "macos")]
        assert!(matches!(error, BrowserError::Parent));
        #[cfg(not(target_os = "macos"))]
        assert!(matches!(
            error,
            BrowserError::Unsupported {
                backend: "WKWebView"
            }
        ));
    }

    #[test]
    fn negative_and_non_finite_rects_do_not_replace_a_pending_rect() {
        let (mut browser, _) = Browser::detached();
        browser.set_bounds(rect(1.0, 2.0, 3.0, 4.0)).unwrap();
        let pending = browser.pending_bounds();
        assert_eq!(
            browser.set_bounds(ClientRect {
                x: -1.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            }),
            Err(RectError::Negative)
        );
        assert_eq!(
            browser
                .set_bounds(ClientRect {
                    x: f32::INFINITY,
                    y: 0.0,
                    width: 1.0,
                    height: 1.0,
                })
                .unwrap_err(),
            RectError::NonFinite
        );
        assert_eq!(browser.pending_bounds(), pending);
        assert_eq!(browser.applied_bounds(), None);
    }

    #[test]
    fn bounds_apply_on_the_next_frame_and_a_later_resize_waits() {
        let (mut browser, probe) = Browser::detached();
        let first = rect(1.0, 2.0, 30.0, 40.0);
        let replacement = rect(4.0, 5.0, 60.0, 70.0);
        let resized = rect(8.0, 9.0, 10.0, 11.0);
        browser.show();
        browser.set_bounds(first).unwrap();
        browser.set_bounds(replacement).unwrap();
        assert_eq!(browser.applied_bounds(), None);
        assert!(!browser.surface_visible());
        assert!(probe_events(&probe).is_empty());

        let _ = browser.frame();
        assert_eq!(browser.applied_bounds(), Some(replacement));
        assert!(browser.surface_visible());
        assert_eq!(
            probe_events(&probe),
            vec![ProbeEvent::Apply(replacement, true)]
        );

        browser.set_bounds(resized).unwrap();
        assert_eq!(browser.applied_bounds(), Some(replacement));
        let _ = browser.frame();
        assert_eq!(browser.applied_bounds(), Some(resized));
        assert_eq!(
            probe_events(&probe).last(),
            Some(&ProbeEvent::Apply(resized, true))
        );
    }

    #[test]
    fn zero_area_hides_the_surface_and_keeps_navigation() {
        let (mut browser, probe) = Browser::detached();
        browser.show();
        browser.set_bounds(rect(0.0, 0.0, 20.0, 20.0)).unwrap();
        let _ = browser.frame();
        browser.guest_navigation(NavigationState {
            title: "kept".into(),
            url: "https://example.test/kept".into(),
            load: LoadOutcome::Finished,
            can_go_back: true,
            can_go_forward: false,
        });
        let _ = browser.frame();
        browser.set_bounds(rect(0.0, 0.0, 0.0, 20.0)).unwrap();
        assert!(browser.surface_visible());
        let _ = browser.frame();
        assert!(!browser.surface_visible());
        assert_eq!(browser.navigation().title, "kept");
        assert_eq!(browser.navigation().url, "https://example.test/kept");
        assert!(browser.navigation().can_go_back);
        assert_eq!(
            probe_events(&probe).last(),
            Some(&ProbeEvent::Apply(rect(0.0, 0.0, 0.0, 20.0), false))
        );
        browser.show();
        assert!(!browser.surface_visible());
    }

    #[test]
    fn hide_and_show_use_the_applied_rect() {
        let (mut browser, probe) = Browser::detached();
        browser.show();
        assert!(probe_events(&probe).is_empty());
        browser.set_bounds(rect(0.0, 0.0, 8.0, 8.0)).unwrap();
        let _ = browser.frame();
        browser.hide();
        assert!(!browser.surface_visible());
        assert_eq!(probe_events(&probe).last(), Some(&ProbeEvent::Hide));
        browser.show();
        assert!(browser.surface_visible());
        assert_eq!(probe_events(&probe).last(), Some(&ProbeEvent::Show));
        assert_eq!(browser.navigation().load, LoadOutcome::Idle);
    }

    #[test]
    fn custom_scheme_is_a_navigation_report() {
        let (mut browser, probe) = Browser::detached();
        browser.navigate("myapp://open?id=1").unwrap();
        assert!(browser.frame().is_empty());
        assert_eq!(
            probe_events(&probe),
            vec![ProbeEvent::Navigate("myapp://open?id=1".into())]
        );
        browser.guest_navigation(NavigationState {
            title: String::new(),
            url: "myapp://open?id=1".into(),
            load: LoadOutcome::Failed,
            can_go_back: false,
            can_go_forward: false,
        });
        match browser.frame().as_slice() {
            [BrowserEvent::Navigation(state)] => {
                assert_eq!(state.url, "myapp://open?id=1");
                assert_eq!(state.load, LoadOutcome::Failed);
                assert!(!format!("{state:?}").contains("command"));
            }
            other => panic!("navigation report, got {other:?}"),
        }
        assert!(browser.navigate("  ").is_err());
    }

    #[test]
    fn back_forward_reload_and_stop_are_host_requests() {
        let (mut browser, probe) = Browser::detached();
        browser.navigate("https://example.test/").unwrap();
        browser.back().unwrap();
        browser.forward().unwrap();
        browser.reload().unwrap();
        browser.guest_navigation(NavigationState {
            title: String::new(),
            url: "https://example.test/".into(),
            load: LoadOutcome::Loading,
            can_go_back: true,
            can_go_forward: true,
        });
        let _ = browser.frame();
        browser.stop();
        assert_eq!(browser.navigation().load, LoadOutcome::Stopped);
        assert!(browser.navigation().can_go_back);
        assert!(browser.navigation().can_go_forward);
        let events = probe_events(&probe);
        assert!(events.contains(&ProbeEvent::Back));
        assert!(events.contains(&ProbeEvent::Forward));
        assert!(events.contains(&ProbeEvent::Reload));
        assert!(events.contains(&ProbeEvent::Stop));
    }

    #[test]
    fn download_writes_only_after_the_host_accepts_a_destination() {
        let (mut browser, probe) = Browser::detached();
        let token = browser.guest_download(
            "https://example.test/file.bin",
            "file.bin",
            "application/octet-stream",
        );
        match browser.frame().as_slice() {
            [BrowserEvent::Download(intent)] => {
                assert_eq!(intent.token, token);
                assert_eq!(intent.suggested_name, "file.bin");
                assert!(!format!("{intent:?}").contains("command"));
            }
            other => panic!("download intent, got {other:?}"),
        }
        assert!(probe_events(&probe)
            .iter()
            .all(|event| !matches!(event, ProbeEvent::Write(_))));

        let destination = std::env::temp_dir().join("rndr-browser-r3-unwritten.bin");
        let _ = std::fs::remove_file(&destination);
        assert_eq!(
            browser.accept_download(token, Path::new("relative.bin")),
            Err(BrowserError::Destination)
        );
        assert!(!destination.exists());
        browser.accept_download(token, &destination).unwrap();
        assert_eq!(
            probe_events(&probe)
                .into_iter()
                .filter(|event| matches!(event, ProbeEvent::Write(_)))
                .collect::<Vec<_>>(),
            vec![ProbeEvent::Write(destination.clone())]
        );
        assert!(
            !destination.exists(),
            "the toolkit must not create the file itself"
        );
        assert_eq!(
            browser.accept_download(token, &destination),
            Err(BrowserError::IntentClosed)
        );
        assert_eq!(
            browser.cancel_download(IntentToken::unused()),
            Err(BrowserError::UnknownIntent)
        );
    }

    #[test]
    fn cancelled_download_and_drop_write_nothing() {
        let (mut browser, probe) = Browser::detached();
        let token = browser.guest_download("https://example.test/a", "a.bin", "text/plain");
        let _ = browser.frame();
        browser.cancel_download(token).unwrap();
        assert!(matches!(
            browser.frame().as_slice(),
            [BrowserEvent::IntentCancelled { token: cancelled }] if *cancelled == token
        ));
        assert!(probe_events(&probe)
            .iter()
            .all(|event| !matches!(event, ProbeEvent::Write(_))));

        let (browser, probe) = Browser::detached();
        let mut browser = browser;
        let token = browser.guest_download("https://example.test/b", "b.bin", "text/plain");
        drop(browser);
        assert_eq!(
            probe_events(&probe),
            vec![ProbeEvent::CancelDownload(token)]
        );
    }

    #[test]
    fn unanswered_file_input_selects_no_path() {
        let (mut browser, probe) = Browser::detached();
        let token = browser.guest_file(false);
        match browser.frame().as_slice() {
            [BrowserEvent::FileInput(intent)] => {
                assert_eq!(intent.token, token);
                assert!(!intent.multiple);
            }
            other => panic!("file intent, got {other:?}"),
        }
        let one = PathBuf::from("/tmp/one");
        let two = PathBuf::from("/tmp/two");
        assert_eq!(
            browser.provide_file_input(token, &[one.clone(), two]),
            Err(BrowserError::FileCount)
        );
        assert!(probe_events(&probe).is_empty());
        browser
            .provide_file_input(token, std::slice::from_ref(&one))
            .unwrap();
        assert_eq!(
            probe_events(&probe),
            vec![ProbeEvent::Files(token, Some(vec![one]))]
        );

        let (mut browser, probe) = Browser::detached();
        let token = browser.guest_file(true);
        browser.provide_file_input(token, &[]).unwrap();
        assert_eq!(probe_events(&probe), vec![ProbeEvent::Files(token, None)]);

        let (browser, probe) = Browser::detached();
        let mut browser = browser;
        let token = browser.guest_file(true);
        drop(browser);
        assert_eq!(probe_events(&probe), vec![ProbeEvent::Files(token, None)]);
    }

    #[test]
    fn matrix_keeps_native_cells_open_and_other_backends_unsupported() {
        use crate::{NativeStatus, SliceStatus};
        let matrix = crate::support_matrix();
        assert!(matrix.iter().any(|row| {
            row.backend == "macOS WKWebView"
                && row.capability.contains("bounds")
                && row.slice == SliceStatus::Implemented
                && row.native == NativeStatus::Unverified
        }));
        assert!(matrix.iter().any(|row| {
            row.backend == "macOS WKWebView"
                && row.capability == "find"
                && row.slice == SliceStatus::Unsupported
                && row.native == NativeStatus::Unsupported
        }));
        assert!(matrix.iter().any(|row| {
            row.backend == "macOS WKWebView"
                && row.capability == "script"
                && row.slice == SliceStatus::Unsupported
        }));
        for backend in ["WebView2", "WebKitGTK"] {
            let rows: Vec<_> = matrix.iter().filter(|row| row.backend == backend).collect();
            assert!(!rows.is_empty());
            assert!(rows.iter().all(|row| {
                row.slice == SliceStatus::Unsupported && row.native == NativeStatus::Unsupported
            }));
        }
        assert!(matrix.iter().all(|row| matches!(
            row.native,
            NativeStatus::Unverified | NativeStatus::Unsupported
        )));
    }
}
