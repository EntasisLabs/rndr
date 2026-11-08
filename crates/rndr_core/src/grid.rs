//! Column grid layout: tracks, auto-placement and spans.

use crate::entity::Entity;
use crate::world::{Component, World};

impl Component for Grid {}
impl Component for GridSpan {}

/// One column's sizing rule.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Track {
    /// Fixed width in points.
    Px(f32),
    /// Share of the width left after fixed and auto columns and gaps.
    Fr(f32),
    /// Widest single-column child that is not [`crate::Size::Fill`].
    Auto,
}

/// Lay children out in columns instead of a stack, like CSS grid with
/// `grid-auto-flow: row`. Replaces [`crate::Layout::axis`] and
/// [`crate::Layout::gap`] for this container; padding, sizing and
/// [`crate::Absolute`] children behave as for a stack.
///
/// Children fill cells row by row; a [`GridSpan`] child takes several columns
/// and starts a new row when it does not fit. Each row is as tall as its
/// tallest child. A child with [`crate::Size::Fill`] width takes its cell's
/// width and with Fill height its row's height. [`crate::Align::items`] and
/// [`crate::AlignSelf`] place children vertically in their row. With no
/// columns the grid has one `Fr(1.0)` column.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grid {
    /// Column tracks, left to right.
    pub columns: Vec<Track>,
    /// Space between columns, in points.
    pub column_gap: f32,
    /// Space between rows, in points.
    pub row_gap: f32,
}

impl Grid {
    /// `count` equal fractional columns with `gap` between rows and columns.
    pub fn equal(count: usize, gap: f32) -> Self {
        Self {
            columns: vec![Track::Fr(1.0); count.max(1)],
            column_gap: gap,
            row_gap: gap,
        }
    }
}

/// Number of grid columns a child occupies. Clamped to the column count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSpan(pub usize);

/// A child's cell: row, first column and column count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Cell {
    pub row: usize,
    pub col: usize,
    pub span: usize,
}

pub(crate) fn tracks(grid: &Grid) -> Vec<Track> {
    if grid.columns.is_empty() {
        vec![Track::Fr(1.0)]
    } else {
        grid.columns.clone()
    }
}

pub(crate) fn auto_place(world: &World, columns: usize, kids: &[Entity]) -> (Vec<Cell>, usize) {
    let mut cells = Vec::with_capacity(kids.len());
    let (mut row, mut col) = (0, 0);
    for child in kids {
        let span = world
            .get::<GridSpan>(*child)
            .map_or(1, |s| s.0)
            .clamp(1, columns);
        if col + span > columns {
            row += 1;
            col = 0;
        }
        cells.push(Cell { row, col, span });
        col += span;
        if col >= columns {
            row += 1;
            col = 0;
        }
    }
    let rows = cells.last().map_or(0, |c| c.row + 1);
    (cells, rows)
}

/// Column widths. `widths[i]` is `None` for a child whose width is Fill.
/// `inner` is the grid's content width, or `None` while measuring, when
/// fractional columns size like [`Track::Auto`].
pub(crate) fn columns(
    grid: &Grid,
    cells: &[Cell],
    widths: &[Option<f32>],
    inner: Option<f32>,
) -> Vec<f32> {
    let tracks = tracks(grid);
    let mut auto = vec![0.0f32; tracks.len()];
    for (cell, w) in cells.iter().zip(widths) {
        if let (1, Some(w)) = (cell.span, w) {
            auto[cell.col] = auto[cell.col].max(*w);
        }
    }
    let gaps = grid.column_gap * tracks.len().saturating_sub(1) as f32;
    let mut out: Vec<f32> = tracks
        .iter()
        .zip(&auto)
        .map(|(t, a)| match t {
            Track::Px(px) => px.max(0.0),
            Track::Auto => *a,
            Track::Fr(_) => 0.0,
        })
        .collect();
    let total_fr: f32 = tracks
        .iter()
        .map(|t| match t {
            Track::Fr(f) if *f > 0.0 => *f,
            _ => 0.0,
        })
        .sum();
    match inner {
        Some(inner) if total_fr > 0.0 => {
            let free = (inner - gaps - out.iter().sum::<f32>()).max(0.0);
            for (o, t) in out.iter_mut().zip(&tracks) {
                if let Track::Fr(f) = t {
                    *o = free * f.max(0.0) / total_fr;
                }
            }
        }
        _ => {
            for ((o, t), a) in out.iter_mut().zip(&tracks).zip(&auto) {
                if let Track::Fr(_) = t {
                    *o = *a;
                }
            }
        }
    }
    out
}

/// Row heights. `heights[i]` is `None` for a child whose height is Fill.
pub(crate) fn rows(cells: &[Cell], heights: &[Option<f32>], count: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; count];
    for (cell, h) in cells.iter().zip(heights) {
        if let Some(h) = h {
            out[cell.row] = out[cell.row].max(*h);
        }
    }
    out
}

/// Start of each track, from zero.
pub(crate) fn starts(sizes: &[f32], gap: f32) -> Vec<f32> {
    let mut at = 0.0;
    sizes
        .iter()
        .map(|s| {
            let start = at;
            at += s + gap;
            start
        })
        .collect()
}

pub(crate) fn total(sizes: &[f32], gap: f32) -> f32 {
    sizes.iter().sum::<f32>() + gap * sizes.len().saturating_sub(1) as f32
}

pub(crate) fn span_width(cols: &[f32], cell: Cell, gap: f32) -> f32 {
    total(&cols[cell.col..cell.col + cell.span], gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_share_what_fixed_and_auto_leave() {
        let grid = Grid {
            columns: vec![Track::Px(50.0), Track::Auto, Track::Fr(1.0), Track::Fr(3.0)],
            column_gap: 10.0,
            row_gap: 0.0,
        };
        let cells = [
            Cell {
                row: 0,
                col: 1,
                span: 1,
            },
            Cell {
                row: 0,
                col: 2,
                span: 2,
            },
        ];
        let cols = columns(&grid, &cells, &[Some(40.0), Some(500.0)], Some(250.0));
        assert_eq!(cols, vec![50.0, 40.0, 32.5, 97.5]);
        let measured = columns(&grid, &cells, &[Some(40.0), None], None);
        assert_eq!(measured, vec![50.0, 40.0, 0.0, 0.0]);
    }
}
