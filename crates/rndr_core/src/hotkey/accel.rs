//! Resolved modifiers and the per-platform numbers a backend registers.
//!
//! `primary` is folded into Ctrl or Command before any of these are built.
//! The numbers are the OS contract: Carbon virtual key codes, Win32 `MOD_*`
//! and virtual keys, X11 keysyms, and an XDG shortcut trigger.

use crate::keys::{KeyCode, Keystroke, LogicalKey, NamedKey, Platform, Trigger};

use super::HotkeyError;

/// Carbon `cmdKey`.
pub const MAC_CMD: u32 = 1 << 8;
/// Carbon `shiftKey`.
pub const MAC_SHIFT: u32 = 1 << 9;
/// Carbon `optionKey`.
pub const MAC_OPTION: u32 = 1 << 11;
/// Carbon `controlKey`.
pub const MAC_CONTROL: u32 = 1 << 12;

/// Win32 `MOD_ALT`.
pub const WIN_ALT: u32 = 0x0001;
/// Win32 `MOD_CONTROL`.
pub const WIN_CONTROL: u32 = 0x0002;
/// Win32 `MOD_SHIFT`.
pub const WIN_SHIFT: u32 = 0x0004;
/// Win32 `MOD_WIN`.
pub const WIN_WIN: u32 = 0x0008;

/// X11 `ShiftMask`.
pub const X11_SHIFT: u32 = 1 << 0;
/// X11 `LockMask` (Caps Lock).
pub const X11_LOCK: u32 = 1 << 1;
/// X11 `ControlMask`.
pub const X11_CONTROL: u32 = 1 << 2;
/// X11 `Mod1Mask` (Alt).
pub const X11_ALT: u32 = 1 << 3;
/// X11 `Mod2Mask` (Num Lock on the usual PC layout).
pub const X11_NUM: u32 = 1 << 4;
/// X11 `Mod4Mask` (Super).
pub const X11_SUPER: u32 = 1 << 6;
/// X11 `Mod5Mask` (often Scroll Lock or ISO level 3).
pub const X11_MOD5: u32 = 1 << 7;

/// Shift, Ctrl, Alt, and Super after `primary` is resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResolvedMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
}

/// Key identity shared by every platform encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AccelKey {
    /// ASCII letter, stored lowercase. The physical letter key, not the glyph.
    Letter(char),
    /// Digit key `0` through `9`.
    Digit(u8),
    /// 1 through 24. Carbon only has codes through F20.
    Function(u8),
    /// A named key other than a function key. Function keys use [`Self::Function`].
    Named(NamedKey),
    /// Punctuation named by its US position. See [`Punct`].
    Punct(Punct),
}

/// A punctuation key named by its US position.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Punct {
    /// The `-` key.
    Minus,
    /// The `=` key.
    Equal,
    /// The `[` key.
    BracketLeft,
    /// The `]` key.
    BracketRight,
    /// The backslash key.
    Backslash,
    /// The `;` key.
    Semicolon,
    /// The `'` key.
    Quote,
    /// The `,` key.
    Comma,
    /// The `.` key.
    Period,
    /// The `/` key.
    Slash,
    /// The grave accent key.
    Grave,
}

/// Carbon virtual key code and modifier flags for `RegisterEventHotKey`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MacAccel {
    /// Carbon virtual key code for the key's position.
    pub key_code: u32,
    /// Carbon modifier bits for command, shift, option, and control.
    pub modifiers: u32,
}

/// Win32 virtual key and `MOD_*` flags. The backend adds `MOD_NOREPEAT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WinAccel {
    /// Combined `MOD_ALT`, `MOD_CONTROL`, `MOD_SHIFT`, and `MOD_WIN` bits.
    ///
    /// `MOD_NOREPEAT` is left for the backend to add.
    pub modifiers: u32,
    /// Win32 virtual-key code.
    pub vk: u32,
}

/// X11 keysym, the modifier mask, and the masks to grab.
///
/// `grab_masks` is `modifiers` combined with every subset of Caps Lock,
/// Num Lock, and Mod5, so a lock state does not hide the chord.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct X11Accel {
    /// Keysym of the key, with no modifiers mixed in.
    pub keysym: u32,
    /// Modifier mask of the chord, with the lock bits clear.
    pub modifiers: u32,
    /// Eight masks: `modifiers` combined with every subset of Caps Lock,
    /// Num Lock, and Mod5, so a lock state still matches the grab.
    pub grab_masks: [u32; 8],
}

/// One binding, encoded for every backend that can express it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PlatformAccel {
    /// Shift from the binding. Resolving `primary` leaves this flag alone.
    pub shift: bool,
    /// Ctrl after `primary` is resolved. On macOS `primary` contributes to
    /// [`Self::super_key`]. Elsewhere `primary` contributes to this flag.
    /// An explicit Ctrl on the binding sets it on every platform.
    pub ctrl: bool,
    /// Alt, which is Option on macOS.
    pub alt: bool,
    /// Command, the Windows key, or Super, after `primary` is resolved.
    pub super_key: bool,
    /// Key identity shared by the encodings below.
    pub key: AccelKey,
    /// Carbon encoding. `None` when Carbon has no virtual key for [`Self::key`].
    pub macos: Option<MacAccel>,
    /// Win32 encoding. `None` when Win32 has no virtual key for [`Self::key`].
    pub windows: Option<WinAccel>,
    /// X11 encoding. `None` when there is no keysym for [`Self::key`].
    pub x11: Option<X11Accel>,
    /// XDG shortcuts trigger, for example `ALT+space` or `CTRL+SHIFT+k`.
    pub wayland_trigger: Option<String>,
}

pub(crate) fn accel_key(trigger: Trigger) -> Result<AccelKey, HotkeyError> {
    match trigger {
        Trigger::Logical(LogicalKey::Named(named)) => AccelKey::from_named(named),
        Trigger::Logical(LogicalKey::Character(ch)) => AccelKey::from_char(ch),
        Trigger::Logical(LogicalKey::Unidentified) => Err(HotkeyError::InvalidBinding),
        Trigger::Code(code) => AccelKey::from_code(code),
    }
}

impl AccelKey {
    fn from_named(named: NamedKey) -> Result<Self, HotkeyError> {
        match named {
            NamedKey::F(n) if NamedKey::function(n).is_some() => Ok(Self::Function(n)),
            NamedKey::F(_) => Err(HotkeyError::InvalidBinding),
            other => Ok(Self::Named(other)),
        }
    }

    fn from_char(ch: char) -> Result<Self, HotkeyError> {
        if ch.is_ascii_alphabetic() {
            return Ok(Self::Letter(ch.to_ascii_lowercase()));
        }
        if ch.is_ascii_digit() {
            return Ok(Self::Digit(ch.to_digit(10).unwrap() as u8));
        }
        punct_char(ch)
            .map(Self::Punct)
            .ok_or(HotkeyError::InvalidBinding)
    }

    fn from_code(code: KeyCode) -> Result<Self, HotkeyError> {
        Ok(match code {
            KeyCode::Letter(ch) => Self::Letter(ch.to_ascii_lowercase()),
            KeyCode::Digit(n) if n < 10 => Self::Digit(n),
            KeyCode::Digit(_) => return Err(HotkeyError::InvalidBinding),
            KeyCode::Function(n) => {
                return NamedKey::function(n)
                    .map(|_| Self::Function(n))
                    .ok_or(HotkeyError::InvalidBinding);
            }
            KeyCode::ArrowUp => Self::Named(NamedKey::ArrowUp),
            KeyCode::ArrowDown => Self::Named(NamedKey::ArrowDown),
            KeyCode::ArrowLeft => Self::Named(NamedKey::ArrowLeft),
            KeyCode::ArrowRight => Self::Named(NamedKey::ArrowRight),
            KeyCode::Enter => Self::Named(NamedKey::Enter),
            KeyCode::Escape => Self::Named(NamedKey::Escape),
            KeyCode::Tab => Self::Named(NamedKey::Tab),
            KeyCode::Backspace => Self::Named(NamedKey::Backspace),
            KeyCode::Delete => Self::Named(NamedKey::Delete),
            KeyCode::Insert => Self::Named(NamedKey::Insert),
            KeyCode::Home => Self::Named(NamedKey::Home),
            KeyCode::End => Self::Named(NamedKey::End),
            KeyCode::PageUp => Self::Named(NamedKey::PageUp),
            KeyCode::PageDown => Self::Named(NamedKey::PageDown),
            KeyCode::Space => Self::Named(NamedKey::Space),
            KeyCode::Minus => Self::Punct(Punct::Minus),
            KeyCode::Equal => Self::Punct(Punct::Equal),
            KeyCode::BracketLeft => Self::Punct(Punct::BracketLeft),
            KeyCode::BracketRight => Self::Punct(Punct::BracketRight),
            KeyCode::Backslash => Self::Punct(Punct::Backslash),
            KeyCode::Semicolon => Self::Punct(Punct::Semicolon),
            KeyCode::Quote => Self::Punct(Punct::Quote),
            KeyCode::Comma => Self::Punct(Punct::Comma),
            KeyCode::Period => Self::Punct(Punct::Period),
            KeyCode::Slash => Self::Punct(Punct::Slash),
            KeyCode::Backquote => Self::Punct(Punct::Grave),
        })
    }
}

pub(crate) fn resolve_mods(stroke: &Keystroke, platform: Platform) -> ResolvedMods {
    let mut ctrl = stroke.ctrl;
    let mut super_key = stroke.super_key;
    if stroke.primary {
        if platform.primary_is_super() {
            super_key = true;
        } else {
            ctrl = true;
        }
    }
    ResolvedMods {
        shift: stroke.shift,
        ctrl,
        alt: stroke.alt,
        super_key,
    }
}

/// Encode `binding`'s keystroke for `platform`.
///
/// Fails with [`HotkeyError::InvalidBinding`] when the active platform has no
/// code for the key. F21–F24 are valid on Windows, X11, and Wayland, and
/// invalid on macOS, which has no Carbon virtual key for them.
pub fn platform_accel(
    binding: &super::HotkeyBinding,
    platform: Platform,
) -> Result<PlatformAccel, HotkeyError> {
    let key = accel_key(binding.keystroke().trigger)?;
    let mods = resolve_mods(&binding.keystroke(), platform);
    let macos = mac_accel(key, mods);
    let windows = win_accel(key, mods);
    let x11 = x11_accel(key, mods);
    let wayland_trigger = wayland_trigger(key, mods);
    let ok = match platform {
        Platform::Mac => macos.is_some(),
        Platform::Windows => windows.is_some(),
        Platform::X11 | Platform::Wayland => x11.is_some() && wayland_trigger.is_some(),
    };
    if !ok {
        return Err(HotkeyError::InvalidBinding);
    }
    Ok(PlatformAccel {
        shift: mods.shift,
        ctrl: mods.ctrl,
        alt: mods.alt,
        super_key: mods.super_key,
        key,
        macos,
        windows,
        x11,
        wayland_trigger,
    })
}

fn mac_accel(key: AccelKey, mods: ResolvedMods) -> Option<MacAccel> {
    let key_code = mac_vk(key)?;
    let mut modifiers = 0;
    if mods.super_key {
        modifiers |= MAC_CMD;
    }
    if mods.shift {
        modifiers |= MAC_SHIFT;
    }
    if mods.alt {
        modifiers |= MAC_OPTION;
    }
    if mods.ctrl {
        modifiers |= MAC_CONTROL;
    }
    Some(MacAccel {
        key_code,
        modifiers,
    })
}

fn win_accel(key: AccelKey, mods: ResolvedMods) -> Option<WinAccel> {
    let vk = win_vk(key)?;
    let mut modifiers = 0;
    if mods.alt {
        modifiers |= WIN_ALT;
    }
    if mods.ctrl {
        modifiers |= WIN_CONTROL;
    }
    if mods.shift {
        modifiers |= WIN_SHIFT;
    }
    if mods.super_key {
        modifiers |= WIN_WIN;
    }
    Some(WinAccel { modifiers, vk })
}

fn x11_accel(key: AccelKey, mods: ResolvedMods) -> Option<X11Accel> {
    let keysym = x11_keysym(key)?;
    let mut modifiers = 0;
    if mods.shift {
        modifiers |= X11_SHIFT;
    }
    if mods.ctrl {
        modifiers |= X11_CONTROL;
    }
    if mods.alt {
        modifiers |= X11_ALT;
    }
    if mods.super_key {
        modifiers |= X11_SUPER;
    }
    Some(X11Accel {
        keysym,
        modifiers,
        grab_masks: x11_grab_masks(modifiers),
    })
}

fn x11_grab_masks(base: u32) -> [u32; 8] {
    let extras = [
        0,
        X11_LOCK,
        X11_NUM,
        X11_LOCK | X11_NUM,
        X11_MOD5,
        X11_MOD5 | X11_LOCK,
        X11_MOD5 | X11_NUM,
        X11_MOD5 | X11_LOCK | X11_NUM,
    ];
    let mut masks = [0; 8];
    for (slot, extra) in masks.iter_mut().zip(extras) {
        *slot = base | extra;
    }
    masks
}

fn wayland_trigger(key: AccelKey, mods: ResolvedMods) -> Option<String> {
    let name = wayland_key(key)?;
    let mut parts = Vec::new();
    if mods.ctrl {
        parts.push("CTRL");
    }
    if mods.alt {
        parts.push("ALT");
    }
    if mods.shift {
        parts.push("SHIFT");
    }
    if mods.super_key {
        parts.push("LOGO");
    }
    parts.push(name);
    Some(parts.join("+"))
}

fn mac_vk(key: AccelKey) -> Option<u32> {
    Some(match key {
        AccelKey::Letter(ch) => mac_letter(ch)?,
        AccelKey::Digit(n) => mac_digit(n)?,
        AccelKey::Function(n) => mac_function(n)?,
        AccelKey::Named(named) => mac_named(named)?,
        AccelKey::Punct(punct) => mac_punct(punct),
    })
}

fn mac_letter(ch: char) -> Option<u32> {
    // ANSI virtual key codes from HIToolbox `Events.h`. These are positions.
    Some(match ch.to_ascii_lowercase() {
        'a' => 0x00,
        's' => 0x01,
        'd' => 0x02,
        'f' => 0x03,
        'h' => 0x04,
        'g' => 0x05,
        'z' => 0x06,
        'x' => 0x07,
        'c' => 0x08,
        'v' => 0x09,
        'b' => 0x0B,
        'q' => 0x0C,
        'w' => 0x0D,
        'e' => 0x0E,
        'r' => 0x0F,
        'y' => 0x10,
        't' => 0x11,
        'o' => 0x1F,
        'u' => 0x20,
        'i' => 0x22,
        'p' => 0x23,
        'l' => 0x25,
        'j' => 0x26,
        'k' => 0x28,
        'n' => 0x2D,
        'm' => 0x2E,
        _ => return None,
    })
}

fn mac_digit(n: u8) -> Option<u32> {
    Some(match n {
        0 => 0x1D,
        1 => 0x12,
        2 => 0x13,
        3 => 0x14,
        4 => 0x15,
        5 => 0x17,
        6 => 0x16,
        7 => 0x1A,
        8 => 0x1C,
        9 => 0x19,
        _ => return None,
    })
}

fn mac_function(n: u8) -> Option<u32> {
    Some(match n {
        1 => 0x7A,
        2 => 0x78,
        3 => 0x63,
        4 => 0x76,
        5 => 0x60,
        6 => 0x61,
        7 => 0x62,
        8 => 0x64,
        9 => 0x65,
        10 => 0x6D,
        11 => 0x67,
        12 => 0x6F,
        13 => 0x69,
        14 => 0x6B,
        15 => 0x71,
        16 => 0x6A,
        17 => 0x40,
        18 => 0x4F,
        19 => 0x50,
        20 => 0x5A,
        _ => return None,
    })
}

fn mac_named(named: NamedKey) -> Option<u32> {
    Some(match named {
        NamedKey::Enter => 0x24,
        NamedKey::Tab => 0x30,
        NamedKey::Space => 0x31,
        NamedKey::Backspace => 0x33,
        NamedKey::Escape => 0x35,
        // PC Insert arrives as `kVK_Help` on the ANSI layout.
        NamedKey::Insert => 0x72,
        NamedKey::Home => 0x73,
        NamedKey::PageUp => 0x74,
        NamedKey::Delete => 0x75,
        NamedKey::End => 0x77,
        NamedKey::PageDown => 0x79,
        NamedKey::ArrowLeft => 0x7B,
        NamedKey::ArrowRight => 0x7C,
        NamedKey::ArrowDown => 0x7D,
        NamedKey::ArrowUp => 0x7E,
        NamedKey::F(_) => return None,
    })
}

fn mac_punct(punct: Punct) -> u32 {
    match punct {
        Punct::Minus => 0x1B,
        Punct::Equal => 0x18,
        Punct::BracketLeft => 0x21,
        Punct::BracketRight => 0x1E,
        Punct::Backslash => 0x2A,
        Punct::Semicolon => 0x29,
        Punct::Quote => 0x27,
        Punct::Comma => 0x2B,
        Punct::Period => 0x2F,
        Punct::Slash => 0x2C,
        Punct::Grave => 0x32,
    }
}

fn win_vk(key: AccelKey) -> Option<u32> {
    Some(match key {
        AccelKey::Letter(ch) => u32::from(ch.to_ascii_uppercase() as u8),
        AccelKey::Digit(n) if n < 10 => 0x30 + u32::from(n),
        AccelKey::Digit(_) => return None,
        AccelKey::Function(n) if (1..=24).contains(&n) => 0x6F + u32::from(n),
        AccelKey::Function(_) => return None,
        AccelKey::Named(named) => win_named(named)?,
        AccelKey::Punct(punct) => win_punct(punct),
    })
}

fn win_named(named: NamedKey) -> Option<u32> {
    Some(match named {
        NamedKey::Backspace => 0x08,
        NamedKey::Tab => 0x09,
        NamedKey::Enter => 0x0D,
        NamedKey::Escape => 0x1B,
        NamedKey::Space => 0x20,
        NamedKey::PageUp => 0x21,
        NamedKey::PageDown => 0x22,
        NamedKey::End => 0x23,
        NamedKey::Home => 0x24,
        NamedKey::ArrowLeft => 0x25,
        NamedKey::ArrowUp => 0x26,
        NamedKey::ArrowRight => 0x27,
        NamedKey::ArrowDown => 0x28,
        NamedKey::Insert => 0x2D,
        NamedKey::Delete => 0x2E,
        NamedKey::F(_) => return None,
    })
}

fn win_punct(punct: Punct) -> u32 {
    match punct {
        Punct::Semicolon => 0xBA,
        Punct::Equal => 0xBB,
        Punct::Comma => 0xBC,
        Punct::Minus => 0xBD,
        Punct::Period => 0xBE,
        Punct::Slash => 0xBF,
        Punct::Grave => 0xC0,
        Punct::BracketLeft => 0xDB,
        Punct::Backslash => 0xDC,
        Punct::BracketRight => 0xDD,
        Punct::Quote => 0xDE,
    }
}

fn x11_keysym(key: AccelKey) -> Option<u32> {
    Some(match key {
        AccelKey::Letter(ch) => u32::from(ch.to_ascii_lowercase() as u8),
        AccelKey::Digit(n) if n < 10 => 0x30 + u32::from(n),
        AccelKey::Digit(_) => return None,
        // `XK_F1` is 0xffbe; the function keys are contiguous through F35.
        AccelKey::Function(n) if (1..=24).contains(&n) => 0xffbe + u32::from(n - 1),
        AccelKey::Function(_) => return None,
        AccelKey::Named(named) => x11_named(named)?,
        AccelKey::Punct(punct) => x11_punct(punct),
    })
}

fn x11_named(named: NamedKey) -> Option<u32> {
    Some(match named {
        NamedKey::Space => 0x0020,
        NamedKey::Backspace => 0xff08,
        NamedKey::Tab => 0xff09,
        NamedKey::Enter => 0xff0d,
        NamedKey::Escape => 0xff1b,
        NamedKey::Home => 0xff50,
        NamedKey::ArrowLeft => 0xff51,
        NamedKey::ArrowUp => 0xff52,
        NamedKey::ArrowRight => 0xff53,
        NamedKey::ArrowDown => 0xff54,
        NamedKey::PageUp => 0xff55,
        NamedKey::PageDown => 0xff56,
        NamedKey::End => 0xff57,
        NamedKey::Insert => 0xff63,
        NamedKey::Delete => 0xffff,
        NamedKey::F(_) => return None,
    })
}

fn x11_punct(punct: Punct) -> u32 {
    match punct {
        Punct::Minus => 0x002d,
        Punct::Equal => 0x003d,
        Punct::BracketLeft => 0x005b,
        Punct::BracketRight => 0x005d,
        Punct::Backslash => 0x005c,
        Punct::Semicolon => 0x003b,
        Punct::Quote => 0x0027,
        Punct::Comma => 0x002c,
        Punct::Period => 0x002e,
        Punct::Slash => 0x002f,
        Punct::Grave => 0x0060,
    }
}

fn wayland_key(key: AccelKey) -> Option<&'static str> {
    Some(match key {
        AccelKey::Letter(ch) => return wayland_letter(ch),
        AccelKey::Digit(n) => return wayland_digit(n),
        AccelKey::Function(n) if (1..=24).contains(&n) => return wayland_function(n),
        AccelKey::Function(_) => return None,
        AccelKey::Named(NamedKey::Space) => "space",
        AccelKey::Named(NamedKey::Enter) => "Return",
        AccelKey::Named(NamedKey::Escape) => "Escape",
        AccelKey::Named(NamedKey::Tab) => "Tab",
        AccelKey::Named(NamedKey::Backspace) => "BackSpace",
        AccelKey::Named(NamedKey::Delete) => "Delete",
        AccelKey::Named(NamedKey::Insert) => "Insert",
        AccelKey::Named(NamedKey::Home) => "Home",
        AccelKey::Named(NamedKey::End) => "End",
        AccelKey::Named(NamedKey::PageUp) => "Page_Up",
        AccelKey::Named(NamedKey::PageDown) => "Page_Down",
        AccelKey::Named(NamedKey::ArrowLeft) => "Left",
        AccelKey::Named(NamedKey::ArrowRight) => "Right",
        AccelKey::Named(NamedKey::ArrowUp) => "Up",
        AccelKey::Named(NamedKey::ArrowDown) => "Down",
        AccelKey::Named(NamedKey::F(_)) => return None,
        AccelKey::Punct(Punct::Minus) => "minus",
        AccelKey::Punct(Punct::Equal) => "equal",
        AccelKey::Punct(Punct::BracketLeft) => "bracketleft",
        AccelKey::Punct(Punct::BracketRight) => "bracketright",
        AccelKey::Punct(Punct::Backslash) => "backslash",
        AccelKey::Punct(Punct::Semicolon) => "semicolon",
        AccelKey::Punct(Punct::Quote) => "apostrophe",
        AccelKey::Punct(Punct::Comma) => "comma",
        AccelKey::Punct(Punct::Period) => "period",
        AccelKey::Punct(Punct::Slash) => "slash",
        AccelKey::Punct(Punct::Grave) => "grave",
    })
}

fn wayland_letter(ch: char) -> Option<&'static str> {
    Some(match ch.to_ascii_lowercase() {
        'a' => "a",
        'b' => "b",
        'c' => "c",
        'd' => "d",
        'e' => "e",
        'f' => "f",
        'g' => "g",
        'h' => "h",
        'i' => "i",
        'j' => "j",
        'k' => "k",
        'l' => "l",
        'm' => "m",
        'n' => "n",
        'o' => "o",
        'p' => "p",
        'q' => "q",
        'r' => "r",
        's' => "s",
        't' => "t",
        'u' => "u",
        'v' => "v",
        'w' => "w",
        'x' => "x",
        'y' => "y",
        'z' => "z",
        _ => return None,
    })
}

fn wayland_digit(n: u8) -> Option<&'static str> {
    Some(match n {
        0 => "0",
        1 => "1",
        2 => "2",
        3 => "3",
        4 => "4",
        5 => "5",
        6 => "6",
        7 => "7",
        8 => "8",
        9 => "9",
        _ => return None,
    })
}

fn wayland_function(n: u8) -> Option<&'static str> {
    Some(match n {
        1 => "F1",
        2 => "F2",
        3 => "F3",
        4 => "F4",
        5 => "F5",
        6 => "F6",
        7 => "F7",
        8 => "F8",
        9 => "F9",
        10 => "F10",
        11 => "F11",
        12 => "F12",
        13 => "F13",
        14 => "F14",
        15 => "F15",
        16 => "F16",
        17 => "F17",
        18 => "F18",
        19 => "F19",
        20 => "F20",
        21 => "F21",
        22 => "F22",
        23 => "F23",
        24 => "F24",
        _ => return None,
    })
}

fn punct_char(ch: char) -> Option<Punct> {
    Some(match ch {
        '-' => Punct::Minus,
        '=' => Punct::Equal,
        '[' => Punct::BracketLeft,
        ']' => Punct::BracketRight,
        '\\' => Punct::Backslash,
        ';' => Punct::Semicolon,
        '\'' => Punct::Quote,
        ',' => Punct::Comma,
        '.' => Punct::Period,
        '/' => Punct::Slash,
        '`' => Punct::Grave,
        _ => return None,
    })
}
