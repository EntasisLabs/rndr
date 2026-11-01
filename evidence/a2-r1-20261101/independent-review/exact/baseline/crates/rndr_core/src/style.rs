use crate::world::Component;

impl Component for Color {}
impl Component for Label {}
impl Component for Body {}

/// sRGB color with straight alpha, drawn as a solid fill.
///
/// `r`, `g`, and `b` are IEC 61966-2-1 channel values in `0.0..=1.0`: the
/// encoding of CSS `rgb()` and of an 8-bit framebuffer. `a` is linear coverage
/// in `0.0..=1.0` and is not premultiplied.
///
/// Paint copies these channels onto quads. The render plugin decodes `r`, `g`,
/// and `b` to linear light before upload when the surface format is sRGB,
/// because that format encodes on store and blends in linear light. A non-sRGB
/// surface stores the channels as given. On either format the pixels match
/// these values. Alpha is left alone on both paths.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// IEC 61966-2-1 red channel, in `0.0..=1.0`.
    pub r: f32,
    /// IEC 61966-2-1 green channel, in `0.0..=1.0`.
    pub g: f32,
    /// IEC 61966-2-1 blue channel, in `0.0..=1.0`.
    pub b: f32,
    /// Linear coverage in `0.0..=1.0`. Straight alpha, kept as given.
    pub a: f32,
}

impl Color {
    /// Channel values stored as given.
    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    /// [`rgba`](Self::rgba) with alpha `1.0`.
    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }
}

/// How a [`Label`] fits the width it was given.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextOverflow {
    /// One line, drawn at its natural width.
    #[default]
    Visible,
    /// Break onto more lines inside the label's width.
    Wrap,
    /// One line, cut with `…` so the text stays inside the label's width.
    Ellipsis,
}

/// ASCII label drawn from the embedded bitmap font.
///
/// `color` overrides [`crate::Theme::text`] for this label. Styled runs on
/// [`crate::Spans`] still win for the bytes they cover. [`TextOverflow`]
/// chooses whether a long line stays whole, wraps, or ends in an ellipsis.
#[derive(Clone, Debug)]
pub struct Label {
    /// Characters drawn from the embedded bitmap font.
    pub text: String,
    /// Overrides [`crate::Theme::text`] for this label when set.
    pub color: Option<Color>,
    /// How a long line fits the width the label was given.
    pub overflow: TextOverflow,
}

impl Label {
    /// Visible overflow and no color override.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
            overflow: TextOverflow::Visible,
        }
    }

    /// Set [`Label::color`].
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Set [`Label::overflow`].
    pub fn overflow(mut self, overflow: TextOverflow) -> Self {
        self.overflow = overflow;
        self
    }
}

/// Document text on a tree node, shown in the pane body field.
#[derive(Clone, Debug, Default)]
pub struct Body {
    /// Document text the pane copies into the body field.
    pub text: String,
}

impl Body {
    /// Store `text` as the document.
    pub fn new(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// Glyph cell in logical points. The render plugin draws the 8×8 atlas at this size.
pub const CELL_W: f32 = 16.0;
/// Cell height in logical points. The atlas cell is a square [`CELL_W`] wide.
pub const CELL_H: f32 = 16.0;
/// Left inset of the first glyph inside a row rectangle.
pub const TEXT_ORIGIN_X: f32 = 12.0;
/// Top inset of the first line inside a row rectangle.
pub const TEXT_ORIGIN_Y: f32 = 8.0;
