use crate::caret::{EditField, Field};
use crate::entity::Entity;
use crate::font::{field_text_width, layout_text, line_col, BitmapFont, TextLayout};
use crate::layout::{Layout, Rect, Size};
use crate::pane::Pane;
use crate::style::{Label, CELL_H, CELL_W, TEXT_ORIGIN_X, TEXT_ORIGIN_Y};
use crate::text::{style_of, text_engine};
use crate::world::World;

/// Atlas glyph size after the render crate's 2x draw scale.
pub const GLYPH_PX: f32 = CELL_W;
/// Left/right inset used when drawing a field label.
pub const TEXT_INSET_X: f32 = TEXT_ORIGIN_X;
/// Top inset of the first glyph row. Matches the render crate.
pub const TEXT_INSET_Y: f32 = TEXT_ORIGIN_Y;

/// A visual row after wrap. Byte range in the source; never includes a trailing `\n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualLine {
    /// First byte of the row. A wrapped row starts after the break.
    pub start: usize,
    /// Byte after the last included scalar. The trailing newline is excluded.
    pub end: usize,
}

/// How many glyphs fit in a field of `width` logical points.
///
/// Both sides lose [`TEXT_INSET_X`], then the remainder is divided by
/// [`GLYPH_PX`]. The count is at least 1. These are bitmap cells, not
/// shaped clusters.
pub fn cols_for(width: f32) -> usize {
    let inner = (width - 2.0 * TEXT_INSET_X).max(GLYPH_PX);
    (inner / GLYPH_PX).floor().max(1.0) as usize
}

/// Height that fits `n` rows of `line_h`, and at least one tree row.
pub fn field_height_for(n: usize, line_h: f32) -> f32 {
    let lines = n.max(1) as f32;
    let h = TEXT_ORIGIN_Y + lines * line_h + TEXT_ORIGIN_Y;
    h.max(36.0)
}

/// Height that fits `n` bitmap glyph rows, and at least one tree row.
pub fn field_height_lines(n: usize) -> f32 {
    field_height_for(n, CELL_H)
}

/// Greedy wrap on the bitmap face. `\n` always starts a new row.
/// A word longer than `cols` cells hard-breaks. `usize::MAX` does not wrap.
///
/// Empty text yields one empty line. A column is [`CELL_W`] points on the
/// bitmap face.
pub fn wrap_lines(text: &str, cols: usize) -> Vec<VisualLine> {
    let max_width = if cols == usize::MAX {
        f32::INFINITY
    } else {
        cols.max(1) as f32 * CELL_W
    };
    layout_text(&BitmapFont, text, max_width)
        .lines
        .into_iter()
        .map(|line| VisualLine {
            start: line.start,
            end: line.end,
        })
        .collect()
}

/// `(visual_line, column)` for a byte index.
///
/// The column is a byte offset from that line's start. An index past the
/// last line lands on that line, at its end. An empty slice returns `(0, 0)`.
pub fn visual_pos(lines: &[VisualLine], index: usize) -> (usize, usize) {
    let ranges: Vec<(usize, usize)> = lines.iter().map(|line| (line.start, line.end)).collect();
    line_col(&ranges, index)
}

/// Selected runs as `(visual_line, start_col, end_col)` in paint order.
///
/// Columns are byte offsets from [`VisualLine::start`]. An empty selection
/// returns an empty vec. A blank line inside the range is `(line, 0, 0)`.
pub fn visual_selection_spans(
    lines: &[VisualLine],
    anchor: usize,
    index: usize,
) -> Vec<(usize, usize, usize)> {
    let start = anchor.min(index);
    let end = anchor.max(index);
    if start == end {
        return Vec::new();
    }
    let mut spans = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.start == line.end {
            if line.start >= start && line.start < end {
                spans.push((i, 0, 0));
            }
            continue;
        }
        if line.end <= start || line.start >= end {
            continue;
        }
        let from = start.max(line.start);
        let to = end.min(line.end);
        if from < to {
            spans.push((i, from - line.start, to - line.start));
        }
    }
    spans
}

/// Byte index at `col` on visual `line`.
///
/// `line` past the end uses the last line. `col` past the line's bytes
/// stops at the line's end. An empty slice returns `0`.
pub fn index_at_visual(lines: &[VisualLine], line: usize, col: usize) -> usize {
    let Some(vis) = lines.get(line.min(lines.len().saturating_sub(1))) else {
        return 0;
    };
    vis.start + col.min(vis.end.saturating_sub(vis.start))
}

/// Byte index under a pointer in a field [`Rect`]. Title uses `cols = usize::MAX`.
///
/// The hit test uses the bitmap face, after removing [`TEXT_INSET_X`] and
/// [`TEXT_INSET_Y`] from the point in `rect`'s space. `usize::MAX` does not
/// wrap.
pub fn index_at_pointer(text: &str, cols: usize, rect: Rect, x: f32, y: f32) -> usize {
    let max_width = if cols == usize::MAX {
        f32::INFINITY
    } else {
        cols.max(1) as f32 * CELL_W
    };
    let layout = layout_text(&BitmapFont, text, max_width);
    layout.index_at(x - rect.x - TEXT_INSET_X, y - rect.y - TEXT_INSET_Y)
}

/// Fresh layout of `text` on `entity`. Body fields wrap to the row width.
/// Uses the world's [`crate::TextFace`] when one is set.
///
/// A [`crate::Field`] of kind [`crate::EditField::Body`] wraps to the row.
/// Other fields get an unbounded width. [`crate::Spans`] on the entity are
/// passed through. The engine may reuse a cached line. The layout is
/// returned and is not written onto the entity.
pub fn layout_of(world: &World, entity: Entity, text: &str) -> TextLayout {
    with_text_layout(world, entity, text, TextLayout::clone)
}

/// Borrow the cached layout when it still matches `text`, otherwise build one.
pub(crate) fn with_text_layout<R>(
    world: &World,
    entity: Entity,
    text: &str,
    f: impl FnOnce(&TextLayout) -> R,
) -> R {
    let style = style_of(world);
    let engine = text_engine(world);
    let wrap = world
        .get::<Field>(entity)
        .is_some_and(|field| field.kind == EditField::Body);
    let max_width = if wrap {
        world
            .get::<Rect>(entity)
            .map(|rect| field_text_width(rect.w, engine.unit_advance(&style)))
            .unwrap_or(f32::INFINITY)
    } else {
        f32::INFINITY
    };
    let spans = world
        .get::<crate::font::Spans>(entity)
        .map(|spans| spans.runs.clone())
        .unwrap_or_default();
    let layout = engine.layout(text, &style, &spans, max_width);
    f(&layout)
}

/// Grow every body field so its height matches wrapped lines.
///
/// Each [`crate::Pane`]'s spawned [`crate::EditField::Body`] gets a
/// [`crate::Layout`] height of [`crate::Size::Px`]. The line count comes
/// from the text engine at the row width, or from newline splits when that
/// width is still unknown. The height is at least one tree row.
pub fn apply_wrap(world: &mut World) {
    let panes = world.entities_with::<Pane>();
    for pane in panes {
        let spawned = world
            .get::<Pane>(pane)
            .map(|pane| pane.spawned.clone())
            .unwrap_or_default();
        for entity in spawned {
            if !world
                .get::<Field>(entity)
                .is_some_and(|field| field.kind == EditField::Body)
            {
                continue;
            }
            let text = world
                .get::<Label>(entity)
                .map(|label| label.text.clone())
                .unwrap_or_default();
            let width = world
                .get::<Rect>(entity)
                .map(|rect| rect.w)
                .or_else(|| pane_inner_width(world, pane));
            let style = style_of(world);
            let engine = text_engine(world);
            let n = match width {
                Some(width) => {
                    let max = field_text_width(width, engine.unit_advance(&style));
                    engine.measure(&text, &style, &[], max).line_count
                }
                None => text.split('\n').count().max(1),
            };
            if let Some(layout) = world.get_mut::<Layout>(entity) {
                layout.height = Size::Px(field_height_for(n, engine.line_height(&style)));
            }
        }
    }
}

fn pane_inner_width(world: &World, pane: Entity) -> Option<f32> {
    let layout = world.get::<Layout>(pane)?;
    let rect = world.get::<Rect>(pane)?;
    Some((rect.w - layout.padding * 2.0).max(0.0))
}
