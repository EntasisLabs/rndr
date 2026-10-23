//! 2D affine transforms and the zoom buckets paths and glyphs share.
//!
//! A transform maps local points into the parent. [`Affine::then`] applies
//! `self` first. The camera on a [`crate::Scene`] is the root of that chain.
//! Device pixels are world units times the camera scale times
//! [`rndr_core::Viewport::scale`]. [`zoom_bucket`] quantizes that product so a
//! pan, or a zoom that stays inside one bucket, can keep the tessellation it
//! already paid for.

use std::ops::Mul;

/// Column-vector affine, stored as six coefficients.
///
/// `x' = m[0] * x + m[2] * y + m[4]`
/// `y' = m[1] * x + m[3] * y + m[5]`
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// `a, b, c, d, tx, ty` for the matrix above.
    pub m: [f32; 6],
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    /// Identity. Points are unchanged.
    pub const IDENTITY: Self = Self {
        m: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    };

    /// Translation by `(x, y)` in the same units as the points it moves.
    pub const fn translate(x: f32, y: f32) -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 1.0, x, y],
        }
    }

    /// Uniform scale about the origin.
    pub const fn scale(s: f32) -> Self {
        Self::scale_xy(s, s)
    }

    /// Independent scales about the origin.
    pub const fn scale_xy(x: f32, y: f32) -> Self {
        Self {
            m: [x, 0.0, 0.0, y, 0.0, 0.0],
        }
    }

    /// Rotation about the origin, in radians, clockwise in y-down space.
    pub fn rotate(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self {
            m: [cos, sin, -sin, cos, 0.0, 0.0],
        }
    }

    /// `self` first, then `next`.
    pub fn then(self, next: Self) -> Self {
        let [a, b, c, d, tx, ty] = self.m;
        let [a2, b2, c2, d2, tx2, ty2] = next.m;
        Self {
            m: [
                a2 * a + c2 * b,
                b2 * a + d2 * b,
                a2 * c + c2 * d,
                b2 * c + d2 * d,
                a2 * tx + c2 * ty + tx2,
                b2 * tx + d2 * ty + ty2,
            ],
        }
    }

    /// Map one point.
    pub fn transform_point(self, p: [f32; 2]) -> [f32; 2] {
        let [a, b, c, d, tx, ty] = self.m;
        [a * p[0] + c * p[1] + tx, b * p[0] + d * p[1] + ty]
    }

    /// Determinant of the linear part. Negative when the transform flips.
    pub fn determinant(self) -> f32 {
        self.m[0] * self.m[3] - self.m[2] * self.m[1]
    }

    /// `sqrt(|det|)`. This is the uniform scale a hairline width is divided by,
    /// and the scale a zoom bucket is chosen from. A pure translation is `1`.
    pub fn uniform_scale(self) -> f32 {
        let det = self.determinant().abs();
        if det.is_finite() && det > 0.0 {
            det.sqrt()
        } else {
            1.0
        }
    }

    /// Inverse, when the linear part is invertible.
    pub fn inverse(self) -> Option<Self> {
        let det = self.determinant();
        if !det.is_finite() || det.abs() < 1.0e-12 {
            return None;
        }
        let [a, b, c, d, tx, ty] = self.m;
        let inv = 1.0 / det;
        let na = d * inv;
        let nb = -b * inv;
        let nc = -c * inv;
        let nd = a * inv;
        Some(Self {
            m: [na, nb, nc, nd, -(na * tx + nc * ty), -(nb * tx + nd * ty)],
        })
    }
}

impl Mul for Affine {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self {
        // `self * rhs` applies `rhs` first, matching matrix multiplication.
        rhs.then(self)
    }
}

/// Power-of-two bucket for `scale` device pixels per world unit.
///
/// A pan does not change the bucket. A zoom that stays between two powers of
/// two does not either, so the tessellation cache key stays put.
pub fn zoom_bucket(scale: f32) -> i32 {
    if !scale.is_finite() || scale <= 0.0 {
        return 0;
    }
    scale.log2().floor() as i32
}

/// Representative scale for [`zoom_bucket`] `bucket`. Tessellation uses this,
/// not the raw scale, so every zoom inside the bucket shares one mesh.
pub fn bucket_scale(bucket: i32) -> f32 {
    2f32.powi(bucket)
}

/// Glyph raster size, in device pixels, snapped to a power of two.
///
/// The floor is 8. Sizes at or above [`OUTLINE_PX`] stay on the outline path
/// instead of growing the atlas, so this never returns more than 64.
pub fn size_bucket(device_px: f32) -> f32 {
    if !device_px.is_finite() || device_px <= 8.0 {
        return 8.0;
    }
    let snapped = 2f32.powf(device_px.log2().round());
    snapped.clamp(8.0, OUTLINE_PX)
}

/// Em sizes at or above this many device pixels are filled as outlines.
///
/// Below it, glyphs are atlas sprites re-rastered when [`size_bucket`] changes.
pub const OUTLINE_PX: f32 = 64.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translate_then_rotate_matches_the_point() {
        let turn = Affine::translate(10.0, 0.0).then(Affine::rotate(std::f32::consts::FRAC_PI_2));
        let p = turn.transform_point([0.0, 0.0]);
        assert!((p[0] - 0.0).abs() < 1.0e-5, "{p:?}");
        assert!((p[1] - 10.0).abs() < 1.0e-5, "{p:?}");
    }

    #[test]
    fn inverse_round_trips() {
        let affine = Affine::translate(3.0, -4.0)
            .then(Affine::rotate(0.4))
            .then(Affine::scale(2.5));
        let back = affine.inverse().unwrap();
        let p = [8.0, 1.5];
        let round = back.transform_point(affine.transform_point(p));
        assert!((round[0] - p[0]).abs() < 1.0e-4);
        assert!((round[1] - p[1]).abs() < 1.0e-4);
    }

    #[test]
    fn uniform_scale_is_the_geometric_mean() {
        let affine = Affine::scale_xy(2.0, 8.0);
        assert!((affine.uniform_scale() - 4.0).abs() < 1.0e-5);
    }

    #[test]
    fn buckets_are_stable_inside_an_octave() {
        assert_eq!(zoom_bucket(1.1), zoom_bucket(1.9));
        assert_ne!(zoom_bucket(1.9), zoom_bucket(2.1));
        assert!((bucket_scale(zoom_bucket(3.0)) - 2.0).abs() < 1.0e-6);
    }

    #[test]
    fn size_buckets_stop_at_the_outline_threshold() {
        assert_eq!(size_bucket(10.0), 8.0);
        assert_eq!(size_bucket(40.0), 32.0);
        assert_eq!(size_bucket(200.0), OUTLINE_PX);
    }
}
