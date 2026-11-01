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

impl Default for Theme {
    fn default() -> Self {
        Self {
            canvas: Color::rgb(0.08, 0.09, 0.10),
            tree: Color::rgb(0.10, 0.11, 0.13),
            seam: Color::rgb(0.06, 0.07, 0.08),
            pane: Color::rgb(0.12, 0.13, 0.15),
            title: Color::rgb(0.22, 0.34, 0.48),
            body: Color::rgb(0.14, 0.16, 0.20),
            status: Color::rgb(0.18, 0.22, 0.26),
            child: Color::rgb(0.20, 0.28, 0.30),
            empty: Color::rgb(0.16, 0.17, 0.19),
            text: Color::rgb(0.94, 0.95, 0.97),
            caret: Color::rgb(0.94, 0.95, 0.97),
            selection: Color::rgba(0.36, 0.62, 0.98, 0.88),
            dirty: Color::rgb(0.86, 0.62, 0.20),
            radius: 8.0,
        }
    }
}

/// [`Theme`] stored on `world`, or [`Theme::default`] when it is absent.
pub fn theme_of(world: &World) -> Theme {
    world.resource::<Theme>().copied().unwrap_or_default()
}
