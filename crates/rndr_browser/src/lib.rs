//! Optional embedded browser.
//!
//! [`Browser`] is a host-owned native view. The `rndr` facade links it only
//! when the `browser` feature is on, and that feature is off by default.
//! `rndr_core`, `rndr_ui`, and `rndr_paint` do not depend on this crate.
//! Enabling `browser` does not enable `dialog`.
//!
//! This slice implements macOS WKWebView: show, hide, logical client-area
//! bounds, navigation, and download and file-input intents. WebView2,
//! WebKitGTK, find, and script evaluation are unsupported. See [`support_matrix`].
//! A headless test does not certify macOS. The native cells stay unverified
//! until a session on that machine is recorded. See the crate README.
//!
//! A guest page does not receive a command channel. Navigation reports a URL,
//! a title, a load outcome, and whether back and forward are available. The
//! host decides what a URL means. A custom scheme is that report, not a
//! dispatch. The page's own scripts stay inside the view. This crate does not
//! evaluate host script, does not install a message handler, and does not
//! register a scheme handler.
//!
//! Downloads and file input are intents with opaque [`IntentToken`]s. The
//! toolkit writes nothing until [`Browser::accept_download`]. It does not open
//! a file panel. [`Browser::cancel_file_input`] and an empty path list select
//! no path. Dropping the session cancels every intent that is still open.
//!
//! Bounds are logical points, origin at the top-left of the host client area.
//! A non-finite or negative component is refused and does not replace a rect
//! already submitted. A zero-area rect hides the surface on the next frame and
//! keeps the view. The rect the host submits is applied on the next [`Browser::frame`].

#![deny(missing_docs)]

mod contract;
mod session;

#[cfg(target_os = "macos")]
mod macos;

pub use contract::{admit_client_rect, ClientRect, RectError};
pub use session::{Browser, BrowserError, BrowserEvent, IntentToken};

use std::fmt;

/// How far a load has gone. The host reads this. It is not a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LoadOutcome {
    /// No navigation has been observed yet.
    Idle,
    /// A navigation is in progress.
    Loading,
    /// The navigation finished.
    Finished,
    /// The navigation failed. A cancelled load is [`Stopped`](Self::Stopped).
    Failed,
    /// The host stopped the load, or the view cancelled it.
    Stopped,
}

/// Title, URL, load outcome, and back/forward availability.
///
/// The URL is a report. This struct has no command name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavigationState {
    /// Document title. Empty when the view has none.
    pub title: String,
    /// Current URL, including a custom scheme. Empty before the first report.
    pub url: String,
    /// Load outcome for this report.
    pub load: LoadOutcome,
    /// The view can go back.
    pub can_go_back: bool,
    /// The view can go forward.
    pub can_go_forward: bool,
}

impl Default for NavigationState {
    fn default() -> Self {
        Self {
            title: String::new(),
            url: String::new(),
            load: LoadOutcome::Idle,
            can_go_back: false,
            can_go_forward: false,
        }
    }
}

/// A download the host may accept or cancel.
///
/// `token` is minted by the toolkit. The page does not choose it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadIntent {
    /// Opaque identity for [`Browser::accept_download`] and [`Browser::cancel_download`].
    pub token: IntentToken,
    /// URL the view reported for the download.
    pub url: String,
    /// File name the view suggested. The host picks the real destination.
    pub suggested_name: String,
    /// MIME type the view reported. Empty when it reported none.
    pub mime: String,
}

/// A file input the host may answer or cancel.
///
/// The toolkit does not open a panel and does not choose paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileInputIntent {
    /// Opaque identity for [`Browser::provide_file_input`] and [`Browser::cancel_file_input`].
    pub token: IntentToken,
    /// The page asked for more than one file. The host still chooses the paths.
    pub multiple: bool,
}

/// Code in this crate for one matrix cell.
///
/// This is not a native certification. [`NativeStatus`] is that column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SliceStatus {
    /// This slice contains the behavior.
    Implemented,
    /// This slice does not provide the behavior.
    Unsupported,
}

/// Whether a live session has shown the cell.
///
/// There is no `Supported` value. A live session is recorded outside this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeStatus {
    /// No live session in this change has shown the behavior.
    Unverified,
    /// This slice does not claim the cell on that backend.
    Unsupported,
}

/// One row of the backend matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixRow {
    /// Backend name. A backend that has not landed is unsupported.
    pub backend: &'static str,
    /// Capability this row describes.
    pub capability: &'static str,
    /// Whether this slice contains the capability.
    pub slice: SliceStatus,
    /// Native certification. Headless tests do not change this.
    pub native: NativeStatus,
}

const MATRIX: &[MatrixRow] = &[
    MatrixRow {
        backend: "macOS WKWebView",
        capability: "navigation, show, hide, bounds",
        slice: SliceStatus::Implemented,
        native: NativeStatus::Unverified,
    },
    MatrixRow {
        backend: "macOS WKWebView",
        capability: "downloads and file input",
        slice: SliceStatus::Implemented,
        native: NativeStatus::Unverified,
    },
    MatrixRow {
        backend: "macOS WKWebView",
        capability: "find",
        slice: SliceStatus::Unsupported,
        native: NativeStatus::Unsupported,
    },
    MatrixRow {
        backend: "macOS WKWebView",
        capability: "script",
        slice: SliceStatus::Unsupported,
        native: NativeStatus::Unsupported,
    },
    MatrixRow {
        backend: "WebView2",
        capability: "navigation, downloads, find, script",
        slice: SliceStatus::Unsupported,
        native: NativeStatus::Unsupported,
    },
    MatrixRow {
        backend: "WebKitGTK",
        capability: "navigation, downloads, find, script",
        slice: SliceStatus::Unsupported,
        native: NativeStatus::Unsupported,
    },
];

/// Backend matrix for this slice.
///
/// macOS navigation and intents are implemented and native-unverified.
/// WebView2, WebKitGTK, find, and script are unsupported.
pub fn support_matrix() -> &'static [MatrixRow] {
    MATRIX
}

impl fmt::Display for RectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonFinite => "client rect has a non-finite component",
            Self::Negative => "client rect has a negative component",
        })
    }
}

impl std::error::Error for RectError {}

impl fmt::Display for BrowserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported { backend } => write!(f, "{backend} is unsupported in this slice"),
            Self::Parent => f.write_str("the host window is not a view this slice can parent"),
            Self::Thread => f.write_str("the browser must be used on the platform main thread"),
            Self::Rect(error) => write!(f, "{error}"),
            Self::Url => f.write_str("the navigation URL is empty or was refused by the view"),
            Self::UnknownIntent => f.write_str("no open intent has that token"),
            Self::IntentClosed => f.write_str("that intent was already answered or cancelled"),
            Self::Destination => f.write_str("the path was refused and nothing was written"),
            Self::FileCount => f.write_str("the file input asked for one path"),
        }
    }
}

impl std::error::Error for BrowserError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Rect(error) => Some(error),
            _ => None,
        }
    }
}
