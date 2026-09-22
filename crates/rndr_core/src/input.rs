/// Pointer state for the current update. The render crate writes this.
#[derive(Clone, Copy, Debug, Default)]
pub struct Input {
    pub cursor: [f32; 2],
    /// Press edge for this update only.
    pub pressed: bool,
    /// Content-offset delta for this update only. Positive Y moves content down.
    pub scroll: [f32; 2],
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
