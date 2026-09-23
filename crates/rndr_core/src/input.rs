use crate::entity::Entity;

/// One-frame key edge. The render crate writes this from the platform.
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
}

/// Mouse contact id. A later touch plugin writes a different id into the same slot.
pub const MOUSE_ID: u64 = 0;

/// One contact in logical points. The desktop plugin fills this with the mouse.
/// A finger can fill the same slot later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pointer {
    /// Stable id for this contact. The mouse is [`MOUSE_ID`].
    pub id: u64,
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
    pub key: Option<Key>,
    /// Printable ASCII gathered this frame.
    pub chars: String,
    pub shift: bool,
    pub ctrl: bool,
    pub quit: bool,
}

/// Currently focused [`Focusable`] entity.
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus {
    pub entity: Option<Entity>,
}

/// Safe-area insets in logical points. Root layout places inside the remainder.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Window size in logical points. Layout, hit-testing, and scroll use this.
/// `scale` is physical pixels per point. The window plugin writes size and scale
/// and leaves [`Insets`] alone.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
    pub scale: f32,
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

    pub fn to_physical(self, points: f32) -> f32 {
        points * self.pixels_per_point()
    }

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
