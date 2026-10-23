//! Vector paths: lines, curves, and the ellipse, polygon, and line helpers.
//!
//! Coordinates are local units, y down. A [`crate::Canvas`] records a path with
//! a fill, a stroke, or both. Tessellation lives in [`crate::tess`].

use rndr_core::Rect;

/// How a filled contour covers its interior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillRule {
    /// Non-zero winding. A hole needs the opposite direction.
    NonZero,
    /// Even-odd. Alternating contours punch holes.
    EvenOdd,
}

/// Cap at the open end of a stroke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cap {
    /// Stop at the end point.
    Butt,
    /// Extend by half the stroke width.
    Square,
    /// A semicircle of the stroke width.
    Round,
}

/// Join where two stroke segments meet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Join {
    /// Sharp corner, clipped by [`Stroke::miter_limit`].
    Miter,
    /// A cut across the corner.
    Bevel,
    /// An arc of the stroke width.
    Round,
}

/// Stroke decoration. Width is in local units unless [`Self::hairline`] is set.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// Width in local units, or device pixels when [`Self::hairline`] is set.
    pub width: f32,
    /// Cap for both open ends.
    pub cap: Cap,
    /// Join between segments.
    pub join: Join,
    /// Miter length, as a multiple of the width. Values below 1 are raised to 1.
    pub miter_limit: f32,
    /// On/off lengths in local units, along the centerline. Empty means solid.
    ///
    /// An odd-length array is repeated once, matching the usual dash rule, so
    /// the pattern always alternates on and off.
    pub dash: Vec<f32>,
    /// Distance into [`Self::dash`] before the first on-segment, in local units.
    pub dash_offset: f32,
    /// When set, [`Self::width`] is device pixels and does not grow with zoom.
    ///
    /// The tessellation divides it by the current uniform scale, and the mesh
    /// is cached per zoom bucket, so a hairline stays about `width` pixels wide
    /// inside a bucket and is rebuilt when the bucket changes.
    pub hairline: bool,
}

impl Default for Stroke {
    fn default() -> Self {
        Self {
            width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            miter_limit: 4.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            hairline: false,
        }
    }
}

impl Stroke {
    /// Solid stroke of `width` local units, butt caps, miter joins.
    pub fn solid(width: f32) -> Self {
        Self {
            width,
            ..Self::default()
        }
    }

    /// [`Self::solid`] with [`Self::hairline`] set.
    ///
    /// `width` is device pixels.
    pub fn hairline(width: f32) -> Self {
        Self {
            width,
            hairline: true,
            ..Self::default()
        }
    }

    /// Store `pattern` as [`Self::dash`] and return the stroke.
    pub fn dashed(mut self, pattern: impl Into<Vec<f32>>) -> Self {
        self.dash = pattern.into();
        self
    }

    /// Store `cap` and return the stroke.
    pub fn with_cap(mut self, cap: Cap) -> Self {
        self.cap = cap;
        self
    }

    /// Store `join` and return the stroke.
    pub fn with_join(mut self, join: Join) -> Self {
        self.join = join;
        self
    }

    /// Width the tessellator should use at `scale` device pixels per local unit.
    pub fn local_width(&self, scale: f32) -> f32 {
        let width = if self.width.is_finite() {
            self.width
        } else {
            0.0
        };
        if !self.hairline {
            return width.max(0.0);
        }
        let scale = if scale.is_finite() && scale > 1.0e-6 {
            scale
        } else {
            1.0
        };
        (width / scale).max(0.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    Move,
    Line,
    Quad,
    Cubic,
    Close,
}

/// A path in local units. Empty until a [`PathBuilder`] or a helper fills it.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    verbs: Vec<Verb>,
    points: Vec<[f32; 2]>,
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl Path {
    /// No verbs and no points.
    pub fn new() -> Self {
        Self {
            verbs: Vec::new(),
            points: Vec::new(),
        }
    }

    /// Start a builder. [`PathBuilder::build`] returns the path.
    pub fn builder() -> PathBuilder {
        PathBuilder {
            path: Self::new(),
            open: false,
        }
    }

    /// One open segment from `a` to `b`.
    pub fn line(a: [f32; 2], b: [f32; 2]) -> Self {
        let mut path = Self::builder();
        path.move_to(a).line_to(b);
        path.build()
    }

    /// Closed polygon. Fewer than three points yields an empty path.
    pub fn polygon(points: &[[f32; 2]]) -> Self {
        if points.len() < 3 {
            return Self::new();
        }
        let mut path = Self::builder();
        path.move_to(points[0]);
        for point in &points[1..] {
            path.line_to(*point);
        }
        path.close();
        path.build()
    }

    /// Ellipse centered at `center` with radii `(rx, ry)`.
    ///
    /// Built from four cubic quarters. A non-positive radius yields an empty path.
    pub fn ellipse(center: [f32; 2], radii: [f32; 2]) -> Self {
        let (cx, cy) = (center[0], center[1]);
        let (rx, ry) = (radii[0], radii[1]);
        if !rx.is_finite() || !ry.is_finite() || rx <= 0.0 || ry <= 0.0 {
            return Self::new();
        }
        const KAPPA: f32 = 0.552_284_8;
        let kx = rx * KAPPA;
        let ky = ry * KAPPA;
        let mut path = Self::builder();
        path.move_to([cx + rx, cy])
            .cubic_to([cx + rx, cy + ky], [cx + kx, cy + ry], [cx, cy + ry])
            .cubic_to([cx - kx, cy + ry], [cx - rx, cy + ky], [cx - rx, cy])
            .cubic_to([cx - rx, cy - ky], [cx - kx, cy - ry], [cx, cy - ry])
            .cubic_to([cx + kx, cy - ry], [cx + rx, cy - ky], [cx + rx, cy])
            .close();
        path.build()
    }

    /// Rounded rectangle. `radius` is clamped to half the shorter side.
    ///
    /// A non-positive size yields an empty path. A zero radius is a polygon.
    pub fn round_rect(rect: Rect, radius: f32) -> Self {
        if rect.w <= 0.0 || rect.h <= 0.0 {
            return Self::new();
        }
        let radius = if radius.is_finite() {
            radius.max(0.0)
        } else {
            0.0
        };
        let radius = radius.min(rect.w * 0.5).min(rect.h * 0.5);
        if radius <= 0.0 {
            return Self::polygon(&[
                [rect.x, rect.y],
                [rect.x + rect.w, rect.y],
                [rect.x + rect.w, rect.y + rect.h],
                [rect.x, rect.y + rect.h],
            ]);
        }
        const KAPPA: f32 = 0.552_284_8;
        let k = radius * KAPPA;
        let l = rect.x;
        let t = rect.y;
        let r = rect.x + rect.w;
        let b = rect.y + rect.h;
        let mut path = Self::builder();
        path.move_to([l + radius, t])
            .line_to([r - radius, t])
            .cubic_to([r - radius + k, t], [r, t + radius - k], [r, t + radius])
            .line_to([r, b - radius])
            .cubic_to([r, b - radius + k], [r - radius + k, b], [r - radius, b])
            .line_to([l + radius, b])
            .cubic_to([l + radius - k, b], [l, b - radius + k], [l, b - radius])
            .line_to([l, t + radius])
            .cubic_to([l, t + radius - k], [l + radius - k, t], [l + radius, t])
            .close();
        path.build()
    }

    /// No verbs.
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty()
    }

    /// Axis-aligned bounds of the stored points. `None` when the path is empty.
    ///
    /// Curves are bounded by their control points, which contains the curve.
    pub fn bounds(&self) -> Option<Rect> {
        let mut iter = self.points.iter();
        let first = *iter.next()?;
        let mut min_x = first[0];
        let mut min_y = first[1];
        let mut max_x = first[0];
        let mut max_y = first[1];
        for point in iter {
            min_x = min_x.min(point[0]);
            min_y = min_y.min(point[1]);
            max_x = max_x.max(point[0]);
            max_y = max_y.max(point[1]);
        }
        Some(Rect {
            x: min_x,
            y: min_y,
            w: max_x - min_x,
            h: max_y - min_y,
        })
    }

    pub(crate) fn verbs(&self) -> impl Iterator<Item = (Verb, usize)> + '_ {
        let mut cursor = 0usize;
        self.verbs.iter().map(move |verb| {
            let at = cursor;
            cursor += match verb {
                Verb::Move | Verb::Line => 1,
                Verb::Quad => 2,
                Verb::Cubic => 3,
                Verb::Close => 0,
            };
            (*verb, at)
        })
    }

    pub(crate) fn point(&self, index: usize) -> [f32; 2] {
        self.points[index]
    }
}

/// Records verbs into a [`Path`].
#[derive(Clone, Debug)]
pub struct PathBuilder {
    path: Path,
    open: bool,
}

impl PathBuilder {
    /// Start a subpath at `p`.
    ///
    /// An open subpath stays open. Only [`Self::close`] adds the edge back to
    /// its start, so a later `move_to` does not turn a dashed polyline into a
    /// closed loop.
    pub fn move_to(&mut self, p: [f32; 2]) -> &mut Self {
        self.path.verbs.push(Verb::Move);
        self.path.points.push(p);
        self.open = true;
        self
    }

    /// Line from the current point to `p`.
    pub fn line_to(&mut self, p: [f32; 2]) -> &mut Self {
        self.ensure_open(p);
        self.path.verbs.push(Verb::Line);
        self.path.points.push(p);
        self
    }

    /// Quadratic segment. `ctrl` is the control point and `p` the end point.
    pub fn quad_to(&mut self, ctrl: [f32; 2], p: [f32; 2]) -> &mut Self {
        self.ensure_open(p);
        self.path.verbs.push(Verb::Quad);
        self.path.points.push(ctrl);
        self.path.points.push(p);
        self
    }

    /// Cubic segment. `c1` and `c2` are the control points and `p` the end.
    pub fn cubic_to(&mut self, c1: [f32; 2], c2: [f32; 2], p: [f32; 2]) -> &mut Self {
        self.ensure_open(p);
        self.path.verbs.push(Verb::Cubic);
        self.path.points.push(c1);
        self.path.points.push(c2);
        self.path.points.push(p);
        self
    }

    /// Close the current subpath. A later segment starts a new one.
    pub fn close(&mut self) -> &mut Self {
        if self.open {
            self.path.verbs.push(Verb::Close);
            self.open = false;
        }
        self
    }

    /// Finish the path. An open subpath stays open.
    pub fn build(self) -> Path {
        self.path
    }

    fn ensure_open(&mut self, p: [f32; 2]) {
        if !self.open {
            self.move_to(p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_bounds_cover_the_radii() {
        let path = Path::ellipse([10.0, 20.0], [4.0, 6.0]);
        let bounds = path.bounds().unwrap();
        assert!((bounds.x - 6.0).abs() < 1.0e-3);
        assert!((bounds.y - 14.0).abs() < 1.0e-3);
        assert!((bounds.w - 8.0).abs() < 1.0e-3);
        assert!((bounds.h - 12.0).abs() < 1.0e-3);
    }

    #[test]
    fn polygon_rejects_a_degenerate_input() {
        assert!(Path::polygon(&[[0.0, 0.0], [1.0, 0.0]]).is_empty());
    }

    #[test]
    fn hairline_width_shrinks_as_the_scale_grows() {
        let stroke = Stroke::hairline(2.0);
        assert!((stroke.local_width(1.0) - 2.0).abs() < 1.0e-6);
        assert!((stroke.local_width(4.0) - 0.5).abs() < 1.0e-6);
        assert!((Stroke::solid(3.0).local_width(4.0) - 3.0).abs() < 1.0e-6);
    }
}
