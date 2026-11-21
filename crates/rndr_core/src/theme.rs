use crate::style::Color;
use crate::world::{Component, World};

impl Component for Radius {}

/// Corner radius in logical points. Zero paints a square.
#[derive(Clone, Copy, Debug, Default)]
pub struct Radius(
    /// Logical points.
    pub f32,
);

/// Chrome colors and the corner radius for surfaces.
///
/// Every [`Color`] is sRGB. See [`Color`] for how the renderer uploads it.
/// Absent from the world means [`Theme::default`].
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    /// Window background. Assign it as a [`Color`] on the root.
    pub canvas: Color,
    /// Tree column fill. Assign it as a [`Color`] on that column.
    pub tree: Color,
    /// Splitter fill. Assign it as a [`Color`] on the handle.
    pub seam: Color,
    /// Detail pane fill. Assign it as a [`Color`] on the pane.
    pub pane: Color,
    /// Title row that [`crate::apply_pane`] spawns.
    pub title: Color,
    /// Document row that [`crate::apply_pane`] spawns.
    pub body: Color,
    /// Open, closed, or leaf row that [`crate::apply_pane`] spawns.
    pub status: Color,
    /// Child-name row that [`crate::apply_pane`] spawns under a disclosure.
    pub child: Color,
    /// Row [`crate::apply_pane`] spawns when nothing is selected.
    pub empty: Color,
    /// Glyph color for a [`crate::Label`] that sets no color of its own.
    pub text: Color,
    /// Caret quad.
    pub caret: Color,
    /// Selection highlight and find decoration. The default is a bright blue
    /// so the range stays readable on the dark chrome.
    pub selection: Color,
    /// Mark painted on a node with unsaved edits.
    pub dirty: Color,
    /// Corner radius in logical points. Copy it onto [`Radius`].
    pub radius: f32,
}

/// [`Theme`] stored on `world`, or [`Theme::default`] when it is absent.
pub fn theme_of(world: &World) -> Theme {
    world.resource::<Theme>().copied().unwrap_or_default()
}
