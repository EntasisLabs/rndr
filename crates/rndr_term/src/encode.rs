//! Key and mouse bytes written to the terminal.
//!
//! Sequences follow the common VT keyboard and mouse reports: CSI / SS3 for
//! editing keys, a single byte for Ctrl+letter, an ESC prefix for Alt, and
//! SGR or normal mouse encoding when the application asks for it.

use rndr_core::{KeyCode, KeyEvent, LogicalKey, Modifiers, NamedKey};

/// Modes that change which bytes a key produces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncodeMode {
    /// Application cursor keys. Arrows, Home, and End use SS3 when unmodified.
    pub app_cursor: bool,
    /// Application keypad. Reserved; this keyboard has no keypad distinct from the digits.
    pub app_keypad: bool,
    /// Newline mode. Enter sends CR LF instead of CR.
    pub newline: bool,
}

/// Button reported to an application that asked for mouse tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    /// Primary button.
    Left,
    /// Wheel press.
    Middle,
    /// Secondary button.
    Right,
    /// Wheel away from the user.
    WheelUp,
    /// Wheel toward the user.
    WheelDown,
    /// Motion with no button held.
    None,
}

/// What the pointer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseAction {
    /// Button went down, or a wheel notch.
    Press,
    /// Button came up.
    Release,
    /// Pointer moved.
    Move,
}

/// Which mouse report the application enabled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MouseProtocol {
    /// The host selects text and scrolls. Nothing is written.
    #[default]
    Off,
    /// Button presses.
    Click,
    /// Presses and motion while a button is held.
    Drag,
    /// Presses and any motion.
    Any,
}

/// How mouse coordinates are encoded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEncode {
    /// Decimal SGR reports (`CSI < Cb ; Cx ; Cy M/m`). Otherwise the normal byte form.
    pub sgr: bool,
    /// Which events the application wants.
    pub protocol: MouseProtocol,
}

impl Default for MouseEncode {
    fn default() -> Self {
        Self {
            sgr: false,
            protocol: MouseProtocol::Off,
        }
    }
}

/// Bytes for one key, or empty when the key should not be written.
///
/// Releases and synthesized focus events produce nothing. Super chords produce
/// nothing here; the session handles copy and paste before encoding. Ctrl+letter
/// is the corresponding control byte. Alt prefixes ESC.
pub fn encode_key(event: &KeyEvent, mode: EncodeMode) -> Vec<u8> {
    if event.released || event.synthetic || event.modifiers.super_key {
        return Vec::new();
    }
    let mods = event.modifiers;
    if mods.ctrl {
        if let Some(byte) = control_byte(event) {
            return prefixed(mods.alt, vec![byte]);
        }
    }
    if let Some(bytes) = named_key(event, mode) {
        return bytes;
    }
    if let Some(text) = event.text.as_ref() {
        if text.is_empty() {
            return Vec::new();
        }
        if mods.alt && text.chars().all(|ch| ch.is_ascii() && !ch.is_control()) {
            let mut out = vec![0x1b];
            out.extend(text.as_bytes());
            return out;
        }
        return text.as_bytes().to_vec();
    }
    Vec::new()
}

/// Bracketed or plain paste payload.
///
/// ESC bytes in `text` are dropped so a paste cannot close the bracket early.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    let mut body = Vec::with_capacity(text.len());
    for ch in text.chars() {
        if ch == '\u{1b}' || ch == '\u{9b}' {
            continue;
        }
        let mut buf = [0u8; 4];
        body.extend(ch.encode_utf8(&mut buf).as_bytes());
    }
    if !bracketed {
        return body;
    }
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend(b"\x1b[200~");
    out.extend(body);
    out.extend(b"\x1b[201~");
    out
}

/// Mouse report, or empty when `encode.protocol` does not want this action.
///
/// `col` and `row` are 0-based viewport cells. The report uses 1-based coordinates.
pub fn encode_mouse(
    button: MouseButton,
    action: MouseAction,
    col: u16,
    row: u16,
    mods: Modifiers,
    encode: MouseEncode,
) -> Vec<u8> {
    if !wants_mouse(button, action, encode.protocol) {
        return Vec::new();
    }
    let mut code = mouse_code(button, action, mods);
    if !encode.sgr && action == MouseAction::Release && !is_wheel(button) {
        code = 3;
        if mods.shift {
            code += 4;
        }
        if mods.alt {
            code += 8;
        }
        if mods.ctrl {
            code += 16;
        }
    }
    let x = col.saturating_add(1);
    let y = row.saturating_add(1);
    if encode.sgr {
        let suffix = if action == MouseAction::Release && !is_wheel(button) {
            'm'
        } else {
            'M'
        };
        return format!("\x1b[<{code};{x};{y}{suffix}").into_bytes();
    }
    let x = x.min(223);
    let y = y.min(223);
    let cb = code.saturating_add(32).min(255) as u8;
    vec![
        0x1b,
        b'[',
        b'M',
        cb,
        32u8.saturating_add(x as u8),
        32u8.saturating_add(y as u8),
    ]
}

fn wants_mouse(button: MouseButton, action: MouseAction, protocol: MouseProtocol) -> bool {
    match protocol {
        MouseProtocol::Off => false,
        MouseProtocol::Click => action != MouseAction::Move,
        MouseProtocol::Drag => match action {
            MouseAction::Move => !matches!(
                button,
                MouseButton::None | MouseButton::WheelUp | MouseButton::WheelDown
            ),
            _ => true,
        },
        MouseProtocol::Any => true,
    }
}

fn is_wheel(button: MouseButton) -> bool {
    matches!(button, MouseButton::WheelUp | MouseButton::WheelDown)
}

fn mouse_code(button: MouseButton, action: MouseAction, mods: Modifiers) -> u16 {
    let base: u16 = match button {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
        MouseButton::None => 3,
        MouseButton::WheelUp => 64,
        MouseButton::WheelDown => 65,
    };
    let mut code = base;
    if action == MouseAction::Move && !is_wheel(button) {
        code += 32;
    }
    if action == MouseAction::Release && !is_wheel(button) && base < 3 {
        // Normal tracking reports release as button 3. SGR uses the same
        // button number with a different final byte; the caller swaps that.
        // Keep the button number here so SGR stays accurate. Normal encoding
        // replaces a release below 3 with button 3.
    }
    if mods.shift {
        code += 4;
    }
    if mods.alt {
        code += 8;
    }
    if mods.ctrl {
        code += 16;
    }
    code
}

fn prefixed(alt: bool, mut bytes: Vec<u8>) -> Vec<u8> {
    if alt {
        bytes.insert(0, 0x1b);
    }
    bytes
}

fn control_byte(event: &KeyEvent) -> Option<u8> {
    if let Some(letter) = event_letter(event) {
        return Some(letter as u8 - b'a' + 1);
    }
    match event.logical {
        LogicalKey::Named(NamedKey::Space) => Some(0),
        _ => match event.code {
            Some(KeyCode::Space) => Some(0),
            _ => None,
        },
    }
}

fn event_letter(event: &KeyEvent) -> Option<char> {
    match event.logical {
        LogicalKey::Character(ch) if ch.is_ascii_alphabetic() => Some(ch.to_ascii_lowercase()),
        _ => match event.code {
            Some(KeyCode::Letter(ch)) => Some(ch.to_ascii_lowercase()),
            _ => None,
        },
    }
}

fn named_key(event: &KeyEvent, mode: EncodeMode) -> Option<Vec<u8>> {
    let LogicalKey::Named(key) = event.logical else {
        return None;
    };
    let mods = event.modifiers;
    let param = modifier_param(mods);
    let bytes = match key {
        NamedKey::Enter => {
            let bytes = if mode.newline {
                vec![b'\r', b'\n']
            } else {
                vec![b'\r']
            };
            prefixed(mods.alt, bytes)
        }
        NamedKey::Tab if mods.shift && !mods.ctrl => b"\x1b[Z".to_vec(),
        NamedKey::Tab if mods.ctrl => format!("\x1b[27;{param};9~").into_bytes(),
        NamedKey::Tab => prefixed(mods.alt, vec![b'\t']),
        NamedKey::Backspace => {
            let byte = if mods.ctrl { 0x08 } else { 0x7f };
            prefixed(mods.alt, vec![byte])
        }
        NamedKey::Escape => prefixed(mods.alt, vec![0x1b]),
        NamedKey::ArrowUp => arrow(b'A', param, mode.app_cursor),
        NamedKey::ArrowDown => arrow(b'B', param, mode.app_cursor),
        NamedKey::ArrowRight => arrow(b'C', param, mode.app_cursor),
        NamedKey::ArrowLeft => arrow(b'D', param, mode.app_cursor),
        NamedKey::Home => home_end(b'H', param, mode.app_cursor),
        NamedKey::End => home_end(b'F', param, mode.app_cursor),
        NamedKey::Insert => tilde(2, param),
        NamedKey::Delete => tilde(3, param),
        NamedKey::PageUp => tilde(5, param),
        NamedKey::PageDown => tilde(6, param),
        NamedKey::Space => prefixed(mods.alt, vec![b' ']),
        NamedKey::F(n) => function_key(n, param)?,
    };
    Some(bytes)
}

fn modifier_param(mods: Modifiers) -> u8 {
    let mut param = 1u8;
    if mods.shift {
        param += 1;
    }
    if mods.alt {
        param += 2;
    }
    if mods.ctrl {
        param += 4;
    }
    param
}

fn arrow(letter: u8, param: u8, app_cursor: bool) -> Vec<u8> {
    if param == 1 && app_cursor {
        return vec![0x1b, b'O', letter];
    }
    if param == 1 {
        return vec![0x1b, b'[', letter];
    }
    format!("\x1b[1;{param}{}", letter as char).into_bytes()
}

fn home_end(letter: u8, param: u8, app_cursor: bool) -> Vec<u8> {
    if param == 1 && app_cursor {
        return vec![0x1b, b'O', letter];
    }
    if param == 1 {
        return vec![0x1b, b'[', letter];
    }
    format!("\x1b[1;{param}{}", letter as char).into_bytes()
}

fn tilde(number: u8, param: u8) -> Vec<u8> {
    if param == 1 {
        format!("\x1b[{number}~").into_bytes()
    } else {
        format!("\x1b[{number};{param}~").into_bytes()
    }
}

fn function_key(n: u8, param: u8) -> Option<Vec<u8>> {
    if !(1..=24).contains(&n) {
        return None;
    }
    if param == 1 {
        let seq: &[u8] = match n {
            1 => b"\x1bOP",
            2 => b"\x1bOQ",
            3 => b"\x1bOR",
            4 => b"\x1bOS",
            5 => b"\x1b[15~",
            6 => b"\x1b[17~",
            7 => b"\x1b[18~",
            8 => b"\x1b[19~",
            9 => b"\x1b[20~",
            10 => b"\x1b[21~",
            11 => b"\x1b[23~",
            12 => b"\x1b[24~",
            13 => b"\x1b[25~",
            14 => b"\x1b[26~",
            15 => b"\x1b[28~",
            16 => b"\x1b[29~",
            17 => b"\x1b[31~",
            18 => b"\x1b[32~",
            19 => b"\x1b[33~",
            20 => b"\x1b[34~",
            21 => b"\x1b[23$",
            22 => b"\x1b[24$",
            23 => b"\x1b[11^",
            24 => b"\x1b[12^",
            _ => return None,
        };
        return Some(seq.to_vec());
    }
    let (number, suffix): (&str, &str) = match n {
        1 => ("1", "P"),
        2 => ("1", "Q"),
        3 => ("1", "R"),
        4 => ("1", "S"),
        5 => ("15", "~"),
        6 => ("17", "~"),
        7 => ("18", "~"),
        8 => ("19", "~"),
        9 => ("20", "~"),
        10 => ("21", "~"),
        11 => ("23", "~"),
        12 => ("24", "~"),
        13 => ("25", "~"),
        14 => ("26", "~"),
        15 => ("28", "~"),
        16 => ("29", "~"),
        17 => ("31", "~"),
        18 => ("32", "~"),
        19 => ("33", "~"),
        20 => ("34", "~"),
        _ => return Some(Vec::new()),
    };
    Some(format!("\x1b[{number};{param}{suffix}").into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rndr_core::NamedKey;

    fn key(named: NamedKey) -> KeyEvent {
        KeyEvent::named(named, Modifiers::default())
    }

    fn with_mods(event: KeyEvent, mods: Modifiers) -> KeyEvent {
        KeyEvent {
            modifiers: mods,
            ..event
        }
    }

    #[test]
    fn printable_enter_and_editing_keys() {
        let mode = EncodeMode::default();
        let letter = KeyEvent::character('a', Modifiers::default());
        assert_eq!(encode_key(&letter, mode), b"a");
        assert_eq!(encode_key(&key(NamedKey::Enter), mode), b"\r");
        assert_eq!(
            encode_key(
                &key(NamedKey::Enter),
                EncodeMode {
                    newline: true,
                    ..mode
                }
            ),
            b"\r\n"
        );
        assert_eq!(encode_key(&key(NamedKey::Backspace), mode), b"\x7f");
        assert_eq!(encode_key(&key(NamedKey::Tab), mode), b"\t");
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            encode_key(&with_mods(key(NamedKey::Tab), shift), mode),
            b"\x1b[Z"
        );
    }

    #[test]
    fn arrows_home_end_pages_and_function_keys() {
        let mode = EncodeMode::default();
        assert_eq!(encode_key(&key(NamedKey::ArrowUp), mode), b"\x1b[A");
        assert_eq!(encode_key(&key(NamedKey::ArrowDown), mode), b"\x1b[B");
        assert_eq!(encode_key(&key(NamedKey::ArrowRight), mode), b"\x1b[C");
        assert_eq!(encode_key(&key(NamedKey::ArrowLeft), mode), b"\x1b[D");
        assert_eq!(
            encode_key(
                &key(NamedKey::ArrowLeft),
                EncodeMode {
                    app_cursor: true,
                    ..mode
                }
            ),
            b"\x1bOD"
        );
        assert_eq!(encode_key(&key(NamedKey::Home), mode), b"\x1b[H");
        assert_eq!(encode_key(&key(NamedKey::End), mode), b"\x1b[F");
        assert_eq!(encode_key(&key(NamedKey::PageUp), mode), b"\x1b[5~");
        assert_eq!(encode_key(&key(NamedKey::PageDown), mode), b"\x1b[6~");
        assert_eq!(encode_key(&key(NamedKey::F(5)), mode), b"\x1b[15~");
        assert_eq!(encode_key(&key(NamedKey::F(1)), mode), b"\x1bOP");
        let shift = Modifiers {
            shift: true,
            ..Modifiers::default()
        };
        assert_eq!(
            encode_key(&with_mods(key(NamedKey::ArrowUp), shift), mode),
            b"\x1b[1;2A"
        );
    }

    #[test]
    fn ctrl_letter_and_alt_meta() {
        let mode = EncodeMode::default();
        let ctrl = Modifiers {
            ctrl: true,
            ..Modifiers::default()
        };
        let c = KeyEvent::character('c', ctrl);
        assert_eq!(encode_key(&c, mode), b"\x03");
        let alt = Modifiers {
            alt: true,
            ..Modifiers::default()
        };
        let x = KeyEvent::character('x', alt);
        assert_eq!(encode_key(&x, mode), b"\x1bx");
        let both = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        };
        let d = KeyEvent::character('d', both);
        assert_eq!(encode_key(&d, mode), b"\x1b\x04");
    }

    #[test]
    fn releases_and_super_are_not_written() {
        let mode = EncodeMode::default();
        let up = key(NamedKey::Enter).released();
        assert!(encode_key(&up, mode).is_empty());
        let mods = Modifiers {
            super_key: true,
            ..Modifiers::default()
        };
        let chord = KeyEvent::character('v', mods);
        assert!(encode_key(&chord, mode).is_empty());
    }

    #[test]
    fn bracketed_paste_wraps_and_drops_esc() {
        assert_eq!(encode_paste("hi", false), b"hi");
        assert_eq!(encode_paste("a\u{1b}b", true), b"\x1b[200~ab\x1b[201~");
    }

    #[test]
    fn sgr_mouse_press_release_and_wheel() {
        let encode = MouseEncode {
            sgr: true,
            protocol: MouseProtocol::Any,
        };
        let mods = Modifiers::default();
        assert_eq!(
            encode_mouse(MouseButton::Left, MouseAction::Press, 0, 0, mods, encode),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            encode_mouse(MouseButton::Left, MouseAction::Release, 2, 3, mods, encode),
            b"\x1b[<0;3;4m"
        );
        assert_eq!(
            encode_mouse(MouseButton::WheelUp, MouseAction::Press, 1, 1, mods, encode),
            b"\x1b[<64;2;2M"
        );
        assert!(encode_mouse(
            MouseButton::Left,
            MouseAction::Press,
            0,
            0,
            mods,
            MouseEncode {
                protocol: MouseProtocol::Off,
                ..encode
            }
        )
        .is_empty());
    }
}
