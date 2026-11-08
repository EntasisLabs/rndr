//! Bar, line, area and combo charts on a category axis; scatter on two value axes.

use rndr_core::Rect;
use rndr_paint::{Affine, Cap, Join, Path, Stroke};

use crate::format::{format_label, format_number};
use crate::geom::{self, Round};
use crate::model::{ChartKind, Orientation, SeriesMark, ValueLabels};
use crate::scene::{size, Anchor, Builder, Region, Shape, DIM};
use crate::theme::alpha;

/// d3 `scaleBand` with inner padding 0.4 and outer padding 0.18.
pub(crate) struct Band {
    start: f32,
    step: f32,
    pub width: f32,
}

impl Band {
    pub fn new(n: usize, range: f32) -> Self {
        let (inner, outer) = (0.4, 0.18);
        let n = n as f32;
        let step = range / (n - inner + outer * 2.0).max(1.0);
        Self {
            start: (range - step * (n - inner)) / 2.0,
            step,
            width: step * (1.0 - inner),
        }
    }

    pub fn at(&self, i: usize) -> f32 {
        self.start + self.step * i as f32
    }

    pub fn center(&self, i: usize) -> f32 {
        self.at(i) + self.width / 2.0
    }
}

/// Round tick values from zero up to `max`: two to five steps of 1, 2, 2.5 or 5 × 10ⁿ.
pub(crate) fn ticks(max: f64) -> Vec<f64> {
    if !(max.is_finite() && max > 0.0) {
        return vec![0.0];
    }
    let raw = max / 4.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .into_iter()
        .map(|m| m * mag)
        .find(|s| max / s <= 4.0 + 1e-9)
        .unwrap_or(10.0 * mag);
    let count = (max / step + 1e-9).floor() as usize;
    (0..=count).map(|i| i as f64 * step).collect()
}

/// Plot frame for bars, lines, areas and combos. Returns the plot offset.
pub(crate) fn build(b: &mut Builder<'_>, width: f32, height: f32) -> Affine {
    let chart = b.chart;
    let horizontal = chart.kind == ChartKind::Bar && chart.orientation == Orientation::Horizontal;
    let bars = chart.marked(SeriesMark::Bar);
    let lines = chart.marked(SeriesMark::Line);
    let dual = chart.kind == ChartKind::Combo && !bars.is_empty() && !lines.is_empty();
    let (top, right, bottom, left) = if horizontal {
        (8.0, 16.0, 24.0, 56.0)
    } else {
        (10.0, if dual { 44.0 } else { 12.0 }, 28.0, 36.0)
    };
    let pw = (width - left - right).max(1.0);
    let ph = (height - top - bottom).max(1.0);
    let n = chart.categories.len();
    let band = Band::new(n, if horizontal { ph } else { pw });

    let pad = if chart.kind == ChartKind::Combo {
        1.08
    } else {
        1.0
    };
    let (left_max, right_max) = match chart.kind {
        ChartKind::Combo => {
            let bar = chart.value_max_for(SeriesMark::Bar);
            let line = chart.value_max_for(SeriesMark::Line);
            let l = if bar > 0.0 { bar } else { chart.value_max() };
            let r = if dual { line } else { l };
            (l * pad, r * pad)
        }
        _ => (chart.value_max(), chart.value_max()),
    };

    let grid = b.theme.grid;
    let axis = b.theme.fg_secondary;
    for f in ticks(left_max)
        .into_iter()
        .skip(1)
        .map(|v| (v / left_max) as f32)
    {
        let p = if horizontal {
            Path::line([pw * f, 0.0], [pw * f, ph])
        } else {
            Path::line([0.0, ph * (1.0 - f)], [pw, ph * (1.0 - f)])
        };
        b.stroke(p, grid, 1.0);
    }
    let base = if horizontal {
        Path::line([0.0, 0.0], [0.0, ph])
    } else {
        Path::line([0.0, ph], [pw, ph])
    };
    b.stroke(base, grid, 1.0);

    for v in ticks(left_max) {
        let f = (v / left_max) as f32;
        if horizontal {
            b.text(
                format_number(v),
                [pw * f, ph + 14.0],
                size::AXIS,
                550,
                axis,
                Anchor::Middle,
            );
        } else {
            b.text(
                format_number(v),
                [-6.0, ph * (1.0 - f)],
                size::AXIS,
                550,
                axis,
                Anchor::End,
            );
        }
    }
    if dual {
        let color = b.color(lines[0].0);
        for v in ticks(right_max) {
            let f = (v / right_max) as f32;
            b.text(
                format_number(v),
                [pw + 6.0, ph * (1.0 - f)],
                size::AXIS,
                550,
                color,
                Anchor::Start,
            );
        }
    }
    for (i, cat) in chart.categories.iter().enumerate() {
        if horizontal {
            b.text(
                cat.clone(),
                [-6.0, band.center(i)],
                size::AXIS,
                550,
                axis,
                Anchor::End,
            );
        } else {
            b.text(
                cat.clone(),
                [band.center(i), ph + 14.0],
                size::AXIS,
                550,
                axis,
                Anchor::Middle,
            );
        }
    }

    if chart.kind != ChartKind::Line && chart.kind != ChartKind::Area {
        bar_marks(b, &band, horizontal, pw, ph, left_max);
    }
    if chart.kind != ChartKind::Bar {
        line_marks(b, &band, ph, if dual { right_max } else { left_max });
    }
    Affine::translate(left, top)
}

fn bar_marks(b: &mut Builder<'_>, band: &Band, horizontal: bool, pw: f32, ph: f32, max: f64) {
    let chart = b.chart;
    let bars = chart.marked(SeriesMark::Bar);
    if bars.is_empty() {
        return;
    }
    let extent = if horizontal { pw } else { ph };
    let len = |v: f64| ((v.max(0.0) / max) as f32 * extent).min(extent);
    let round = if horizontal { Round::End } else { Round::Top };
    let rect = |offset: f32, thick: f32, from: f32, l: f32| {
        if horizontal {
            Rect {
                x: from,
                y: offset,
                w: l,
                h: thick,
            }
        } else {
            Rect {
                x: offset,
                y: ph - from - l,
                w: thick,
                h: l,
            }
        }
    };
    for (ci, cat) in chart.categories.iter().enumerate() {
        let mut tips = Vec::new();
        if chart.stacked {
            let thick = (band.width * 0.72).max(2.0);
            let offset = band.at(ci) + (band.width - thick) / 2.0;
            let top = bars.iter().rposition(|(_, s)| s.value(ci) > 0.0);
            let mut from = 0.0;
            let mut total = 0.0;
            for (k, (si, s)) in bars.iter().enumerate() {
                let v = s.value(ci);
                let l = len(v);
                let shape = if Some(k) == top { round } else { Round::None };
                let color = b.color(*si);
                let active = chart.is_active(&[cat, &s.key, &s.label]);
                b.mark(
                    Shape::Fill(geom::bar(rect(offset, thick, from, l), shape), color),
                    cat,
                    active,
                    DIM,
                );
                tips.push(b.tip(&s.label, v, color));
                from += l;
                total += v;
            }
            if chart.labels != ValueLabels::None {
                let text = format_label(chart.labels, cat, total);
                let at = if horizontal {
                    [from + 4.0, offset + thick / 2.0]
                } else {
                    [offset + thick / 2.0, ph - from - 9.0]
                };
                let anchor = if horizontal {
                    Anchor::Start
                } else {
                    Anchor::Middle
                };
                b.text(text, at, size::VALUE, 600, b.theme.fg_secondary, anchor);
            }
        } else {
            let slot = band.width / bars.len() as f32;
            let thick = (slot * 0.68).max(2.0);
            for (k, (si, s)) in bars.iter().enumerate() {
                let v = s.value(ci);
                let l = len(v);
                let offset = band.at(ci) + slot * k as f32 + (slot - thick) / 2.0;
                let color = b.color(*si);
                let active = chart.is_active(&[cat, &s.key, &s.label]);
                b.mark(
                    Shape::Fill(geom::bar(rect(offset, thick, 0.0, l), round), color),
                    cat,
                    active,
                    DIM,
                );
                tips.push(b.tip(&s.label, v, color));
                if chart.labels != ValueLabels::None {
                    let text = format_label(chart.labels, cat, v);
                    let (at, anchor) = if horizontal {
                        ([l + 4.0, offset + thick / 2.0], Anchor::Start)
                    } else {
                        ([offset + thick / 2.0, ph - l - 9.0], Anchor::Middle)
                    };
                    b.text(text, at, size::VALUE, 600, b.theme.fg_secondary, anchor);
                }
            }
        }
        let region = if horizontal {
            Rect {
                x: 0.0,
                y: band.at(ci),
                w: pw,
                h: band.width,
            }
        } else {
            Rect {
                x: band.at(ci),
                y: 0.0,
                w: band.width,
                h: ph,
            }
        };
        b.hit(Region::Rect(region), Some(cat), cat.clone(), tips);
    }
}

fn line_marks(b: &mut Builder<'_>, band: &Band, ph: f32, max: f64) {
    let chart = b.chart;
    let area = chart.kind == ChartKind::Area;
    let halo = b.theme.halo;
    for (si, s) in chart.marked(SeriesMark::Line) {
        let color = b.color(si);
        let active = chart.is_active(&[&s.key, &s.label]);
        let pts: Vec<[f32; 2]> = (0..chart.categories.len())
            .map(|i| {
                let f = (s.value(i).max(0.0) / max) as f32;
                [band.center(i), ph * (1.0 - f.min(1.0))]
            })
            .collect();
        if chart.kind != ChartKind::Combo {
            let (a, dim) = if area { (0.22, 0.28) } else { (0.06, DIM) };
            b.mark(
                Shape::Fill(geom::area(&pts, chart.curve, ph), alpha(color, a)),
                &s.key,
                active,
                dim,
            );
        }
        let stroke = Stroke::solid(2.35)
            .with_cap(Cap::Round)
            .with_join(Join::Round);
        b.mark(
            Shape::Stroke(geom::line(&pts, chart.curve), color, stroke),
            &s.key,
            active,
            DIM,
        );
        for (i, p) in pts.iter().enumerate() {
            let cat = &chart.categories[i];
            b.mark(
                Shape::Fill(Path::ellipse(*p, [3.25, 3.25]), color),
                &s.key,
                active,
                DIM,
            );
            b.mark(
                Shape::Stroke(
                    Path::ellipse(*p, [3.25, 3.25]),
                    alpha(halo, halo.a * 0.7),
                    Stroke::solid(1.0),
                ),
                &s.key,
                active,
                DIM,
            );
            if chart.labels != ValueLabels::None {
                let text = format_label(chart.labels, cat, s.value(i));
                b.text(
                    text,
                    [p[0], p[1] - 10.0],
                    size::VALUE,
                    600,
                    b.theme.fg_secondary,
                    Anchor::Middle,
                );
            }
            let tip = vec![b.tip(&s.label, s.value(i), color)];
            b.hit(Region::Circle(*p, 10.0), Some(&s.key), cat.clone(), tip);
        }
    }
}

/// X/Y scatter. Returns the plot offset.
pub(crate) fn scatter(b: &mut Builder<'_>, width: f32, height: f32) -> Affine {
    let chart = b.chart;
    let (top, right, bottom, left) = (12.0, 16.0, 32.0, 40.0);
    let pw = (width - left - right).max(1.0);
    let ph = (height - top - bottom).max(1.0);
    let extent = |vals: Vec<f64>, zero: bool| {
        let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = if hi > lo { hi - lo } else { 1.0 };
        let pad = span * 0.08;
        let lo = if zero { (lo - pad).min(0.0) } else { lo - pad };
        (lo, hi + pad)
    };
    let (x0, x1) = extent(chart.points.iter().map(|p| p.x).collect(), false);
    let (y0, y1) = extent(chart.points.iter().map(|p| p.y).collect(), true);
    let sx = |x: f64| ((x - x0) / (x1 - x0)) as f32 * pw;
    let sy = |y: f64| ph - ((y - y0) / (y1 - y0)) as f32 * ph;

    let grid = b.theme.grid;
    let axis = b.theme.fg_secondary;
    for f in [0.25f32, 0.5, 0.75] {
        b.stroke(Path::line([pw * f, 0.0], [pw * f, ph]), grid, 1.0);
        b.stroke(Path::line([0.0, ph * f], [pw, ph * f]), grid, 1.0);
    }
    b.stroke(Path::line([0.0, ph], [pw, ph]), grid, 1.0);
    b.stroke(Path::line([0.0, 0.0], [0.0, ph]), grid, 1.0);
    for f in [0.0, 0.5, 1.0] {
        let x = x0 + (x1 - x0) * f;
        let y = y0 + (y1 - y0) * f;
        b.text(
            format_number(x),
            [sx(x), ph + 16.0],
            size::AXIS,
            550,
            axis,
            Anchor::Middle,
        );
        b.text(
            format_number(y),
            [-6.0, sy(y)],
            size::AXIS,
            550,
            axis,
            Anchor::End,
        );
    }

    let groups = chart.scatter_groups();
    let halo = b.theme.halo;
    for p in &chart.points {
        let group = p
            .group
            .as_deref()
            .filter(|g| !g.is_empty())
            .unwrap_or(&groups[0]);
        let gi = groups.iter().position(|g| g == group).unwrap_or(0);
        let color = b.color(gi);
        let c = [sx(p.x), sy(p.y)];
        let active = chart.is_active(&[group]);
        b.mark(
            Shape::Fill(Path::ellipse(c, [4.0, 4.0]), color),
            group,
            active,
            DIM,
        );
        b.mark(
            Shape::Stroke(
                Path::ellipse(c, [4.0, 4.0]),
                alpha(halo, halo.a * 0.55),
                Stroke::solid(1.0),
            ),
            group,
            active,
            DIM,
        );
        if chart.labels != ValueLabels::None {
            let text = format!("{}, {}", format_number(p.x), format_number(p.y));
            b.text(
                text,
                [c[0], c[1] - 10.0],
                size::VALUE,
                600,
                axis,
                Anchor::Middle,
            );
        }
        let lines = vec![b.tip("X", p.x, color), b.tip("Y", p.y, color)];
        b.hit(
            Region::Circle(c, 8.0),
            Some(group),
            group.to_string(),
            lines,
        );
    }
    Affine::translate(left, top)
}
