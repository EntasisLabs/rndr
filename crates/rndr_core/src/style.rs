use crate::world::Component;

impl Component for Color {}
impl Component for Label {}
impl Component for Body {}

/// Solid fill drawn by the render plugin.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }
}

/// ASCII label drawn from the embedded bitmap font.
#[derive(Clone, Debug)]
pub struct Label {
    pub text: String,
}

impl Label {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Document text on a tree node, shown in the pane body field.
#[derive(Clone, Debug, Default)]
pub struct Body {
    pub text: String,
}

impl Body {
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}
