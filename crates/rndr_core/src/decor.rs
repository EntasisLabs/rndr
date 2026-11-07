//! Paint decorations for any laid-out entity: per-corner radii, gradients,
//! box shadows, and per-side or dashed borders.
//!
//! These are read by a paint projection. They do not change layout, hit
//! testing, or the entity's [`crate::Clip`].

use crate::style::Color;
use crate::world::Component;

impl Component for Corners {}
impl Component for Gradient {}
impl Component for Shadows {}
impl Component for BorderSides {}
impl Component for ClipRounded {}

/// Corner radii in logical points: top-left, top-right, bottom-right,
/// bottom-left. Overrides [`crate::Radius`]. Each radius is clamped to half
/// the shorter side when painted.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Corners(
    /// Top-left, top-right, bottom-right, bottom-left.
    pub [f32; 4],
);

impl Corners {
    /// All four corners at `radius`.
    pub const fn all(radius: f32) -> Self {
        Self([radius; 4])
    }

    /// Top corners at `top`, bottom corners at `bottom`.
    pub const fn top_bottom(top: f32, bottom: f32) -> Self {
        Self([top, top, bottom, bottom])
    }
}

/// One color stop. `offset` is the position along the gradient in `0..=1`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient, `0.0..=1.0`. Stops are sorted when painted.
    pub offset: f32,
    /// Straight-alpha sRGB color.
    pub color: Color,
}

impl GradientStop {
    /// Stop at `offset` with `color`.
    pub const fn new(offset: f32, color: Color) -> Self {
        Self { offset, color }
    }
}

/// Gradient painted over the entity's [`Color`] fill, clipped to its corners.
///
/// Colors interpolate in the surface's working space, the same space used for
/// blending: linear light on an sRGB target.
#[derive(Clone, Debug, PartialEq)]
pub enum Gradient {
    /// Along a line through the center. `angle` is in radians clockwise from
    /// pointing down (`0.0` runs top to bottom, like CSS `180deg`); the line
    /// spans the box so the first and last stops reach its corners.
    Linear {
        /// Radians, clockwise, `0.0` = top to bottom.
        angle: f32,
        /// At least one stop.
        stops: Vec<GradientStop>,
    },
    /// Circles around `center`.
    Radial {
        /// Center as a fraction of the box, `[0.5, 0.5]` is the middle.
        center: [f32; 2],
        /// Radius of the last stop as a fraction of half the shorter side.
        radius: f32,
        /// At least one stop.
        stops: Vec<GradientStop>,
    },
}

impl Gradient {
    /// Two-stop gradient from `top` to `bottom`.
    pub fn vertical(top: Color, bottom: Color) -> Self {
        Self::Linear {
            angle: 0.0,
            stops: vec![GradientStop::new(0.0, top), GradientStop::new(1.0, bottom)],
        }
    }
}

/// One box shadow, CSS style.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Shadow {
    /// Offset in logical points, x then y (down).
    pub offset: [f32; 2],
    /// Blur radius in logical points (`2σ`, as CSS `box-shadow`).
    pub blur: f32,
    /// Grows (or with a negative value shrinks) the shadow shape.
    pub spread: f32,
    /// Straight-alpha sRGB color; alpha is the shadow opacity.
    pub color: Color,
    /// Drawn inside the box instead of behind it.
    pub inset: bool,
}

impl Shadow {
    /// Outer shadow `y` points down with `blur` and `color`.
    pub const fn drop(y: f32, blur: f32, color: Color) -> Self {
        Self {
            offset: [0.0, y],
            blur,
            spread: 0.0,
            color,
            inset: false,
        }
    }
}

/// Box shadows in paint order. Outer shadows draw behind the fill; inset
/// shadows draw over the fill and gradient, under the border and children.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shadows(
    /// First entry is painted first.
    pub Vec<Shadow>,
);

/// Inset border with a width per side and an optional dash pattern.
/// Overrides [`crate::Border`]. Follows [`Corners`] or [`crate::Radius`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderSides {
    /// Straight-alpha sRGB color.
    pub color: Color,
    /// Logical points: top, right, bottom, left.
    pub widths: [f32; 4],
    /// Dash and gap length in logical points. `None` is solid.
    pub dash: Option<[f32; 2]>,
}

impl BorderSides {
    /// Same width on every side.
    pub const fn uniform(color: Color, width: f32) -> Self {
        Self {
            color,
            widths: [width; 4],
            dash: None,
        }
    }

    /// Same border, dashed.
    pub const fn dashed(mut self, dash: f32, gap: f32) -> Self {
        self.dash = Some([dash, gap]);
        self
    }
}

/// Masks the entity and its descendants to its rounded rectangle, with an
/// optional soft edge.
///
/// The mask applies to fills, decorations and text. Images and canvas
/// drawings keep the rectangular [`crate::Clip`]. A nested `ClipRounded`
/// replaces the outer one for its own subtree. Hit testing stays rectangular.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClipRounded {
    /// Top-left, top-right, bottom-right, bottom-left radii in logical points.
    pub corners: [f32; 4],
    /// Width of the soft edge in logical points, measured inward from the
    /// rectangle. `0.0` is a hard, anti-aliased edge.
    pub feather: f32,
}

impl ClipRounded {
    /// Same radius on every corner, hard edge.
    pub const fn radius(radius: f32) -> Self {
        Self {
            corners: [radius; 4],
            feather: 0.0,
        }
    }

    /// Same mask with a soft edge `feather` points wide.
    pub const fn feathered(mut self, feather: f32) -> Self {
        self.feather = feather;
        self
    }
}
