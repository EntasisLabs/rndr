//! System-wide hotkeys.
//!
//! A binding uses the same modifiers and keys as a [`Keystroke`](crate::Keystroke):
//! `primary+Space`, `Alt+Space`, `Ctrl+Shift+K`, or a [`Keystroke`] built in
//! code. `primary` is Command on macOS and Ctrl on Windows, X11, and Wayland.
//!
//! [`register`] talks to a [`HotkeyBackend`]. The desktop runner installs the
//! operating-system backend (behind the `hotkey` feature, on by default in
//! `rndr_render`). Headless tests install a fake. With no backend,
//! [`register`] returns [`HotkeyError::Unsupported`].
//!
//! A press is queued by the backend and delivered as [`GlobalHotkey`] on the
//! next frame. The runner wakes the winit loop, which stays on `Wait` the rest
//! of the time. [`toggle_window`] is the launcher case: the chord shows and
//! focuses a hidden window, or hides a visible one. A key event that matches a
//! registered chord, including its repeats, is removed from that frame's text
//! input. Once the chord fires, its base key is ignored while that press is
//! still held. The guard clears when the backend reports the chord released,
//! when a physical key-up arrives after the hotkey, or when the window hides.
//! A later non-repeat of the base key is a new press: it clears the guard and
//! is inserted. Only auto-repeat of the held chord stays out. A synthetic
//! focus event (`is_synthetic` on X11 and Windows; macOS and Wayland never set
//! it) is not text and does not release the guard. A key-up the runner had
//! already received before the hotkey cannot release it either, so a Space
//! queued while the window was hidden does not let the focus-in Space into
//! the field. Key releases are removed as well, so a key-up cannot insert text.
//!
//! Re-registering a chord that this process already owns returns another
//! handle for the same id and does not grab twice. Dropping a handle, or
//! calling [`HotkeyHandle::unregister`], releases one owner. The last owner
//! releases the operating-system grab.
//!
//! ## Platform support
//!
//! | | macOS | Windows | X11 | Wayland |
//! | --- | --- | --- | --- | --- |
//! | Mechanism | Carbon `RegisterEventHotKey` | `RegisterHotKey` | `XGrabKey` on the root window | XDG GlobalShortcuts portal |
//! | Works while another app is focused | yes | yes | yes | yes, after the portal grants it |
//! | Permission | none (no Accessibility prompt) | none | none | a portal dialog the user can edit |
//! | Chord already owned | `AlreadyTaken` | `AlreadyTaken` | `AlreadyTaken` (`BadAccess`) | portal declines the grant (`AlreadyTaken`) |
//! | Unavailable session | — | — | no X server (`Unsupported`) | no portal (`Unsupported`); X11 is not used as a fallback |
//! | Lock keys | ignored by Carbon | ignored by `RegisterHotKey` | grabbed again with Caps Lock, Num Lock, and Mod5 | portal / compositor |
//! | Key repeat | one event per press | `MOD_NOREPEAT` | first press only; a held key is a repeat | one `Activated` until `Deactivated` |
//! | Idle | the Cocoa run loop is already waiting | the hotkey thread blocks in `GetMessage` | the hotkey thread blocks in `poll` | the portal thread blocks on D-Bus |
//!
//! F21–F24 have no Carbon virtual key, so they are [`HotkeyError::InvalidBinding`]
//! on macOS and valid elsewhere. On Wayland the portal may show a system dialog
//! and the user may change the trigger; the id still fires when that trigger
//! is pressed. Each registration is its own portal session, because a session
//! can only be bound once.
//!
//! `rndr_core` does not link Carbon, Win32, X11, or a D-Bus stack. Those live
//! in `rndr_render` and compile only with its `hotkey` feature.

mod accel;
mod registry;

use std::sync::Arc;

use crate::event::{self, Event};
use crate::keys::{KeyCode, Keystroke, NamedKey};
use crate::world::World;

pub use accel::{platform_accel, AccelKey, MacAccel, PlatformAccel, Punct, WinAccel, X11Accel};
pub use registry::{
    note_key_order, GlobalHotkey, HotkeyBackend, HotkeyEdge, HotkeyFailure, HotkeyHandle, HotkeyId,
};

pub(crate) use registry::disarm_guard;

pub(crate) use registry::drive;

/// Why a hotkey was not registered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyError {
    /// Another client already owns this chord, or the portal did not grant it.
    AlreadyTaken,
    /// No backend is installed, or this session cannot bind global hotkeys.
    /// On Wayland this is a missing XDG GlobalShortcuts portal.
    Unsupported,
    /// The text or keystroke is not a single chord this platform can encode.
    InvalidBinding,
}

impl std::fmt::Display for HotkeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::AlreadyTaken => "another application already owns this hotkey",
            Self::Unsupported => "global hotkeys are not supported on this platform",
            Self::InvalidBinding => "invalid hotkey binding",
        })
    }
}

impl std::error::Error for HotkeyError {}

/// A chord the app asked for that the platform did not grab.
///
/// Stored as a resource as soon as [`note_unregistered`] runs, and delivered
/// as an event on the next frame, so startup and Update can both show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HotkeyRejected {
    pub binding: String,
    pub error: HotkeyError,
}

impl Event for HotkeyRejected {}

/// Log `error` and keep running. The process does not exit.
pub fn note_unregistered(world: &mut World, binding: &str, error: HotkeyError) {
    eprintln!("warning: hotkey {binding}: {error}; continuing without it");
    let notice = HotkeyRejected {
        binding: binding.to_string(),
        error,
    };
    world.insert_resource(notice.clone());
    event::send(world, notice);
}

/// One chord in keymap vocabulary. Global hotkeys are a single keystroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyBinding {
    stroke: Keystroke,
}

impl HotkeyBinding {
    pub fn parse(text: &str) -> Result<Self, HotkeyError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(HotkeyError::InvalidBinding);
        }
        let parts: Vec<&str> = text.split('+').map(str::trim).collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(HotkeyError::InvalidBinding);
        }
        let (key, modifiers) = parts.split_last().expect("non-empty binding");
        let mut stroke = key_token(key)?;
        for modifier in modifiers {
            stroke = apply_modifier(stroke, modifier)?;
        }
        Self::from_keystroke(stroke)
    }

    pub fn from_keystroke(stroke: Keystroke) -> Result<Self, HotkeyError> {
        accel::accel_key(stroke.trigger)?;
        Ok(Self { stroke })
    }

    pub fn keystroke(self) -> Keystroke {
        self.stroke
    }
}

impl std::str::FromStr for HotkeyBinding {
    type Err = HotkeyError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text)
    }
}

/// Anything [`register`] accepts.
pub trait IntoHotkey {
    fn into_hotkey(self) -> Result<HotkeyBinding, HotkeyError>;
}

impl IntoHotkey for HotkeyBinding {
    fn into_hotkey(self) -> Result<HotkeyBinding, HotkeyError> {
        Ok(self)
    }
}

impl IntoHotkey for Keystroke {
    fn into_hotkey(self) -> Result<HotkeyBinding, HotkeyError> {
        HotkeyBinding::from_keystroke(self)
    }
}

impl IntoHotkey for &str {
    fn into_hotkey(self) -> Result<HotkeyBinding, HotkeyError> {
        HotkeyBinding::parse(self)
    }
}

impl IntoHotkey for String {
    fn into_hotkey(self) -> Result<HotkeyBinding, HotkeyError> {
        HotkeyBinding::parse(&self)
    }
}

/// Install `backend`. The first call wins; a later call leaves the original in place.
pub fn install_backend(world: &mut World, backend: impl HotkeyBackend + 'static) {
    registry::install(world, backend);
}

/// Whether [`install_backend`] has already run.
pub fn backend_installed(world: &World) -> bool {
    registry::installed(world)
}

/// Grab `binding` until the returned handle is dropped or unregistered.
pub fn register(world: &World, binding: impl IntoHotkey) -> Result<HotkeyHandle, HotkeyError> {
    registry::register(world, binding, false)
}

/// Grab `binding` and toggle the window on each press.
///
/// A hidden window is shown and focused. A visible window is hidden. The
/// press is also a [`GlobalHotkey`] on that frame.
pub fn toggle_window(world: &World, binding: impl IntoHotkey) -> Result<HotkeyHandle, HotkeyError> {
    registry::register(world, binding, true)
}

/// Wake closure aimed at the runner's slot.
///
/// [`crate::App::set_wake`] replaces the slot's target later, and this closure
/// sees that replacement. The hotkey thread calls it and then blocks again.
pub fn frame_wake(world: &World) -> Arc<dyn Fn() + Send + Sync> {
    let slot = crate::runtime::wake_slot(world);
    Arc::new(move || crate::runtime::lock(&slot).wake())
}

fn key_token(token: &str) -> Result<Keystroke, HotkeyError> {
    let lower = token.to_ascii_lowercase();
    if let Some(named) = named_token(&lower) {
        return Ok(Keystroke::named(named));
    }
    if let Some(code) = punct_token(&lower) {
        return Ok(Keystroke::code(code));
    }
    let mut chars = lower.chars();
    if let (Some(ch), None) = (chars.next(), chars.next()) {
        if ch.is_ascii_alphabetic() {
            return Ok(Keystroke::character(ch));
        }
        if let Some(digit) = ch.to_digit(10) {
            return Ok(Keystroke::code(
                KeyCode::digit(digit as u8).ok_or(HotkeyError::InvalidBinding)?,
            ));
        }
        if let Some(punct) = punct_symbol(ch) {
            return Ok(Keystroke::code(punct));
        }
    }
    if let Some(rest) = lower.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u8>() {
            if NamedKey::function(n).is_some() {
                return Ok(Keystroke::named(NamedKey::F(n)));
            }
        }
    }
    Err(HotkeyError::InvalidBinding)
}

fn apply_modifier(mut stroke: Keystroke, token: &str) -> Result<Keystroke, HotkeyError> {
    match token.to_ascii_lowercase().as_str() {
        "primary" => stroke.primary = true,
        "ctrl" | "control" => stroke.ctrl = true,
        "alt" | "option" | "opt" => stroke.alt = true,
        "shift" => stroke.shift = true,
        "super" | "cmd" | "command" | "win" | "windows" | "meta" | "logo" => {
            stroke.super_key = true;
        }
        _ => return Err(HotkeyError::InvalidBinding),
    }
    Ok(stroke)
}

fn named_token(lower: &str) -> Option<NamedKey> {
    Some(match lower {
        "space" | "spacebar" => NamedKey::Space,
        "enter" | "return" => NamedKey::Enter,
        "esc" | "escape" => NamedKey::Escape,
        "tab" => NamedKey::Tab,
        "backspace" | "bksp" => NamedKey::Backspace,
        "delete" | "del" => NamedKey::Delete,
        "insert" | "ins" => NamedKey::Insert,
        "home" => NamedKey::Home,
        "end" => NamedKey::End,
        "pageup" | "page_up" | "pgup" => NamedKey::PageUp,
        "pagedown" | "page_down" | "pgdn" => NamedKey::PageDown,
        "up" | "arrowup" => NamedKey::ArrowUp,
        "down" | "arrowdown" => NamedKey::ArrowDown,
        "left" | "arrowleft" => NamedKey::ArrowLeft,
        "right" | "arrowright" => NamedKey::ArrowRight,
        _ => return None,
    })
}

fn punct_token(lower: &str) -> Option<KeyCode> {
    Some(match lower {
        "minus" => KeyCode::Minus,
        "equal" | "equals" | "plus" => KeyCode::Equal,
        "bracketleft" => KeyCode::BracketLeft,
        "bracketright" => KeyCode::BracketRight,
        "backslash" => KeyCode::Backslash,
        "semicolon" => KeyCode::Semicolon,
        "quote" | "apostrophe" => KeyCode::Quote,
        "comma" => KeyCode::Comma,
        "period" => KeyCode::Period,
        "slash" => KeyCode::Slash,
        "grave" | "backquote" => KeyCode::Backquote,
        _ => return None,
    })
}

fn punct_symbol(ch: char) -> Option<KeyCode> {
    Some(match ch {
        '-' => KeyCode::Minus,
        '=' => KeyCode::Equal,
        '[' => KeyCode::BracketLeft,
        ']' => KeyCode::BracketRight,
        '\\' => KeyCode::Backslash,
        ';' => KeyCode::Semicolon,
        '\'' => KeyCode::Quote,
        ',' => KeyCode::Comma,
        '.' => KeyCode::Period,
        '/' => KeyCode::Slash,
        '`' => KeyCode::Backquote,
        _ => return None,
    })
}
