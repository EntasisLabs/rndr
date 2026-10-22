use crate::style::Color;
use crate::world::{Component, World};

impl Component for Radius {}

/// Corner radius in logical points. Zero paints a square.
#[derive(Clone, Copy, Debug, Default)]
pub struct Radius(pub f32);

/// Chrome colors and the corner radius for surfaces.
///
/// Every [`Color`] is sRGB. See [`Color`] for how the renderer uploads it.
/// Absent from the world means [`Theme::default`].
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub canvas: Color,
    pub tree: Color,
    pub seam: Color,
    pub pane: Color,
    pub title: Color,
    pub body: Color,
    pub status: Color,
    pub child: Color,
    pub empty: Color,
    pub text: Color,
    pub caret: Color,
    pub selection: Color,
    pub dirty: Color,
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
            selection: Color::rgba(0.35, 0.55, 0.85, 0.55),
            dirty: Color::rgb(0.86, 0.62, 0.20),
            radius: 8.0,
        }
    }
}

pub fn theme_of(world: &World) -> Theme {
    world.resource::<Theme>().copied().unwrap_or_default()
}
