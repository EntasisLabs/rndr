//! Logical keys, physical codes, and the platform shortcut table.
//!
//! A keystroke is three facts, not one enum variant:
//! - [`LogicalKey`] is the key with Shift/Ctrl/Alt/Super ignored, for shortcut matching.
//! - [`KeyEvent::text`] is what the layout produced, for insertion.
//! - [`KeyCode`] is the physical position, so Cmd+C still copies on a Cyrillic layout.

use crate::world::World;

/// Legacy one-key edge. The runner still writes the first event here.
/// Prefer [`KeyEvent`] on [`crate::Input::keys`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Tab,
    Backspace,
    Save,
    New,
    Delete,
    Copy,
    Cut,
    Paste,
}

/// Host window system. Tests insert this resource to pretend to be another OS.
/// The primary modifier is Super on [`Platform::Mac`] and Ctrl everywhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    Mac,
    Windows,
    X11,
    Wayland,
}

impl Platform {
    /// Compile-time guess. Linux defaults to X11; the runner overwrites Wayland.
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::X11
        }
    }

    pub fn primary_is_super(self) -> bool {
        matches!(self, Self::Mac)
    }
}

/// [`Platform`] resource, or [`Platform::host`] when the app has not set one.
pub fn platform_of(world: &World) -> Platform {
    world
        .resource::<Platform>()
        .copied()
        .unwrap_or_else(Platform::host)
}

/// Shift, Ctrl, Alt, and Super as held at the moment of a key event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    /// Super, Command, or the Windows key.
    pub super_key: bool,
}

impl Modifiers {
    pub const fn new() -> Self {
        Self {
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
        }
    }

    /// Cmd on macOS, Ctrl on Windows, X11, and Wayland.
    pub fn primary(self, platform: Platform) -> bool {
        if platform.primary_is_super() {
            self.super_key
        } else {
            self.ctrl
        }
    }

    /// Ctrl or Super. Text is not inserted while either is held.
    pub fn command(self) -> bool {
        self.ctrl || self.super_key
    }
}

/// Named key, independent of the character it might also produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    /// Function key. `n` is 1 through 24.
    F(u8),
}

impl NamedKey {
    pub fn function(n: u8) -> Option<Self> {
        (1..=24).contains(&n).then_some(Self::F(n))
    }

    pub(crate) fn code(self) -> Option<KeyCode> {
        Some(match self {
            Self::ArrowUp => KeyCode::ArrowUp,
            Self::ArrowDown => KeyCode::ArrowDown,
            Self::ArrowLeft => KeyCode::ArrowLeft,
            Self::ArrowRight => KeyCode::ArrowRight,
            Self::Enter => KeyCode::Enter,
            Self::Escape => KeyCode::Escape,
            Self::Tab => KeyCode::Tab,
            Self::Backspace => KeyCode::Backspace,
            Self::Delete => KeyCode::Delete,
            Self::Insert => KeyCode::Insert,
            Self::Home => KeyCode::Home,
            Self::End => KeyCode::End,
            Self::PageUp => KeyCode::PageUp,
            Self::PageDown => KeyCode::PageDown,
            Self::Space => KeyCode::Space,
            Self::F(n) => KeyCode::Function(n),
        })
    }
}

/// Key identity with modifiers removed. Shortcut matching uses this or [`KeyCode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LogicalKey {
    Named(NamedKey),
    /// Character the key produces with no Shift/Ctrl/Alt/Super. Layout dependent.
    Character(char),
    Unidentified,
}

/// Physical key position. US-letter names are positions, not the typed character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A–Z key. Stored lowercase.
    Letter(char),
    /// Top-row digit, 0–9.
    Digit(u8),
    /// Function key, 1–24.
    Function(u8),
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Enter,
    Escape,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Minus,
    Equal,
    BracketLeft,
    BracketRight,
    Backslash,
    Semicolon,
    Quote,
    Comma,
    Period,
    Slash,
    Backquote,
}

impl KeyCode {
    pub fn letter(ch: char) -> Option<Self> {
        ch.is_ascii_alphabetic()
            .then_some(Self::Letter(ch.to_ascii_lowercase()))
    }

    pub fn digit(n: u8) -> Option<Self> {
        (n < 10).then_some(Self::Digit(n))
    }

    pub fn function(n: u8) -> Option<Self> {
        (1..=24).contains(&n).then_some(Self::Function(n))
    }
}

/// One press, repeat, or release. Several of these can land in a single frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub logical: LogicalKey,
    /// Characters to insert, after layout and Shift. `None` when the key types nothing.
    pub text: Option<String>,
    pub code: Option<KeyCode>,
    pub modifiers: Modifiers,
    pub repeat: bool,
    /// The key went up. Releases carry no text.
    pub released: bool,
}

impl KeyEvent {
    pub fn named(key: NamedKey, modifiers: Modifiers) -> Self {
        Self {
            logical: LogicalKey::Named(key),
            text: None,
            code: key.code(),
            modifiers,
            repeat: false,
            released: false,
        }
    }

    pub fn character(ch: char, modifiers: Modifiers) -> Self {
        let code = KeyCode::letter(ch);
        Self {
            logical: LogicalKey::Character(ch),
            text: (!modifiers.command() && is_insertable(ch)).then(|| ch.to_string()),
            code,
            modifiers,
            repeat: false,
            released: false,
        }
    }

    pub fn with_code(mut self, code: KeyCode) -> Self {
        self.code = Some(code);
        self
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    pub fn repeating(mut self) -> Self {
        self.repeat = true;
        self
    }

    /// Mark this event as a key-up and drop any text it was carrying.
    pub fn released(mut self) -> Self {
        self.released = true;
        self.text = None;
        self
    }

    /// Text that should be inserted.
    ///
    /// Super never inserts. Ctrl and Alt insert only when this event already
    /// carries text, which is how AltGr types. Control characters are dropped.
    pub fn insertable_text(&self) -> Option<String> {
        if self.modifiers.super_key {
            return None;
        }
        let text = self.text.as_ref()?;
        let filtered: String = text.chars().filter(|ch| is_insertable(*ch)).collect();
        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
    }
}

/// Printable text, including non-ASCII. Control characters stay out of the buffer.
pub fn is_insertable(ch: char) -> bool {
    !ch.is_control()
}

/// Legacy [`Key`] for the first event of a frame, when it has one.
///
/// Primary+S/N/C/X/V become Save/New/Copy/Cut/Paste so the file tree and the
/// clipboard chords keep working. Anything else maps only when it is one of
/// the original navigation keys.
pub fn legacy_key(event: &KeyEvent, platform: Platform) -> Option<Key> {
    if event.modifiers.primary(platform) {
        if let Some(letter) = event_letter(event) {
            return match letter {
                's' => Some(Key::Save),
                'n' => Some(Key::New),
                'c' => Some(Key::Copy),
                'x' => Some(Key::Cut),
                'v' => Some(Key::Paste),
                _ => named_legacy(event),
            };
        }
    }
    named_legacy(event)
}

fn event_letter(event: &KeyEvent) -> Option<char> {
    if let Some(KeyCode::Letter(ch)) = event.code {
        return Some(ch.to_ascii_lowercase());
    }
    if let LogicalKey::Character(ch) = event.logical {
        if ch.is_ascii_alphabetic() {
            return Some(ch.to_ascii_lowercase());
        }
    }
    None
}

fn named_legacy(event: &KeyEvent) -> Option<Key> {
    let named = match event.logical {
        LogicalKey::Named(named) => Some(named),
        _ => event.code.and_then(named_of_code),
    }?;
    Some(match named {
        NamedKey::ArrowUp => Key::Up,
        NamedKey::ArrowDown => Key::Down,
        NamedKey::ArrowLeft => Key::Left,
        NamedKey::ArrowRight => Key::Right,
        NamedKey::Enter => Key::Enter,
        NamedKey::Escape => Key::Escape,
        NamedKey::Tab => Key::Tab,
        NamedKey::Backspace => Key::Backspace,
        NamedKey::Delete => Key::Delete,
        NamedKey::Insert
        | NamedKey::Home
        | NamedKey::End
        | NamedKey::PageUp
        | NamedKey::PageDown
        | NamedKey::Space
        | NamedKey::F(_) => return None,
    })
}

fn named_of_code(code: KeyCode) -> Option<NamedKey> {
    Some(match code {
        KeyCode::ArrowUp => NamedKey::ArrowUp,
        KeyCode::ArrowDown => NamedKey::ArrowDown,
        KeyCode::ArrowLeft => NamedKey::ArrowLeft,
        KeyCode::ArrowRight => NamedKey::ArrowRight,
        KeyCode::Enter => NamedKey::Enter,
        KeyCode::Escape => NamedKey::Escape,
        KeyCode::Tab => NamedKey::Tab,
        KeyCode::Backspace => NamedKey::Backspace,
        KeyCode::Delete => NamedKey::Delete,
        KeyCode::Insert => NamedKey::Insert,
        KeyCode::Home => NamedKey::Home,
        KeyCode::End => NamedKey::End,
        KeyCode::PageUp => NamedKey::PageUp,
        KeyCode::PageDown => NamedKey::PageDown,
        KeyCode::Space => NamedKey::Space,
        KeyCode::Function(n) => NamedKey::F(n),
        _ => return None,
    })
}

/// What a shortcut binds to: the unmodified logical key, or the physical code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Trigger {
    Logical(LogicalKey),
    Code(KeyCode),
}

/// One step of a binding. `primary` means Cmd on macOS and Ctrl elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keystroke {
    pub trigger: Trigger,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub super_key: bool,
    pub primary: bool,
}

impl Keystroke {
    pub const fn logical(key: LogicalKey) -> Self {
        Self {
            trigger: Trigger::Logical(key),
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
            primary: false,
        }
    }

    pub const fn named(key: NamedKey) -> Self {
        Self::logical(LogicalKey::Named(key))
    }

    pub fn character(ch: char) -> Self {
        Self::logical(LogicalKey::Character(ch.to_ascii_lowercase()))
    }

    pub const fn code(code: KeyCode) -> Self {
        Self {
            trigger: Trigger::Code(code),
            shift: false,
            ctrl: false,
            alt: false,
            super_key: false,
            primary: false,
        }
    }

    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    pub const fn super_key(mut self) -> Self {
        self.super_key = true;
        self
    }

    pub const fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    pub fn matches(&self, event: &KeyEvent, platform: Platform) -> bool {
        mods_match(self, &event.modifiers, platform) && trigger_match(self.trigger, event)
    }

    /// Short label for a command list. Primary renders as Cmd or Ctrl.
    pub fn label(&self, platform: Platform) -> String {
        let mut parts = Vec::new();
        if self.primary {
            parts.push(
                if platform.primary_is_super() {
                    "Cmd"
                } else {
                    "Ctrl"
                }
                .to_string(),
            );
        }
        if self.ctrl && !(self.primary && !platform.primary_is_super()) {
            parts.push("Ctrl".to_string());
        }
        if self.super_key && !(self.primary && platform.primary_is_super()) {
            parts.push("Cmd".to_string());
        }
        if self.alt {
            parts.push("Alt".to_string());
        }
        if self.shift {
            parts.push("Shift".to_string());
        }
        parts.push(trigger_label(self.trigger));
        parts.join("+")
    }

    /// Platform shortcut text. macOS uses symbols (`⌘K`, `⇧⌘K`).
    /// Windows, X11, and Wayland use [`Keystroke::label`] (`Ctrl+K`).
    pub fn display(&self, platform: Platform) -> String {
        if platform.primary_is_super() {
            mac_display(self)
        } else {
            self.label(platform)
        }
    }
}

/// Control, Option, Shift, Command, then the key. Matches the macOS order,
/// so a lone command chord is `⌘K` and command-shift is `⇧⌘K`.
fn mac_display(stroke: &Keystroke) -> String {
    let mut out = String::new();
    if stroke.ctrl {
        out.push('\u{2303}');
    }
    if stroke.alt {
        out.push('\u{2325}');
    }
    if stroke.shift {
        out.push('\u{21E7}');
    }
    if stroke.primary || stroke.super_key {
        out.push('\u{2318}');
    }
    out.push_str(&trigger_label(stroke.trigger));
    out
}

fn trigger_label(trigger: Trigger) -> String {
    match trigger {
        Trigger::Logical(LogicalKey::Named(NamedKey::F(n))) => format!("F{n}"),
        Trigger::Logical(LogicalKey::Named(named)) => named_label(named).to_string(),
        Trigger::Logical(LogicalKey::Character(ch)) => ch.to_ascii_uppercase().to_string(),
        Trigger::Logical(LogicalKey::Unidentified) => "Unknown".to_string(),
        Trigger::Code(KeyCode::Letter(ch)) => ch.to_ascii_uppercase().to_string(),
        Trigger::Code(KeyCode::Digit(n)) => n.to_string(),
        Trigger::Code(KeyCode::Function(n)) => format!("F{n}"),
        Trigger::Code(code) => code_label(code).to_string(),
    }
}

fn named_label(named: NamedKey) -> &'static str {
    match named {
        NamedKey::ArrowUp => "Up",
        NamedKey::ArrowDown => "Down",
        NamedKey::ArrowLeft => "Left",
        NamedKey::ArrowRight => "Right",
        NamedKey::Enter => "Enter",
        NamedKey::Escape => "Escape",
        NamedKey::Tab => "Tab",
        NamedKey::Backspace => "Backspace",
        NamedKey::Delete => "Delete",
        NamedKey::Insert => "Insert",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        NamedKey::Space => "Space",
        NamedKey::F(_) => "F",
    }
}

fn code_label(code: KeyCode) -> &'static str {
    match code {
        KeyCode::Letter(_) | KeyCode::Digit(_) | KeyCode::Function(_) => "",
        KeyCode::ArrowUp => "Up",
        KeyCode::ArrowDown => "Down",
        KeyCode::ArrowLeft => "Left",
        KeyCode::ArrowRight => "Right",
        KeyCode::Enter => "Enter",
        KeyCode::Escape => "Escape",
        KeyCode::Tab => "Tab",
        KeyCode::Backspace => "Backspace",
        KeyCode::Delete => "Delete",
        KeyCode::Insert => "Insert",
        KeyCode::Home => "Home",
        KeyCode::End => "End",
        KeyCode::PageUp => "PageUp",
        KeyCode::PageDown => "PageDown",
        KeyCode::Space => "Space",
        KeyCode::Minus => "-",
        KeyCode::Equal => "=",
        KeyCode::BracketLeft => "[",
        KeyCode::BracketRight => "]",
        KeyCode::Backslash => "\\",
        KeyCode::Semicolon => ";",
        KeyCode::Quote => "'",
        KeyCode::Comma => ",",
        KeyCode::Period => ".",
        KeyCode::Slash => "/",
        KeyCode::Backquote => "`",
    }
}

fn mods_match(stroke: &Keystroke, mods: &Modifiers, platform: Platform) -> bool {
    if stroke.shift != mods.shift || stroke.alt != mods.alt {
        return false;
    }
    let want_ctrl = stroke.ctrl || (stroke.primary && !platform.primary_is_super());
    let want_super = stroke.super_key || (stroke.primary && platform.primary_is_super());
    mods.ctrl == want_ctrl && mods.super_key == want_super
}

fn trigger_match(trigger: Trigger, event: &KeyEvent) -> bool {
    match trigger {
        Trigger::Logical(want) => logical_eq(want, event.logical),
        Trigger::Code(want) => event.code.is_some_and(|got| code_eq(want, got)),
    }
}

fn logical_eq(want: LogicalKey, got: LogicalKey) -> bool {
    match (want, got) {
        (LogicalKey::Character(a), LogicalKey::Character(b)) => a.eq_ignore_ascii_case(&b),
        (LogicalKey::Named(a), LogicalKey::Named(b)) => a == b,
        (LogicalKey::Unidentified, LogicalKey::Unidentified) => true,
        _ => false,
    }
}

fn code_eq(want: KeyCode, got: KeyCode) -> bool {
    match (want, got) {
        (KeyCode::Letter(a), KeyCode::Letter(b)) => a.eq_ignore_ascii_case(&b),
        _ => want == got,
    }
}

/// Editing commands whose chord depends on the platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditAction {
    SelectAll,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    WordLeft,
    WordRight,
    LineStart,
    LineEnd,
    DocStart,
    DocEnd,
    DeleteWordBackward,
}

impl EditAction {
    pub const ALL: [EditAction; 13] = [
        EditAction::SelectAll,
        EditAction::Undo,
        EditAction::Redo,
        EditAction::Copy,
        EditAction::Cut,
        EditAction::Paste,
        EditAction::WordLeft,
        EditAction::WordRight,
        EditAction::LineStart,
        EditAction::LineEnd,
        EditAction::DocStart,
        EditAction::DocEnd,
        EditAction::DeleteWordBackward,
    ];
}

#[derive(Clone, Copy)]
enum ShiftMatch {
    /// Letter chords: Redo requires Shift, Undo rejects it.
    Exact(bool),
    /// Motions: Shift extends the selection and still matches.
    Ignore,
}

struct StdPat {
    action: EditAction,
    key: StdKey,
    alt: bool,
    primary: bool,
    shift: ShiftMatch,
}

enum StdKey {
    Letter(char),
    Named(NamedKey),
}

impl StdPat {
    fn matches(&self, event: &KeyEvent, platform: Platform) -> bool {
        if self.alt != event.modifiers.alt {
            return false;
        }
        if self.primary != event.modifiers.primary(platform) {
            return false;
        }
        // The modifier that is not primary must be up, so Ctrl+Cmd does not fire.
        let other = if platform.primary_is_super() {
            event.modifiers.ctrl
        } else {
            event.modifiers.super_key
        };
        if other {
            return false;
        }
        match self.shift {
            ShiftMatch::Exact(want) => {
                if event.modifiers.shift != want {
                    return false;
                }
            }
            ShiftMatch::Ignore => {}
        }
        match self.key {
            StdKey::Letter(ch) => is_letter(event, ch),
            StdKey::Named(named) => is_named(event, named),
        }
    }
}

fn is_letter(event: &KeyEvent, want: char) -> bool {
    let want = want.to_ascii_lowercase();
    if event
        .code
        .is_some_and(|code| matches!(code, KeyCode::Letter(ch) if ch.to_ascii_lowercase() == want))
    {
        return true;
    }
    matches!(event.logical, LogicalKey::Character(ch) if ch.eq_ignore_ascii_case(&want))
}

fn is_named(event: &KeyEvent, named: NamedKey) -> bool {
    event.logical == LogicalKey::Named(named) || event.code == named.code()
}

/// Resolve one standard editing chord for `platform`.
///
/// Letter chords match the physical key or the unmodified Latin letter, so
/// Cmd+C copies when the logical character is Cyrillic. Shift selects for
/// motions and is part of the chord for Undo/Redo.
pub fn standard_action(platform: Platform, event: &KeyEvent) -> Option<EditAction> {
    patterns(platform)
        .into_iter()
        .find(|pat| pat.matches(event, platform))
        .map(|pat| pat.action)
}

fn letter(action: EditAction, ch: char, shift: bool) -> StdPat {
    StdPat {
        action,
        key: StdKey::Letter(ch),
        alt: false,
        primary: true,
        shift: ShiftMatch::Exact(shift),
    }
}

fn motion(action: EditAction, key: StdKey, alt: bool, primary: bool) -> StdPat {
    StdPat {
        action,
        key,
        alt,
        primary,
        shift: ShiftMatch::Ignore,
    }
}

fn patterns(platform: Platform) -> Vec<StdPat> {
    let mac = platform.primary_is_super();
    let mut pats = vec![
        letter(EditAction::SelectAll, 'a', false),
        letter(EditAction::Redo, if mac { 'z' } else { 'y' }, mac),
        letter(EditAction::Undo, 'z', false),
        letter(EditAction::Copy, 'c', false),
        letter(EditAction::Cut, 'x', false),
        letter(EditAction::Paste, 'v', false),
    ];
    if mac {
        pats.push(motion(
            EditAction::WordLeft,
            StdKey::Named(NamedKey::ArrowLeft),
            true,
            false,
        ));
        pats.push(motion(
            EditAction::WordRight,
            StdKey::Named(NamedKey::ArrowRight),
            true,
            false,
        ));
        pats.push(motion(
            EditAction::LineStart,
            StdKey::Named(NamedKey::ArrowLeft),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::LineEnd,
            StdKey::Named(NamedKey::ArrowRight),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::DocStart,
            StdKey::Named(NamedKey::ArrowUp),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::DocEnd,
            StdKey::Named(NamedKey::ArrowDown),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::DeleteWordBackward,
            StdKey::Named(NamedKey::Backspace),
            true,
            false,
        ));
    } else {
        pats.push(motion(
            EditAction::WordLeft,
            StdKey::Named(NamedKey::ArrowLeft),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::WordRight,
            StdKey::Named(NamedKey::ArrowRight),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::LineStart,
            StdKey::Named(NamedKey::Home),
            false,
            false,
        ));
        pats.push(motion(
            EditAction::LineEnd,
            StdKey::Named(NamedKey::End),
            false,
            false,
        ));
        pats.push(motion(
            EditAction::DocStart,
            StdKey::Named(NamedKey::Home),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::DocEnd,
            StdKey::Named(NamedKey::End),
            false,
            true,
        ));
        pats.push(motion(
            EditAction::DeleteWordBackward,
            StdKey::Named(NamedKey::Backspace),
            false,
            true,
        ));
    }
    pats
}
