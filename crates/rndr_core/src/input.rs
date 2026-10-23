use crate::entity::Entity;
use crate::keys::{legacy_key, KeyEvent, Platform};

pub use crate::keys::Key;

/// Mouse contact id. A later touch plugin writes a different id into the same slot.
pub const MOUSE_ID: u64 = 0;

/// One contact in logical points. The desktop plugin fills this with the mouse.
/// A finger can fill the same slot later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    /// Stable id for this contact. The mouse is [`MOUSE_ID`].
    pub id: u64,
    /// Contact position in logical points, origin top-left. Tracks hover
    /// while [`Self::down`] is false.
    pub position: [f32; 2],
    /// Press edge for this update only.
    pub pressed: bool,
    /// Contact held this frame. Core does not clear it.
    pub down: bool,
}

impl Default for Pointer {
    fn default() -> Self {
        Self {
            id: MOUSE_ID,
            position: [0.0, 0.0],
            pressed: false,
            down: false,
        }
    }
}

/// Pointer and key state for the current update. The window plugin writes this.
#[derive(Clone, Debug, Default)]
pub struct Input {
    /// The single active contact. Position tracks hover when the button is up.
    pub pointer: Pointer,
    /// Content-offset delta for this update only, in points. Positive Y moves content down.
    pub scroll: [f32; 2],
    /// Deprecated: legacy form of the first [`KeyEvent`] this frame.
    ///
    /// A frame can contain more than one press. [`Input::keys`] is the queue.
    /// This field stays for one release so existing callers still compile. It is
    /// the first event mapped onto [`Key`] (arrows, Enter, Escape, Tab,
    /// Backspace, Delete, and the primary chords Save/New/Copy/Cut/Paste), or
    /// `None` when that event has no legacy form.
    pub key: Option<Key>,
    /// Key events in arrival order. The runner clears this after a presented frame.
    pub keys: Vec<KeyEvent>,
    /// Insertable text gathered this frame, control characters excluded.
    /// Widgets that read [`Input::keys`] ignore this when the queue is non-empty.
    pub chars: String,
    /// Uncommitted IME composition. Empty when the platform is not composing.
    pub ime_preedit: String,
    /// Cursor range inside [`Input::ime_preedit`], in bytes. `None` when the
    /// platform did not report one; the caret then sits at the end of the preedit.
    pub ime_cursor: Option<(usize, usize)>,
    /// Text the IME committed this frame.
    pub ime_commit: String,
    /// Shift held this frame. Extends a selection, reverses Tab, and matches
    /// [`crate::Actions`] chords.
    pub shift: bool,
    /// Control held this frame. [`crate::Actions`] matches chords against it.
    pub ctrl: bool,
    /// Alt / Option held this frame. A held Alt blocks a bare Escape dismiss.
    pub alt: bool,
    /// Super / Command / Windows key, held this frame.
    pub super_key: bool,
    /// Ask the shell to exit. The desktop loop stops while this stays true.
    ///
    /// Focus sets it from Escape when the tree cannot collapse further. A
    /// window dismiss of [`crate::Dismiss::Close`] and a tray item that quits
    /// set it too. Nothing in core clears it.
    pub quit: bool,
}

impl Input {
    /// Append one event. The deprecated [`Input::key`] shim is the first press only.
    ///
    /// A press also appends [`crate::KeyEvent::insertable_text`] onto
    /// [`Input::chars`]. A release does not. `platform` is used only to map
    /// that first press onto [`Key`]. Modifier flags are left unchanged.
    pub fn push_key(&mut self, event: KeyEvent, platform: Platform) {
        let first_press = self.keys.iter().all(|event| event.released);
        if !event.released {
            if let Some(text) = event.insertable_text() {
                self.chars.push_str(&text);
            }
        }
        let released = event.released;
        self.keys.push(event);
        if first_press && !released {
            self.key = self
                .keys
                .iter()
                .find(|event| !event.released)
                .and_then(|event| legacy_key(event, platform));
        }
    }
}

/// Currently focused [`crate::Focusable`] entity.
///
/// [`crate::apply_focus`] writes this from clicks and tree keys.
/// [`crate::Shell`] inserts the resource. A stale id is snapped on the next
/// focus pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus {
    /// Focused entity, or `None` when nothing has focus. May be dead until
    /// [`crate::apply_focus`] snaps it.
    pub entity: Option<Entity>,
}

/// Safe-area insets in logical points. Root layout places inside the remainder.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Top inset in logical points. Zero is flush with the window edge.
    pub top: f32,
    /// Right inset in logical points. Zero is flush with the window edge.
    pub right: f32,
    /// Bottom inset in logical points. Zero is flush with the window edge.
    pub bottom: f32,
    /// Left inset in logical points. Zero is flush with the window edge.
    pub left: f32,
}

/// Window size in logical points. Layout, hit-testing, and scroll use this.
/// `scale` is physical pixels per point. The window plugin writes size and scale
/// and leaves [`Insets`] alone.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    /// Window width in logical points.
    pub width: f32,
    /// Window height in logical points.
    pub height: f32,
    /// Physical pixels per point. [`Self::pixels_per_point`] reads a
    /// non-positive or non-finite value as 1.
    pub scale: f32,
    /// Safe area subtracted by [`Self::content_width`] and
    /// [`Self::content_height`].
    pub insets: Insets,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
            scale: 1.0,
            insets: Insets::default(),
        }
    }
}

impl Viewport {
    /// Width and height in logical points. Scale is 1 and insets are zero.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            ..Self::default()
        }
    }

    /// Physical pixels per point. Non-positive or non-finite values read as 1.
    pub fn pixels_per_point(self) -> f32 {
        if self.scale.is_finite() && self.scale > 0.0 {
            self.scale
        } else {
            1.0
        }
    }

    /// Convert `points` to physical pixels with [`Self::pixels_per_point`].
    pub fn to_physical(self, points: f32) -> f32 {
        points * self.pixels_per_point()
    }

    /// Convert `physical` pixels to logical points with [`Self::pixels_per_point`].
    pub fn to_points(self, physical: f32) -> f32 {
        physical / self.pixels_per_point()
    }

    /// Width available to roots after horizontal insets.
    pub fn content_width(self) -> f32 {
        (self.width - self.insets.left - self.insets.right).max(0.0)
    }

    /// Height available to roots after vertical insets.
    pub fn content_height(self) -> f32 {
        (self.height - self.insets.top - self.insets.bottom).max(0.0)
    }
}
