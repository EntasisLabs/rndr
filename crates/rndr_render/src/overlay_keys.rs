//! Keysym and evdev mapping for a Wayland panel seat.
//!
//! The numbers are the XKB keysyms and Linux evdev codes. They do not require
//! a compositor. Text that depends on the live keymap is supplied by the caller.

use rndr_core::{KeyCode, LogicalKey, NamedKey};

/// A key the panel seat can hand to the editor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MappedKey {
    /// Shortcut identity.
    pub logical: LogicalKey,
    /// Characters to insert. Empty on release and when the keysym types nothing.
    pub text: Option<String>,
    /// Physical position when the evdev code is one this table knows.
    pub code: Option<KeyCode>,
}

/// Map one XKB keysym plus the compositor's UTF-8 string.
///
/// `utf8` is used when it is non-empty and the event is a press. Named keys
/// do not insert that string. Returns `None` when nothing identifiable remains.
pub(crate) fn map_keysym(sym: u32, utf8: &str, released: bool) -> Option<MappedKey> {
    let named = named_keysym(sym);
    let logical = if let Some(named) = named {
        LogicalKey::Named(named)
    } else if let Some(ch) = latin1(sym) {
        LogicalKey::Character(ch)
    } else {
        LogicalKey::Unidentified
    };
    let text = if released || named.is_some() {
        None
    } else if utf8.is_empty() {
        latin1(sym).map(|ch| ch.to_string())
    } else {
        Some(utf8.to_string())
    };
    if matches!(logical, LogicalKey::Unidentified) && text.is_none() {
        return None;
    }
    Some(MappedKey {
        logical,
        text,
        code: None,
    })
}

/// Attach a physical code when `evdev` is a key this table knows.
pub(crate) fn with_evdev(mut mapped: MappedKey, evdev: u32) -> MappedKey {
    mapped.code = evdev_code(evdev);
    mapped
}

fn latin1(sym: u32) -> Option<char> {
    if (0x20..=0x7e).contains(&sym) {
        char::from_u32(sym)
    } else {
        None
    }
}

fn named_keysym(sym: u32) -> Option<NamedKey> {
    Some(match sym {
        0xff0d | 0xff8d => NamedKey::Enter,
        0xff1b => NamedKey::Escape,
        0xff09 => NamedKey::Tab,
        0xff08 => NamedKey::Backspace,
        0xffff => NamedKey::Delete,
        0xff63 => NamedKey::Insert,
        0xff50 => NamedKey::Home,
        0xff57 => NamedKey::End,
        0xff55 => NamedKey::PageUp,
        0xff56 => NamedKey::PageDown,
        0xff51 => NamedKey::ArrowLeft,
        0xff52 => NamedKey::ArrowUp,
        0xff53 => NamedKey::ArrowRight,
        0xff54 => NamedKey::ArrowDown,
        0x0020 => NamedKey::Space,
        0xffbe => NamedKey::F(1),
        0xffbf => NamedKey::F(2),
        0xffc0 => NamedKey::F(3),
        0xffc1 => NamedKey::F(4),
        0xffc2 => NamedKey::F(5),
        0xffc3 => NamedKey::F(6),
        0xffc4 => NamedKey::F(7),
        0xffc5 => NamedKey::F(8),
        0xffc6 => NamedKey::F(9),
        0xffc7 => NamedKey::F(10),
        0xffc8 => NamedKey::F(11),
        0xffc9 => NamedKey::F(12),
        _ => return None,
    })
}

fn evdev_code(code: u32) -> Option<KeyCode> {
    Some(match code {
        1 => KeyCode::Escape,
        2 => KeyCode::Digit(1),
        3 => KeyCode::Digit(2),
        4 => KeyCode::Digit(3),
        5 => KeyCode::Digit(4),
        6 => KeyCode::Digit(5),
        7 => KeyCode::Digit(6),
        8 => KeyCode::Digit(7),
        9 => KeyCode::Digit(8),
        10 => KeyCode::Digit(9),
        11 => KeyCode::Digit(0),
        12 => KeyCode::Minus,
        13 => KeyCode::Equal,
        14 => KeyCode::Backspace,
        15 => KeyCode::Tab,
        16 => KeyCode::Letter('q'),
        17 => KeyCode::Letter('w'),
        18 => KeyCode::Letter('e'),
        19 => KeyCode::Letter('r'),
        20 => KeyCode::Letter('t'),
        21 => KeyCode::Letter('y'),
        22 => KeyCode::Letter('u'),
        23 => KeyCode::Letter('i'),
        24 => KeyCode::Letter('o'),
        25 => KeyCode::Letter('p'),
        26 => KeyCode::BracketLeft,
        27 => KeyCode::BracketRight,
        28 | 96 => KeyCode::Enter,
        30 => KeyCode::Letter('a'),
        31 => KeyCode::Letter('s'),
        32 => KeyCode::Letter('d'),
        33 => KeyCode::Letter('f'),
        34 => KeyCode::Letter('g'),
        35 => KeyCode::Letter('h'),
        36 => KeyCode::Letter('j'),
        37 => KeyCode::Letter('k'),
        38 => KeyCode::Letter('l'),
        39 => KeyCode::Semicolon,
        40 => KeyCode::Quote,
        41 => KeyCode::Backquote,
        43 => KeyCode::Backslash,
        44 => KeyCode::Letter('z'),
        45 => KeyCode::Letter('x'),
        46 => KeyCode::Letter('c'),
        47 => KeyCode::Letter('v'),
        48 => KeyCode::Letter('b'),
        49 => KeyCode::Letter('n'),
        50 => KeyCode::Letter('m'),
        51 => KeyCode::Comma,
        52 => KeyCode::Period,
        53 => KeyCode::Slash,
        57 => KeyCode::Space,
        59..=68 => KeyCode::Function((code as u8 - 59) + 1),
        87 => KeyCode::Function(11),
        88 => KeyCode::Function(12),
        102 => KeyCode::Home,
        103 => KeyCode::ArrowUp,
        104 => KeyCode::PageUp,
        105 => KeyCode::ArrowLeft,
        106 => KeyCode::ArrowRight,
        107 => KeyCode::End,
        108 => KeyCode::ArrowDown,
        109 => KeyCode::PageDown,
        110 => KeyCode::Insert,
        111 => KeyCode::Delete,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_is_enter_without_inserting_the_control_string() {
        let mapped = map_keysym(0xff0d, "\r", false).unwrap();
        assert_eq!(mapped.logical, LogicalKey::Named(NamedKey::Enter));
        assert!(mapped.text.is_none());
    }

    #[test]
    fn latin_letter_uses_utf8_when_the_keymap_supplies_it() {
        let mapped = with_evdev(map_keysym(0x0061, "á", false).unwrap(), 30);
        assert_eq!(mapped.logical, LogicalKey::Character('a'));
        assert_eq!(mapped.text.as_deref(), Some("á"));
        assert_eq!(mapped.code, Some(KeyCode::Letter('a')));
    }

    #[test]
    fn release_drops_text() {
        let mapped = map_keysym(0x0061, "a", true).unwrap();
        assert!(mapped.text.is_none());
    }

    #[test]
    fn qwerty_row_matches_evdev() {
        assert_eq!(evdev_code(16), Some(KeyCode::Letter('q')));
        assert_eq!(evdev_code(25), Some(KeyCode::Letter('p')));
        assert_eq!(evdev_code(30), Some(KeyCode::Letter('a')));
        assert_eq!(evdev_code(31), Some(KeyCode::Letter('s')));
        assert_eq!(evdev_code(44), Some(KeyCode::Letter('z')));
        assert_eq!(evdev_code(45), Some(KeyCode::Letter('x')));
        assert_eq!(evdev_code(103), Some(KeyCode::ArrowUp));
    }
}
