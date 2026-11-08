//! Pie, donut, radar and radial charts, built in a square view box.

use std::f32::consts::{FRAC_PI_2, TAU};

use rndr_paint::{Affine, Cap, Join, Path, Stroke};

use crate::format::{format_label, format_number};
use crate::geom::{self, polar};
use crate::model::{ChartKind, LabelPosition, ValueLabels};
use crate::scene::{size, Anchor, Builder, Region, Shape, DIM};
use crate::theme::alpha;

/// Scale a `view` × `view` box to fit, at most `max` points, centered.
fn fit(width: f32, height: f32, view: f32, max: f32) -> Affine {
    let side = width.min(height).min(max);
    let s = side / view;
    Affine::scale(s).then(Affine::translate(
        (width - side) / 2.0,
        (height - side) / 2.0,
    ))
}

fn side_anchor(dx: f32) -> Anchor {
    if dx.abs() < 1.0 {
        Anchor::Middle
    } else if dx > 0.0 {
        Anchor::Start
    } else {
        Anchor::End
    }
}

pub(crate) fn pie(b: &mut Builder<'_>, width: f32, height: f32) -> Affine {
    const VIEW: f32 = 260.0;
    let chart = b.chart;
    let c = [VIEW / 2.0, VIEW / 2.0];
    let donut = chart.kind == ChartKind::Donut;
    let outer = 78.0;
    let inner = if donut { 44.0 } else { 0.0 };
    let series = &chart.series[0];
    let values: Vec<f64> = (0..chart.categories.len())
        .map(|i| series.value(i).max(0.0))
        .collect();
    let total: f64 = values.iter().sum();
    let pad = if chart.separator { 0.025 } else { 0.0 };

    if let Some(plate) = chart
        .surface
        .is_explicit()
        .then(|| b.theme.surface(chart.surface))
        .flatten()
    {
        b.fill(Path::ellipse(c, [outer + 10.0, outer + 10.0]), plate);
    }
    let labels = chart.resolved_label_position();
    let mut a0 = 0.0f32;
    for (i, cat) in chart.categories.iter().enumerate() {
        let v = values[i];
        let sweep = if total > 0.0 {
            (v / total) as f32 * TAU
        } else {
            0.0
        };
        let a1 = a0 + sweep;
        let color = b.color(i);
        let active = chart.is_active(&[cat]);
        if sweep > 0.0 {
            let path = if sweep >= TAU - 1e-4 && pad == 0.0 && inner == 0.0 {
                Path::ellipse(c, [outer, outer])
            } else {
                geom::wedge(c, inner, outer, a0, a1, pad)
            };
            b.mark(Shape::Fill(path, color), cat, active, DIM);
            let mid = (a0 + a1) / 2.0;
            let text = format_label(chart.labels, cat, v);
            match labels {
                Some(LabelPosition::Inside) => {
                    let r = if donut {
                        (inner + outer) / 2.0
                    } else {
                        outer * 0.6
                    };
                    let halo = b.theme.halo;
                    b.text(
                        text,
                        polar(c, r, mid),
                        size::PIE,
                        650,
                        alpha(halo, 1.0),
                        Anchor::Middle,
                    );
                }
                Some(_) => {
                    let start = polar(c, outer, mid);
                    let elbow = polar(c, 90.0, mid);
                    let label = polar(c, 102.0, mid);
                    let side = if label[0] >= c[0] { 1.0 } else { -1.0 };
                    let end = [label[0] + side * 8.0, label[1]];
                    let mut p = Path::builder();
                    p.move_to(start).line_to(elbow).line_to(end);
                    b.stroke(p.build(), b.theme.grid, 1.0);
                    let anchor = if side > 0.0 {
                        Anchor::Start
                    } else {
                        Anchor::End
                    };
                    b.text(
                        text,
                        [end[0] + side * 4.0, end[1]],
                        size::PIE,
                        650,
                        b.theme.fg_secondary,
                        anchor,
                    );
                }
                None => {}
            }
            let tip = vec![b.tip(&series.label, v, color)];
            let region = Region::Wedge {
                center: c,
                inner,
                outer,
                start: a0,
                end: a1,
            };
            b.hit(region, Some(cat), cat.clone(), tip);
        }
        a0 = a1;
    }
    if donut {
        let value = if chart.center_value.is_empty() {
            format_number(total)
        } else {
            chart.center_value.clone()
        };
        let label_y = if chart.center_label.is_empty() {
            c[1]
        } else {
            c[1] - 2.0
        };
        b.text(
            value,
            [c[0], label_y],
            size::CENTER_VALUE,
            700,
            b.theme.fg,
            Anchor::Middle,
        );
        b.text(
            chart.center_label.clone(),
            [c[0], c[1] + 16.0],
            size::CENTER_LABEL,
            500,
            b.theme.fg_muted,
            Anchor::Middle,
        );
    }
    fit(width, height, VIEW, 272.0)
}

pub(crate) fn radar(b: &mut Builder<'_>, width: f32, height: f32) -> Affine {
    const VIEW: f32 = 300.0;
    let chart = b.chart;
    let c = [VIEW / 2.0, VIEW / 2.0];
    let radius = 92.0;
    let n = chart.categories.len();
    let angle = |i: usize| TAU * i as f32 / n as f32;
    let ring =
        |f: f32| -> Vec<[f32; 2]> { (0..n).map(|i| polar(c, radius * f, angle(i))).collect() };
    let max = {
        let mut flat = chart.clone();
        flat.stacked = false;
        flat.value_max()
    };

    if let Some(plate) = b.theme.surface(chart.surface) {
        b.fill(Path::polygon(&ring(1.0)), plate);
    }
    let grid = alpha(b.theme.grid, 0.16);
    for f in [0.25, 0.5, 0.75, 1.0] {
        b.stroke(Path::polygon(&ring(f)), grid, 1.0);
    }
    for (i, cat) in chart.categories.iter().enumerate() {
        let p = polar(c, radius, angle(i));
        b.stroke(Path::line(c, p), alpha(b.theme.grid, 0.14), 1.0);
        let l = polar(c, radius + 26.0, angle(i));
        b.text(
            cat.clone(),
            l,
            size::AXIS,
            600,
            b.theme.fg_secondary,
            side_anchor(l[0] - c[0]),
        );
    }
    let dot = if chart.labels == ValueLabels::None {
        2.0
    } else {
        2.75
    };
    for (si, s) in chart.series.iter().enumerate() {
        let color = b.color(si);
        let active = chart.is_active(&[&s.key, &s.label]);
        let pts: Vec<[f32; 2]> = (0..n)
            .map(|i| {
                polar(
                    c,
                    radius * ((s.value(i).max(0.0) / max) as f32).min(1.0),
                    angle(i),
                )
            })
            .collect();
        b.mark(
            Shape::Fill(Path::polygon(&pts), alpha(color, 0.2)),
            &s.key,
            active,
            DIM,
        );
        let stroke = Stroke::solid(2.0).with_join(Join::Round);
        b.mark(
            Shape::Stroke(Path::polygon(&pts), color, stroke),
            &s.key,
            active,
            DIM,
        );
        for (i, p) in pts.iter().enumerate() {
            b.mark(
                Shape::Fill(Path::ellipse(*p, [dot, dot]), color),
                &s.key,
                active,
                DIM,
            );
            if chart.labels != ValueLabels::None {
                let text = format_label(chart.labels, &chart.categories[i], s.value(i));
                b.text(
                    text,
                    [p[0], p[1] - 9.0],
                    size::VALUE,
                    600,
                    b.theme.fg_secondary,
                    Anchor::Middle,
                );
            }
            let tip = vec![b.tip(&s.label, s.value(i), color)];
            b.hit(
                Region::Circle(*p, 9.0),
                Some(&s.key),
                chart.categories[i].clone(),
                tip,
            );
        }
    }
    fit(width, height, VIEW, 304.0)
}

pub(crate) fn radial(b: &mut Builder<'_>, width: f32, height: f32) -> Affine {
    const VIEW: f32 = 240.0;
    let chart = b.chart;
    let c = [VIEW / 2.0, VIEW / 2.0];
    let series = &chart.series[0];
    let n = chart.categories.len();
    let values: Vec<f64> = (0..n).map(|i| series.value(i)).collect();
    let max = values.iter().copied().fold(1.0, f64::max);
    let start = -FRAC_PI_2;
    if let Some(plate) = chart
        .surface
        .is_explicit()
        .then(|| b.theme.surface(chart.surface))
        .flatten()
    {
        b.fill(Path::ellipse(c, [98.0, 98.0]), plate);
    }
    let rings: Vec<(usize, f32, f32)> = if n >= 3 {
        (0..n)
            .map(|i| (i, 92.0 - i as f32 * 19.0, 14.0))
            .filter(|r| r.1 - r.2 > 0.0)
            .collect()
    } else {
        vec![(0, 74.0, 24.0)]
    };
    for &(i, outer, thick) in &rings {
        let cat = &chart.categories[i];
        let key = if n >= 3 { cat.as_str() } else { "single" };
        let color = if n >= 3 { b.color(i) } else { b.color(0) };
        let mid = outer - thick / 2.0;
        let ratio = ((values[i] / max) as f32).clamp(0.0, 1.0);
        let active = chart.is_active(&[cat, &series.key, &series.label]);
        let track = Stroke::solid(thick);
        b.stroke_track(Path::ellipse(c, [mid, mid]), alpha(color, 0.16), track);
        if ratio > 0.0 {
            let stroke = Stroke::solid(thick).with_cap(Cap::Round);
            let path = if ratio >= 1.0 {
                Path::ellipse(c, [mid, mid])
            } else {
                geom::arc(c, mid, start, start + ratio * TAU)
            };
            b.mark(Shape::Stroke(path, color, stroke), key, active, DIM);
        }
        if n >= 3 {
            let text = format_label(
                if chart.labels == ValueLabels::None {
                    ValueLabels::Category
                } else {
                    chart.labels
                },
                cat,
                values[i],
            );
            b.text(
                text,
                [c[0] - 6.0, c[1] - mid],
                size::RING,
                600,
                b.theme.fg_secondary,
                Anchor::End,
            );
        }
        let tip = vec![b.tip(&series.label, values[i], color)];
        let region = Region::Wedge {
            center: c,
            inner: outer - thick,
            outer,
            start: 0.0,
            end: TAU,
        };
        b.hit(region, Some(key), cat.clone(), tip);
    }
    let center = if n >= 3 {
        (chart.center_value.clone(), chart.center_label.clone())
    } else {
        let value = if chart.center_value.is_empty() {
            format_number(values[0])
        } else {
            chart.center_value.clone()
        };
        let label = if chart.center_label.is_empty() {
            series.label.clone()
        } else {
            chart.center_label.clone()
        };
        (value, label)
    };
    let value_y = if center.1.is_empty() {
        c[1]
    } else {
        c[1] - 2.0
    };
    b.text(
        center.0,
        [c[0], value_y],
        size::CENTER_VALUE,
        700,
        b.theme.fg,
        Anchor::Middle,
    );
    b.text(
        center.1,
        [c[0], c[1] + 16.0],
        size::CENTER_LABEL,
        500,
        b.theme.fg_muted,
        Anchor::Middle,
    );
    fit(width, height, VIEW, 240.0)
}
