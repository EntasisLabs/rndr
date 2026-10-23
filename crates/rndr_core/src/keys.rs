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
    /// Legacy edge for the up arrow, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::ArrowUp`], or [`KeyCode::ArrowUp`]
    /// when the logical key is not named.
    Up,
    /// Legacy edge for the down arrow, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::ArrowDown`], or [`KeyCode::ArrowDown`]
    /// when the logical key is not named.
    Down,
    /// Legacy edge for the left arrow, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::ArrowLeft`], or [`KeyCode::ArrowLeft`]
    /// when the logical key is not named.
    Left,
    /// Legacy edge for the right arrow, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::ArrowRight`], or [`KeyCode::ArrowRight`]
    /// when the logical key is not named.
    Right,
    /// Legacy edge for Enter, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::Enter`], or [`KeyCode::Enter`] when
    /// the logical key is not named.
    Enter,
    /// Legacy edge for Escape, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::Escape`], or [`KeyCode::Escape`] when
    /// the logical key is not named.
    Escape,
    /// Legacy edge for Tab, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::Tab`], or [`KeyCode::Tab`] when the
    /// logical key is not named.
    Tab,
    /// Legacy edge for Backspace, not a physical position.
    /// [`legacy_key`] maps [`NamedKey::Backspace`], or [`KeyCode::Backspace`]
    /// when the logical key is not named.
    Backspace,
    /// Legacy edge for primary+S. Not a logical key or a physical position.
    /// [`legacy_key`] matches physical [`KeyCode::Letter`] `s`, or a Latin
    /// logical character, with the platform primary modifier.
    Save,
    /// Legacy edge for primary+N. Not a logical key or a physical position.
    /// [`legacy_key`] matches physical [`KeyCode::Letter`] `n`, or a Latin
    /// logical character, with the platform primary modifier.
    New,
    /// Legacy edge for forward delete, not a letter chord and not a physical
    /// position. [`legacy_key`] maps [`NamedKey::Delete`], or
    /// [`KeyCode::Delete`] when the logical key is not named.
    Delete,
    /// Legacy edge for primary+C. Not a logical key or a physical position.
    /// [`legacy_key`] matches physical [`KeyCode::Letter`] `c`, or a Latin
    /// logical character, with the platform primary modifier.
    Copy,
    /// Legacy edge for primary+X. Not a logical key or a physical position.
    /// [`legacy_key`] matches physical [`KeyCode::Letter`] `x`, or a Latin
    /// logical character, with the platform primary modifier.
    Cut,
    /// Legacy edge for primary+V. Not a logical key or a physical position.
    /// [`legacy_key`] matches physical [`KeyCode::Letter`] `v`, or a Latin
    /// logical character, with the platform primary modifier.
    Paste,
}

/// Host window system. Tests insert this resource to pretend to be another OS.
/// The primary modifier is Super on [`Platform::Mac`] and Ctrl everywhere else.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Platform {
    /// macOS. The primary modifier is Super, not Ctrl.
    Mac,
    /// Windows. The primary modifier is Ctrl.
    Windows,
    /// X11. The primary modifier is Ctrl. [`Platform::host`] reports this on
    /// Linux until the runner stores [`Self::Wayland`].
    X11,
    /// Wayland. The primary modifier is Ctrl. The runner overwrites the host
    /// guess with this when the session is Wayland.
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

    /// True only for [`Platform::Mac`]. Shortcut tables branch on this.
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
    /// Shift was held. Letter matching still folds ASCII case.
    pub shift: bool,
    /// Control was held. Without Alt, text from this chord is not inserted.
    pub ctrl: bool,
    /// Alt was held. Shortcut matching compares this exactly.
    pub alt: bool,
    /// Super, Command, or the Windows key. Text is not inserted while this
    /// is set.
    pub super_key: bool,
}

impl Modifiers {
    /// No modifiers held.
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

    /// Ctrl or Super.
    ///
    /// Super never inserts text. Ctrl inserts only together with Alt, and
    /// only when that text is not the key's own character (AltGr).
    pub fn command(self) -> bool {
        self.ctrl || self.super_key
    }
}

/// Named key, independent of the character it might also produce.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Logical up arrow, not the physical [`KeyCode::ArrowUp`].
    /// [`legacy_key`] maps this to [`Key::Up`].
    ArrowUp,
    /// Logical down arrow, not the physical [`KeyCode::ArrowDown`].
    /// [`legacy_key`] maps this to [`Key::Down`].
    ArrowDown,
    /// Logical left arrow, not the physical [`KeyCode::ArrowLeft`].
    /// [`legacy_key`] maps this to [`Key::Left`].
    ArrowLeft,
    /// Logical right arrow, not the physical [`KeyCode::ArrowRight`].
    /// [`legacy_key`] maps this to [`Key::Right`].
    ArrowRight,
    /// Logical Enter, not the physical [`KeyCode::Enter`].
    /// [`legacy_key`] maps this to [`Key::Enter`].
    Enter,
    /// Logical Escape, not the physical [`KeyCode::Escape`].
    /// [`legacy_key`] maps this to [`Key::Escape`].
    Escape,
    /// Logical Tab, not the physical [`KeyCode::Tab`].
    /// [`legacy_key`] maps this to [`Key::Tab`].
    Tab,
    /// Logical Backspace, not the physical [`KeyCode::Backspace`].
    /// [`legacy_key`] maps this to [`Key::Backspace`].
    Backspace,
    /// Logical forward delete, not the physical [`KeyCode::Delete`].
    /// [`legacy_key`] maps this to [`Key::Delete`].
    Delete,
    /// Logical Insert, not the physical [`KeyCode::Insert`].
    /// [`legacy_key`] has no [`Key`] for it and returns `None`.
    Insert,
    /// Logical Home, not the physical [`KeyCode::Home`].
    /// [`legacy_key`] returns `None`.
    Home,
    /// Logical End, not the physical [`KeyCode::End`].
    /// [`legacy_key`] returns `None`.
    End,
    /// Logical Page Up, not the physical [`KeyCode::PageUp`].
    /// [`legacy_key`] returns `None`.
    PageUp,
    /// Logical Page Down, not the physical [`KeyCode::PageDown`].
    /// [`legacy_key`] returns `None`.
    PageDown,
    /// Logical space bar, not the physical [`KeyCode::Space`] and not the
    /// character. [`legacy_key`] returns `None`.
    Space,
    /// Logical function key. `n` is 1 through 24, the same range as
    /// [`KeyCode::Function`]. [`legacy_key`] returns `None`.
    F(u8),
}

impl NamedKey {
    /// [`Self::F`] when `n` is 1 through 24. `None` outside that range.
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
    /// A [`NamedKey`] with Shift, Ctrl, Alt, and Super ignored.
    Named(NamedKey),
    /// Character the key produces with no Shift/Ctrl/Alt/Super. Layout
    /// dependent, not a physical position. Matching folds ASCII case.
    Character(char),
    /// The host reported no logical key and no character. Matches only
    /// another unidentified key, never a [`KeyCode`].
    Unidentified,
}

/// Physical key position. US-letter names are positions, not the typed character.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A–Z key. Stored lowercase. Physical position, not the typed character.
    /// Matching folds ASCII case.
    Letter(char),
    /// Top-row digit, 0–9. Physical position, not the shifted symbol.
    Digit(u8),
    /// Function key, 1–24. Physical position. Same range as [`NamedKey::F`].
    Function(u8),
    /// Physical up arrow. [`legacy_key`] maps this to [`Key::Up`] when the
    /// logical key is not named.
    ArrowUp,
    /// Physical down arrow. [`legacy_key`] maps this to [`Key::Down`] when
    /// the logical key is not named.
    ArrowDown,
    /// Physical left arrow. [`legacy_key`] maps this to [`Key::Left`] when
    /// the logical key is not named.
    ArrowLeft,
    /// Physical right arrow. [`legacy_key`] maps this to [`Key::Right`] when
    /// the logical key is not named.
    ArrowRight,
    /// Physical Enter. [`legacy_key`] maps this to [`Key::Enter`] when the
    /// logical key is not named.
    Enter,
    /// Physical Escape. [`legacy_key`] maps this to [`Key::Escape`] when the
    /// logical key is not named.
    Escape,
    /// Physical Tab. [`legacy_key`] maps this to [`Key::Tab`] when the
    /// logical key is not named.
    Tab,
    /// Physical Backspace. [`legacy_key`] maps this to [`Key::Backspace`]
    /// when the logical key is not named.
    Backspace,
    /// Physical forward delete. [`legacy_key`] maps this to [`Key::Delete`]
    /// when the logical key is not named.
    Delete,
    /// Physical Insert. [`legacy_key`] returns `None` for this position.
    Insert,
    /// Physical Home. [`legacy_key`] returns `None` for this position.
    Home,
    /// Physical End. [`legacy_key`] returns `None` for this position.
    End,
    /// Physical Page Up. [`legacy_key`] returns `None` for this position.
    PageUp,
    /// Physical Page Down. [`legacy_key`] returns `None` for this position.
    PageDown,
    /// Physical space bar. Not the character, and not [`NamedKey::Space`].
    /// [`legacy_key`] returns `None`.
    Space,
    /// Physical minus key. US position of `-` and `_`, not the character typed.
    Minus,
    /// Physical equal key. US position of `=` and `+`, not the character typed.
    Equal,
    /// Physical left bracket. US position of `[` and `{`.
    BracketLeft,
    /// Physical right bracket. US position of `]` and `}`.
    BracketRight,
    /// Physical backslash. US position of `\` and `|`.
    Backslash,
    /// Physical semicolon. US position of `;` and `:`.
    Semicolon,
    /// Physical quote. US position of `'` and `"`.
    Quote,
    /// Physical comma. US position of `,` and `<`.
    Comma,
    /// Physical period. US position of `.` and `>`.
    Period,
    /// Physical slash. US position of `/` and `?`.
    Slash,
    /// Physical backquote. US position of the grave accent and `~`.
    Backquote,
}

impl KeyCode {
    /// [`Self::Letter`] of the ASCII letter, stored lowercase. `None` when
    /// `ch` is not A–Z or a–z.
    pub fn letter(ch: char) -> Option<Self> {
        ch.is_ascii_alphabetic()
            .then_some(Self::Letter(ch.to_ascii_lowercase()))
    }

    /// [`Self::Digit`] when `n` is 0 through 9. `None` otherwise.
    pub fn digit(n: u8) -> Option<Self> {
        (n < 10).then_some(Self::Digit(n))
    }

    /// [`Self::Function`] when `n` is 1 through 24. `None` otherwise.
    pub fn function(n: u8) -> Option<Self> {
        (1..=24).contains(&n).then_some(Self::Function(n))
    }
}

/// One press, repeat, or release. Several of these can land in a single frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    /// Key with modifiers stripped. Shortcut matching uses this or [`Self::code`].
    pub logical: LogicalKey,
    /// Characters to insert, after layout and Shift. `None` when the key types nothing.
    pub text: Option<String>,
    /// Physical position when the host reported one. `None` does not match a
    /// [`Trigger::Code`] binding.
    pub code: Option<KeyCode>,
    /// Modifiers held for this event.
    pub modifiers: Modifiers,
    /// The host repeated this key while it was held down.
    pub repeat: bool,
    /// The key went up. Releases carry no text.
    pub released: bool,
    /// winit made this event up on a focus change. It is not a physical key.
    ///
    /// X11 and Windows synthesize a key-down for every key held when a window
    /// gains focus, and a key-up when it loses focus. macOS and Wayland leave
    /// this false.
    pub synthetic: bool,
    /// Receive order assigned by the desktop runner.
    ///
    /// [`Self::UNORDERED`] was built in-process and counts as arriving after
    /// any hotkey. A smaller order was queued before the hotkey that armed
    /// the guard.
    pub order: u64,
}

impl KeyEvent {
    /// No receive-order. Counts as arriving after every hotkey.
    pub const UNORDERED: u64 = u64::MAX;

    /// A press of `key` with `modifiers`, no text, and the physical code for
    /// that name. Order is [`Self::UNORDERED`].
    pub fn named(key: NamedKey, modifiers: Modifiers) -> Self {
        Self {
            logical: LogicalKey::Named(key),
            text: None,
            code: key.code(),
            modifiers,
            repeat: false,
            released: false,
            synthetic: false,
            order: Self::UNORDERED,
        }
    }

    /// A press of `ch`. Text is set when `ch` is insertable and neither Ctrl
    /// nor Super is held. An ASCII letter also stores [`KeyCode::Letter`]
    /// in lowercase. Order is [`Self::UNORDERED`].
    pub fn character(ch: char, modifiers: Modifiers) -> Self {
        let code = KeyCode::letter(ch);
        Self {
            logical: LogicalKey::Character(ch),
            text: (!modifiers.command() && is_insertable(ch)).then(|| ch.to_string()),
            code,
            modifiers,
            repeat: false,
            released: false,
            synthetic: false,
            order: Self::UNORDERED,
        }
    }

    /// Set the physical [`KeyCode`].
    pub fn with_code(mut self, code: KeyCode) -> Self {
        self.code = Some(code);
        self
    }

    /// Replace [`Self::text`]. Control characters stay until
    /// [`Self::insertable_text`] filters them.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Mark this event as a key repeat.
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

    /// winit synthesized this event for a focus change.
    pub fn synthetic(mut self) -> Self {
        self.synthetic = true;
        self
    }

    /// Stamp the runner's receive order. [`Self::UNORDERED`] is the default.
    pub fn with_order(mut self, order: u64) -> Self {
        self.order = order;
        self
    }

    /// Text that should be inserted.
    ///
    /// Synthetic focus events never insert. Super never inserts. Ctrl without
    /// Alt never inserts, so an unbound Ctrl or Cmd letter chord does not type
    /// that letter. Alt, and Ctrl+Alt, insert when the text is not the key's
    /// own plain or shifted character: that is how AltGr and Option type.
    /// Control characters are dropped.
    pub fn insertable_text(&self) -> Option<String> {
        if self.synthetic || self.modifiers.super_key {
            return None;
        }
        let text = self.text.as_ref()?;
        let filtered: String = text.chars().filter(|ch| is_insertable(*ch)).collect();
        if filtered.is_empty() {
            return None;
        }
        if self.modifiers.ctrl && !self.modifiers.alt {
            return None;
        }
        if self.modifiers.alt && is_plain_key_text(self, &filtered) {
            return None;
        }
        Some(filtered)
    }
}

/// `text` is the unmodified or shifted character of this key, not AltGr output.
fn is_plain_key_text(event: &KeyEvent, text: &str) -> bool {
    let Some(identity) = key_identity(event) else {
        return false;
    };
    let mut chars = text.chars();
    let Some(got) = chars.next() else {
        return true;
    };
    if chars.next().is_some() {
        return false;
    }
    got.eq_ignore_ascii_case(&identity)
}

fn key_identity(event: &KeyEvent) -> Option<char> {
    if let Some(ch) = event.code.and_then(code_identity) {
        return Some(ch);
    }
    match event.logical {
        LogicalKey::Character(ch) => Some(ch),
        LogicalKey::Named(NamedKey::Space) => Some(' '),
        _ => None,
    }
}

fn code_identity(code: KeyCode) -> Option<char> {
    Some(match code {
        KeyCode::Letter(ch) => ch,
        KeyCode::Digit(n) => char::from_digit(u32::from(n), 10)?,
        KeyCode::Space => ' ',
        KeyCode::Minus => '-',
        KeyCode::Equal => '=',
        KeyCode::BracketLeft => '[',
        KeyCode::BracketRight => ']',
        KeyCode::Backslash => '\\',
        KeyCode::Semicolon => ';',
        KeyCode::Quote => '\'',
        KeyCode::Comma => ',',
        KeyCode::Period => '.',
        KeyCode::Slash => '/',
        KeyCode::Backquote => '`',
        _ => return None,
    })
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
    /// Match [`KeyEvent::logical`]. Characters compare ASCII-case-insensitively.
    /// A named key does not match a character that looks the same.
    Logical(LogicalKey),
    /// Match [`KeyEvent::code`]. Letters compare ASCII-case-insensitively.
    /// A missing code does not match.
    Code(KeyCode),
}

/// One step of a binding. `primary` means Cmd on macOS and Ctrl elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Keystroke {
    /// Logical key or physical code this step requires.
    pub trigger: Trigger,
    /// Shift must be down. Compared exactly.
    pub shift: bool,
    /// Require Ctrl. On macOS this is separate from [`Self::primary`]. On
    /// other hosts primary already means Ctrl, so both flags ask for it.
    pub ctrl: bool,
    /// Alt must be down. Compared exactly.
    pub alt: bool,
    /// Require Super. On macOS [`Self::primary`] already means Super, so both
    /// flags ask for it. Elsewhere this is separate from primary.
    pub super_key: bool,
    /// Cmd on macOS, Ctrl elsewhere. Add [`Self::ctrl`] or [`Self::super_key`]
    /// only when that second modifier is also required.
    pub primary: bool,
}

impl Keystroke {
    /// Stroke matching `key` with no modifiers.
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

    /// [`Self::logical`] of [`LogicalKey::Named`].
    pub const fn named(key: NamedKey) -> Self {
        Self::logical(LogicalKey::Named(key))
    }

    /// [`Self::logical`] of `ch`, forced to ASCII lowercase. No modifiers.
    pub fn character(ch: char) -> Self {
        Self::logical(LogicalKey::Character(ch.to_ascii_lowercase()))
    }

    /// Stroke matching the physical `code` with no modifiers.
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

    /// Require Shift.
    pub const fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Require Ctrl, as well as [`Self::primary`] when that is also set.
    pub const fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Require Alt.
    pub const fn alt(mut self) -> Self {
        self.alt = true;
        self
    }

    /// Require Super, as well as [`Self::primary`] when that is also set.
    pub const fn super_key(mut self) -> Self {
        self.super_key = true;
        self
    }

    /// Require the platform primary modifier: Cmd on macOS, Ctrl elsewhere.
    pub const fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// True when the modifiers and the trigger agree. A release or a
    /// synthetic event can still match; callers filter those.
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
    /// Select the whole field. Primary+A with Shift up, on every platform.
    /// Matches the physical A key or the Latin letter.
    SelectAll,
    /// Undo. Primary+Z with Shift up. On macOS, Shift+Primary+Z is
    /// [`Self::Redo`] instead. Matches the physical Z key or the Latin letter.
    Undo,
    /// Redo. Shift+Primary+Z on every platform. Primary+Y with Shift up also
    /// redoes on Windows, X11, and Wayland. Matches the physical letter or
    /// the Latin letter.
    Redo,
    /// Copy. Primary+C with Shift up. Matches the physical C key or the Latin
    /// letter, so a non-Latin layout still copies.
    Copy,
    /// Cut. Primary+X with Shift up. Matches the physical X key or the Latin
    /// letter.
    Cut,
    /// Paste. Primary+V with Shift up. Matches the physical V key or the Latin
    /// letter.
    Paste,
    /// Move one word left. Alt+Left on macOS, Ctrl+Left elsewhere.
    /// Shift is ignored, so it still extends the selection.
    WordLeft,
    /// Move one word right. Alt+Right on macOS, Ctrl+Right elsewhere.
    /// Shift is ignored, so it still extends the selection.
    WordRight,
    /// Move to the start of the line. Cmd+Left on macOS, Home elsewhere.
    /// Shift is ignored, so it still extends the selection.
    LineStart,
    /// Move to the end of the line. Cmd+Right on macOS, End elsewhere.
    /// Shift is ignored, so it still extends the selection.
    LineEnd,
    /// Move to the start of the document. Cmd+Up on macOS, Ctrl+Home elsewhere.
    /// Shift is ignored, so it still extends the selection.
    DocStart,
    /// Move to the end of the document. Cmd+Down on macOS, Ctrl+End elsewhere.
    /// Shift is ignored, so it still extends the selection.
    DocEnd,
    /// Delete the word before the caret. Alt+Backspace on macOS, Ctrl+Backspace
    /// elsewhere. Shift is ignored.
    DeleteWordBackward,
}

impl EditAction {
    /// Every action [`standard_action`] can return.
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
/// motions and is part of the chord for Undo/Redo. The modifier that is not
/// primary must be up, so Ctrl and Super together do not match.
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
    // Ctrl+Y stays. Ctrl+Shift+Z is the other redo chord on Windows and Linux.
    if !mac {
        pats.push(letter(EditAction::Redo, 'z', true));
    }
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
