//! IME preedit and commit, shared by the field, the prompt, and the editor.
//!
//! The desktop runner records platform events in order and folds them with
//! [`resolve_ime`] before they reach [`crate::Input`]. Widgets also call
//! [`text_frame`], which applies the same rules to a snapshot, so a commit is
//! inserted once and a preedit does not also type the keys that built it.
//!
//! While a preedit is showing, character keys, Space, Enter, Backspace, Delete,
//! and the arrows are dropped unless Ctrl, Alt, or Super is held. A commit
//! clears text on the keys that led up to it. A key after the commit whose text
//! repeats the committed string loses that text. Shortcuts are kept.

use crate::input::{Input, Key};
use crate::keys::{KeyEvent, LogicalKey, NamedKey};

/// One platform input event, in the order the window delivered it.
#[derive(Clone, Debug, PartialEq)]
pub enum ImeEvent {
    /// The platform enabled the input method.
    Enabled,
    /// The platform disabled the input method and cleared the preedit.
    Disabled,
    /// Replace the uncommitted composition. An empty string clears it.
    ///
    /// `cursor` is a byte range inside the string. `None` means the platform
    /// did not report a caret; the field draws one at the end of the preedit.
    Preedit(String, Option<(usize, usize)>),
    /// Insert `text` and clear the preedit. An empty string still clears it.
    Commit(String),
    /// A key. Text may be removed by [`resolve_ime`].
    Key(KeyEvent),
}

/// Keys and strings safe to apply for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedIme {
    /// Keys with composition text removed.
    pub keys: Vec<KeyEvent>,
    /// Text to insert once. Empty when the frame did not commit.
    pub commit: String,
    /// Uncommitted composition. Empty when the platform is not composing.
    pub preedit: String,
    /// Caret range inside [`Self::preedit`], in bytes.
    pub cursor: Option<(usize, usize)>,
    /// `true` after [`ImeEvent::Enabled`] until [`ImeEvent::Disabled`].
    pub enabled: bool,
}

/// Fold `events` in order.
///
/// A preedit drops the text of keys seen since the previous commit, and drops
/// editing keys that arrive while the preedit is non-empty. A commit does the
/// same to keys that led into it, then strips a following key whose text
/// repeats the committed string.
pub fn resolve_ime(events: &[ImeEvent]) -> ResolvedIme {
    let mut keys = Vec::new();
    let mut pending_text = Vec::new();
    let mut commit = String::new();
    let mut preedit = String::new();
    let mut cursor = None;
    let mut enabled = false;
    let mut composing = false;
    for event in events {
        match event {
            ImeEvent::Enabled => enabled = true,
            ImeEvent::Disabled => {
                enabled = false;
                composing = false;
                preedit.clear();
                cursor = None;
                pending_text.clear();
            }
            ImeEvent::Preedit(text, range) => {
                preedit.clone_from(text);
                cursor = *range;
                composing = !text.is_empty();
                if composing {
                    strip_pending(&mut keys, &mut pending_text);
                }
            }
            ImeEvent::Commit(text) => {
                if !text.is_empty() {
                    commit.push_str(text);
                }
                preedit.clear();
                cursor = None;
                composing = false;
                strip_pending(&mut keys, &mut pending_text);
            }
            ImeEvent::Key(event) => {
                if composing && is_composition_key(event) {
                    continue;
                }
                let mut event = event.clone();
                if !commit.is_empty() && text_repeats(event.text.as_deref(), &commit) {
                    event.text = None;
                }
                let has_text = event.text.as_ref().is_some_and(|text| !text.is_empty());
                keys.push(event);
                if has_text {
                    pending_text.push(keys.len() - 1);
                }
            }
        }
    }
    ResolvedIme {
        keys,
        commit,
        preedit,
        cursor,
        enabled,
    }
}

/// Snapshot of one [`Input`] after [`text_frame`] has removed composition duplicates.
#[derive(Clone, Debug, PartialEq)]
pub struct TextFrame {
    /// Keys safe to apply. Composition keys are gone while a preedit is showing.
    pub keys: Vec<KeyEvent>,
    /// Insertable text from keys, with a duplicated commit removed.
    pub chars: String,
    /// IME commit to insert once.
    pub commit: String,
    /// Uncommitted composition.
    pub preedit: String,
    /// Legacy key, or `None` while a preedit is showing.
    pub legacy_key: Option<Key>,
    /// Shift held this frame.
    pub shift: bool,
}

/// Filter `input` for a text widget.
///
/// A non-empty preedit drops editing keys and character text. A non-empty
/// commit drops key text that repeats it. When [`Input::ime_enabled`] is set,
/// a commit drops every key's text, because that text is the key that the
/// input method just turned into the commit.
pub fn text_frame(input: &Input) -> TextFrame {
    let mut keys = input.keys.clone();
    let mut chars = input.chars.clone();
    let commit = input.ime_commit.clone();
    let preedit = input.ime_preedit.clone();
    if !preedit.is_empty() {
        keys.retain(|event| !is_composition_key(event));
        for event in &mut keys {
            event.text = None;
        }
        chars.clear();
    }
    if !commit.is_empty() {
        if input.ime_enabled {
            for event in &mut keys {
                event.text = None;
            }
            chars.clear();
        } else {
            for event in &mut keys {
                if text_repeats(event.text.as_deref(), &commit) {
                    event.text = None;
                }
            }
            if text_repeats(Some(chars.as_str()), &commit) {
                chars.clear();
            }
        }
    }
    let legacy_key = if preedit.is_empty() { input.key } else { None };
    TextFrame {
        keys,
        chars,
        commit,
        preedit,
        legacy_key,
        shift: input.shift,
    }
}

fn strip_pending(keys: &mut [KeyEvent], pending: &mut Vec<usize>) {
    for index in pending.drain(..) {
        if let Some(event) = keys.get_mut(index) {
            event.text = None;
        }
    }
}

fn is_composition_key(event: &KeyEvent) -> bool {
    if event.released || event.synthetic {
        return false;
    }
    if event.modifiers.ctrl || event.modifiers.alt || event.modifiers.super_key {
        return false;
    }
    if event.text.as_ref().is_some_and(|text| !text.is_empty()) {
        return true;
    }
    matches!(
        event.logical,
        LogicalKey::Named(
            NamedKey::Backspace
                | NamedKey::Delete
                | NamedKey::Enter
                | NamedKey::ArrowLeft
                | NamedKey::ArrowRight
                | NamedKey::ArrowUp
                | NamedKey::ArrowDown
                | NamedKey::Space
        )
    )
}

fn text_repeats(text: Option<&str>, commit: &str) -> bool {
    let Some(text) = text.filter(|text| !text.is_empty()) else {
        return false;
    };
    commit == text || commit.ends_with(text) || text.ends_with(commit)
}
