//! Path builders: rounded bars, curves and arcs.

use rndr_core::Rect;
use rndr_paint::Path;

use crate::model::Curve;

/// Which corners of a bar are rounded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Round {
    None,
    /// Top corners (vertical bars).
    Top,
    /// Right corners (horizontal bars).
    End,
}

/// Bar with a rounded leading edge.
pub(crate) fn bar(r: Rect, round: Round) -> Path {
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    if w <= 0.0 || h <= 0.0 {
        return Path::new();
    }
    let r_max = (w / 2.0).min(h / 2.0);
    if round == Round::None || r_max < 0.5 {
        return Path::round_rect(r, 0.0);
    }
    let rad = r_max.min(w.max(h) * 0.45).min(14.0);
    let mut p = Path::builder();
    match round {
        Round::Top => {
            p.move_to([x, y + h])
                .line_to([x, y + rad])
                .quad_to([x, y], [x + rad, y])
                .line_to([x + w - rad, y])
                .quad_to([x + w, y], [x + w, y + rad])
                .line_to([x + w, y + h])
                .close();
        }
        Round::End => {
            p.move_to([x, y])
                .line_to([x + w - rad, y])
                .quad_to([x + w, y], [x + w, y + rad])
                .line_to([x + w, y + h - rad])
                .quad_to([x + w, y + h], [x + w - rad, y + h])
                .line_to([x, y + h])
                .close();
        }
        Round::None => unreachable!(),
    }
    p.build()
}

fn sign(v: f32) -> f32 {
    if v < 0.0 {
        -1.0
    } else {
        1.0
    }
}

/// d3 `curveMonotoneX` tangents.
fn monotone_tangents(pts: &[[f32; 2]]) -> Vec<f32> {
    let n = pts.len();
    let mut t = vec![0.0; n];
    let slope = |a: [f32; 2], b: [f32; 2]| {
        let h = b[0] - a[0];
        if h == 0.0 {
            0.0
        } else {
            (b[1] - a[1]) / h
        }
    };
    for i in 1..n.saturating_sub(1) {
        let (p0, p1, p2) = (pts[i - 1], pts[i], pts[i + 1]);
        let h0 = p1[0] - p0[0];
        let h1 = p2[0] - p1[0];
        let s0 = slope(p0, p1);
        let s1 = slope(p1, p2);
        let p = if h0 + h1 == 0.0 {
            0.0
        } else {
            (s0 * h1 + s1 * h0) / (h0 + h1)
        };
        let v = (sign(s0) + sign(s1)) * s0.abs().min(s1.abs()).min(0.5 * p.abs());
        t[i] = if v.is_finite() { v } else { 0.0 };
    }
    if n >= 3 {
        let end = |a: [f32; 2], b: [f32; 2], inner: f32| {
            let h = b[0] - a[0];
            if h == 0.0 {
                inner
            } else {
                (3.0 * (b[1] - a[1]) / h - inner) / 2.0
            }
        };
        t[0] = end(pts[0], pts[1], t[1]);
        t[n - 1] = end(pts[n - 2], pts[n - 1], t[n - 2]);
    }
    t
}

/// Append `pts` to `p` using `curve`. The first point is a `move_to` when
/// `start` is true, a `line_to` otherwise.
pub(crate) fn trace(p: &mut rndr_paint::PathBuilder, pts: &[[f32; 2]], curve: Curve, start: bool) {
    let Some(first) = pts.first() else { return };
    if start {
        p.move_to(*first);
    } else {
        p.line_to(*first);
    }
    match curve {
        Curve::Linear => {
            for q in &pts[1..] {
                p.line_to(*q);
            }
        }
        Curve::Step => {
            for w in pts.windows(2) {
                p.line_to([w[1][0], w[0][1]]).line_to(w[1]);
            }
        }
        Curve::Smooth if pts.len() < 3 => {
            for q in &pts[1..] {
                p.line_to(*q);
            }
        }
        Curve::Smooth => {
            let t = monotone_tangents(pts);
            for i in 0..pts.len() - 1 {
                let (a, b) = (pts[i], pts[i + 1]);
                let dx = (b[0] - a[0]) / 3.0;
                p.cubic_to(
                    [a[0] + dx, a[1] + dx * t[i]],
                    [b[0] - dx, b[1] - dx * t[i + 1]],
                    b,
                );
            }
        }
    }
}

/// Open polyline or curve through `pts`.
pub(crate) fn line(pts: &[[f32; 2]], curve: Curve) -> Path {
    let mut p = Path::builder();
    trace(&mut p, pts, curve, true);
    p.build()
}

/// Region between the curve through `pts` and the horizontal line `base`.
pub(crate) fn area(pts: &[[f32; 2]], curve: Curve, base: f32) -> Path {
    let (Some(first), Some(last)) = (pts.first(), pts.last()) else {
        return Path::new();
    };
    let mut p = Path::builder();
    p.move_to([first[0], base]);
    trace(&mut p, pts, curve, false);
    p.line_to([last[0], base]).close();
    p.build()
}

/// Point at d3 angle `a` (0 at twelve o'clock, clockwise) and radius `r`.
pub(crate) fn polar(c: [f32; 2], r: f32, a: f32) -> [f32; 2] {
    [c[0] + a.sin() * r, c[1] - a.cos() * r]
}

fn arc_to(p: &mut rndr_paint::PathBuilder, c: [f32; 2], r: f32, a0: f32, a1: f32) {
    let sweep = a1 - a0;
    let steps = (sweep.abs() / std::f32::consts::FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / steps as f32;
    for i in 0..steps {
        let a = a0 + step * i as f32;
        let b = a + step;
        let k = 4.0 / 3.0 * (step / 4.0).tan() * r;
        let pa = polar(c, r, a);
        let pb = polar(c, r, b);
        p.cubic_to(
            [pa[0] + k * a.cos(), pa[1] + k * a.sin()],
            [pb[0] - k * b.cos(), pb[1] - k * b.sin()],
            pb,
        );
    }
}

/// Open arc at radius `r` from `a0` to `a1`.
pub(crate) fn arc(c: [f32; 2], r: f32, a0: f32, a1: f32) -> Path {
    let mut p = Path::builder();
    p.move_to(polar(c, r, a0));
    arc_to(&mut p, c, r, a0, a1);
    p.build()
}

/// Annular sector. `pad` is the angular gap at the outer radius; the inner
/// edge keeps the same linear gap.
pub(crate) fn wedge(c: [f32; 2], inner: f32, outer: f32, a0: f32, a1: f32, pad: f32) -> Path {
    let half = pad / 2.0;
    let (o0, o1) = if a1 - a0 > pad {
        (a0 + half, a1 - half)
    } else {
        let m = (a0 + a1) / 2.0;
        (m, m)
    };
    if o1 <= o0 {
        return Path::new();
    }
    let mut p = Path::builder();
    p.move_to(polar(c, outer, o0));
    arc_to(&mut p, c, outer, o0, o1);
    if inner > 0.0 {
        let ih = (half * outer / inner).min((a1 - a0) / 2.0);
        let (i0, i1) = (a0 + ih, a1 - ih);
        p.line_to(polar(c, inner, i1));
        arc_to(&mut p, c, inner, i1, i0);
    } else {
        p.line_to(c);
    }
    p.close();
    p.build()
}
