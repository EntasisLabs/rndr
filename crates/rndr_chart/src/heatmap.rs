//! Row × column heatmap. Sizes its own height.

use rndr_core::Rect;
use rndr_paint::Path;

use crate::format::{format_label, format_number};
use crate::scene::{size, Anchor, Builder, Region, Shape, TipLine};
use crate::theme::{alpha, Swatch};

const HEADER: f32 = 24.0;
const CELL_H: f32 = 32.8;
const ROW_GAP: f32 = 6.08;
const COL_GAP: f32 = 6.4;

/// Height of a heatmap with `rows` rows.
pub(crate) fn height(rows: usize) -> f32 {
    HEADER + rows as f32 * (CELL_H + ROW_GAP) - ROW_GAP
}

pub(crate) fn build(b: &mut Builder<'_>, width: f32) -> f32 {
    let chart = b.chart;
    let Some(m) = chart.matrix.as_ref() else {
        return HEADER;
    };
    let chars = m.rows.iter().map(|r| r.chars().count()).max().unwrap_or(0) as f32;
    let label_w = (chars * 0.6 * size::AXIS + 8.0).clamp(72.0, 112.0);
    let cols = m.cols.len().max(1) as f32;
    let cell_w = ((width - label_w - COL_GAP * cols) / cols).max(4.0);
    let base = match chart.colors.first() {
        Some(c) => b.theme.color(*c),
        None => b.theme.swatch(Swatch::Blue),
    };
    let flat = m.values.iter().flatten().copied();
    let lo = flat.clone().fold(f64::INFINITY, f64::min);
    let mut hi = flat.fold(f64::NEG_INFINITY, f64::max);
    if hi <= lo {
        hi = lo + 1.0;
    }
    let axis = b.theme.fg_secondary;
    let x0 = label_w + COL_GAP;
    for (ci, col) in m.cols.iter().enumerate() {
        let x = x0 + ci as f32 * (cell_w + COL_GAP) + cell_w / 2.0;
        b.text(
            col.clone(),
            [x, HEADER / 2.0],
            size::AXIS,
            600,
            axis,
            Anchor::Middle,
        );
    }
    for (ri, row) in m.rows.iter().enumerate() {
        let y = HEADER + ri as f32 * (CELL_H + ROW_GAP);
        b.text(
            row.clone(),
            [label_w, y + CELL_H / 2.0],
            size::AXIS,
            600,
            axis,
            Anchor::End,
        );
        for (ci, col) in m.cols.iter().enumerate() {
            let v = m.values[ri][ci];
            let t = ((v - lo) / (hi - lo)) as f32;
            let color = alpha(base, base.a * (0.18 + t * 0.78));
            let rect = Rect {
                x: x0 + ci as f32 * (cell_w + COL_GAP),
                y,
                w: cell_w,
                h: CELL_H,
            };
            let key = format!("{row} · {col}");
            let active = chart.is_active(&[row, col, &key]);
            b.mark(
                Shape::Fill(Path::round_rect(rect, 7.2), color),
                &key,
                active,
                1.0,
            );
            let text = format_label(chart.labels, col, v);
            if !text.is_empty() {
                let ink = if t > 0.55 {
                    alpha(b.theme.halo, 1.0)
                } else {
                    b.theme.fg
                };
                b.text(
                    text,
                    [rect.x + cell_w / 2.0, y + CELL_H / 2.0],
                    size::VALUE,
                    650,
                    ink,
                    Anchor::Middle,
                );
            }
            let line = TipLine {
                label: "Value".into(),
                value: format_number(v),
                color: base,
            };
            b.hit(Region::Rect(rect), None, key, vec![line]);
        }
    }
    height(m.rows.len())
}
