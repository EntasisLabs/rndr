//! Operating-system clipboard. Core keeps the strings; this pushes and pulls them.

use rndr_core::Clipboard;

#[cfg(all(
    unix,
    not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
))]
use arboard::{GetExtLinux, LinuxClipboardKind, SetExtLinux};

/// A clipboard connection opened on first use. `None` until then, and stays
/// closed when the platform has no clipboard.
pub(crate) struct OsClipboard {
    board: Option<arboard::Clipboard>,
    unavailable: bool,
}

impl OsClipboard {
    pub(crate) fn new() -> Self {
        Self {
            board: None,
            unavailable: false,
        }
    }

    fn board(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.board.is_none() && !self.unavailable {
            match arboard::Clipboard::new() {
                Ok(board) => self.board = Some(board),
                Err(_) => self.unavailable = true,
            }
        }
        self.board.as_mut()
    }

    /// Copy the OS clipboard into `clip` when this frame did not write it.
    pub(crate) fn pull(&mut self, clip: &mut Clipboard) {
        let Some(board) = self.board() else {
            return;
        };
        if !clip.dirty {
            if let Ok(text) = board.get_text() {
                if text != clip.text {
                    clip.text = text;
                }
            }
        }
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        ))]
        if !clip.primary_dirty {
            if let Ok(text) = board.get().clipboard(LinuxClipboardKind::Primary).text() {
                if text != clip.primary {
                    clip.primary = text;
                }
            }
        }
    }

    /// Write dirty fields out. Failures are ignored; the caller clears the flags.
    pub(crate) fn push(&mut self, clip: &Clipboard) {
        let Some(board) = self.board() else {
            return;
        };
        if clip.dirty {
            let _ = board.set_text(clip.text.as_str());
        }
        #[cfg(all(
            unix,
            not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))
        ))]
        if clip.primary_dirty {
            let _ = board
                .set()
                .clipboard(LinuxClipboardKind::Primary)
                .text(clip.primary.as_str());
        }
    }
}
