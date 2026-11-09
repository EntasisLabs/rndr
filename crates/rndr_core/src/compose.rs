//! Compositing properties for any entity: group opacity, a 2D transform and
//! blur.
//!
//! Both act after layout. Siblings keep their rectangles, and the entity's
//! own [`Rect`] and [`crate::Clip`] stay in untransformed layout points.
//! A paint projection draws the entity and its descendants as one group and
//! composites the group once, so overlapping children at half opacity read
//! as one half-transparent picture rather than stacking.

use crate::layout::Rect;
use crate::world::Component;

impl Component for Opacity {}
impl Component for Transform {}
impl Component for Blur {}
impl Component for BackdropBlur {}

/// Gaussian blur of an entity and its descendants after they are drawn, like
/// CSS `filter: blur()`.
///
/// The value is the standard deviation in logical points. Zero, negative and
/// non-finite values draw unblurred and cost nothing. The blurred group
/// spreads about three sigma past its content and is cut by the parent's
/// [`crate::Clip`]. Hit testing is unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Blur(
    /// Gaussian sigma in logical points.
    pub f32,
);

impl Blur {
    /// Sigma to paint with: `0.0` for a zero, negative or non-finite value.
    pub fn sigma(self) -> f32 {
        finite_sigma(self.0)
    }
}

/// Blurs what is already drawn behind an entity, like CSS
/// `backdrop-filter: blur()`: a frosted panel.
///
/// The blur fills the entity's rect inside its largest corner radius and its
/// parent's [`crate::Clip`]. The entity's own fill then draws over it as a
/// tint, with its border and children on top, so a translucent fill such as
/// white at 40% reads as frosted glass. The value is the standard deviation
/// in logical points; zero, negative and non-finite values turn it off.
///
/// Inside a group drawn as one layer ([`Opacity`] below one, a [`Transform`]
/// or [`Blur`]), the backdrop is what that group has drawn so far. A target
/// the renderer cannot copy draws only the fill.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackdropBlur(
    /// Gaussian sigma in logical points.
    pub f32,
);

impl BackdropBlur {
    /// Sigma to paint with: `0.0` for a zero, negative or non-finite value.
    pub fn sigma(self) -> f32 {
        finite_sigma(self.0)
    }
}

fn finite_sigma(sigma: f32) -> f32 {
    if sigma.is_finite() && sigma > 0.0 {
        sigma
    } else {
        0.0
    }
}

/// Group opacity for an entity and its descendants. `1.0` is opaque.
///
/// Values are clamped to `0.0..=1.0` when painted; a non-finite value paints
/// as opaque. Zero paints nothing but leaves hit testing unchanged.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opacity(
    /// Multiplier on the composited group's alpha.
    pub f32,
);

impl Default for Opacity {
    fn default() -> Self {
        Self(1.0)
    }
}

impl Opacity {
    /// Clamped multiplier. Non-finite values are opaque.
    pub fn alpha(self) -> f32 {
        if self.0.is_finite() {
            self.0.clamp(0.0, 1.0)
        } else {
            1.0
        }
    }
}

/// 2D transform for an entity and its descendants, applied after layout.
///
/// The map is: move [`Self::origin`] to the coordinate origin, scale, rotate,
/// move back, then translate. Hit testing maps the pointer through the
/// inverse, so a scaled or rotated control is pressed where it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    /// Offset in logical points, x then y, applied last.
    pub translate: [f32; 2],
    /// Scale factors, x then y. `[1.0, 1.0]` keeps the size.
    pub scale: [f32; 2],
    /// Rotation in radians, clockwise on screen.
    pub rotate: f32,
    /// Pivot as a fraction of the entity's layout rect. `[0.5, 0.5]` is the
    /// center, `[0.0, 0.0]` the top-left corner.
    pub origin: [f32; 2],
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    /// No movement, unit scale, no rotation, centered pivot.
    pub const IDENTITY: Self = Self {
        translate: [0.0, 0.0],
        scale: [1.0, 1.0],
        rotate: 0.0,
        origin: [0.5, 0.5],
    };

    /// Offset by `x`, `y` logical points.
    pub fn translate(x: f32, y: f32) -> Self {
        Self {
            translate: [x, y],
            ..Self::IDENTITY
        }
    }

    /// Uniform scale about the center.
    pub fn scale(scale: f32) -> Self {
        Self {
            scale: [scale, scale],
            ..Self::IDENTITY
        }
    }

    /// Rotation in radians about the center.
    pub fn rotate(radians: f32) -> Self {
        Self {
            rotate: radians,
            ..Self::IDENTITY
        }
    }

    /// Same transform with a different pivot.
    pub fn with_origin(mut self, x: f32, y: f32) -> Self {
        self.origin = [x, y];
        self
    }

    /// Maps every point to itself.
    pub fn is_identity(&self) -> bool {
        self.translate == [0.0, 0.0] && self.scale == [1.0, 1.0] && self.rotate == 0.0
    }

    /// Column-vector matrix `a, b, c, d, tx, ty` in logical points for an
    /// entity laid out at `rect`:
    /// `x' = a * x + c * y + tx`, `y' = b * x + d * y + ty`.
    ///
    /// Non-finite fields fall back to their identity values.
    pub fn matrix(&self, rect: Rect) -> [f32; 6] {
        let finite = |v: f32, fallback: f32| if v.is_finite() { v } else { fallback };
        let sx = finite(self.scale[0], 1.0);
        let sy = finite(self.scale[1], 1.0);
        let (sin, cos) = finite(self.rotate, 0.0).sin_cos();
        let px = rect.x + rect.w * finite(self.origin[0], 0.5);
        let py = rect.y + rect.h * finite(self.origin[1], 0.5);
        let (a, b, c, d) = (cos * sx, sin * sx, -sin * sy, cos * sy);
        let tx = px - (a * px + c * py) + finite(self.translate[0], 0.0);
        let ty = py - (b * px + d * py) + finite(self.translate[1], 0.0);
        [a, b, c, d, tx, ty]
    }
}

/// Map `p` through the column-vector matrix `m`.
pub(crate) fn apply(m: [f32; 6], p: [f32; 2]) -> [f32; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}

/// Inverse of `m`, or `None` when it collapses an axis.
pub(crate) fn invert(m: [f32; 6]) -> Option<[f32; 6]> {
    let det = m[0] * m[3] - m[2] * m[1];
    if !det.is_finite() || det.abs() < 1.0e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let (a, b, c, d) = (m[3] * inv, -m[1] * inv, -m[2] * inv, m[0] * inv);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect() -> Rect {
        Rect {
            x: 10.0,
            y: 20.0,
            w: 40.0,
            h: 20.0,
        }
    }

    #[test]
    fn identity_and_default_matrix_is_identity() {
        assert!(Transform::default().is_identity());
        assert_eq!(
            Transform::default().matrix(rect()),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn scale_keeps_the_pivot_fixed() {
        let m = Transform::scale(2.0).matrix(rect());
        assert_eq!(apply(m, [30.0, 30.0]), [30.0, 30.0]);
        assert_eq!(apply(m, [10.0, 20.0]), [-10.0, 10.0]);
        let corner = Transform::scale(2.0).with_origin(0.0, 0.0).matrix(rect());
        assert_eq!(apply(corner, [10.0, 20.0]), [10.0, 20.0]);
        assert_eq!(apply(corner, [50.0, 40.0]), [90.0, 60.0]);
    }

    #[test]
    fn rotation_is_clockwise_and_invertible() {
        let t = Transform {
            translate: [3.0, -4.0],
            ..Transform::rotate(std::f32::consts::FRAC_PI_2)
        };
        let m = t.matrix(rect());
        // Right of the center moves below it on a y-down screen.
        let p = apply(m, [40.0, 30.0]);
        assert!((p[0] - 33.0).abs() < 1.0e-4 && (p[1] - 36.0).abs() < 1.0e-4);
        let back = apply(invert(m).unwrap(), p);
        assert!((back[0] - 40.0).abs() < 1.0e-4 && (back[1] - 30.0).abs() < 1.0e-4);
    }

    #[test]
    fn collapsed_and_non_finite_values() {
        assert!(invert(Transform::scale(0.0).matrix(rect())).is_none());
        assert_eq!(
            Transform::scale(f32::NAN).matrix(rect()),
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
        );
        assert_eq!(Opacity(f32::INFINITY).alpha(), 1.0);
        assert_eq!(Opacity(-1.0).alpha(), 0.0);
    }
}
