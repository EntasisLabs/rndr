use crate::entity::Entity;

/// One-frame key edge. The render crate writes this from winit.
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
}

/// Pointer and key state for the current update. The render crate writes this.
#[derive(Clone, Debug, Default)]
pub struct Input {
    pub cursor: [f32; 2],
    /// Press edge for this update only.
    pub pressed: bool,
    /// Content-offset delta for this update only. Positive Y moves content down.
    pub scroll: [f32; 2],
    pub key: Option<Key>,
    /// Printable ASCII gathered this frame.
    pub chars: String,
    pub shift: bool,
    pub quit: bool,
}

/// Currently focused [`Focusable`] entity.
#[derive(Clone, Copy, Debug, Default)]
pub struct Focus {
    pub entity: Option<Entity>,
}

/// Physical pixel size of the window. Fill roots use this.
#[derive(Clone, Copy, Debug)]
pub struct Viewport {
    pub width: f32,
    pub height: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width: 800.0,
            height: 600.0,
        }
    }
}
