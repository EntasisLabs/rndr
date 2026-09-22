use crate::caret::{EditField, Field};
use crate::entity::Entity;
use crate::layout::{Layout, Rect, Size};
use crate::pane::Pane;
use crate::style::Label;
use crate::world::World;

/// Atlas glyph size after the render crate's 2x draw scale.
pub const GLYPH_PX: f32 = 16.0;
/// Left/right inset used when drawing a field label.
pub const TEXT_INSET_X: f32 = 12.0;
/// Top inset of the first glyph row. Matches the render crate.
pub const TEXT_INSET_Y: f32 = 8.0;

/// A visual row after wrap. Byte range in the source; never includes a trailing `\n`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisualLine {
    pub start: usize,
    pub end: usize,
}

/// How many glyphs fit in a field of `width` pixels.
pub fn cols_for(width: f32) -> usize {
    let inner = (width - 2.0 * TEXT_INSET_X).max(GLYPH_PX);
    (inner / GLYPH_PX).floor().max(1.0) as usize
}

/// `16 + n.max(1) * 20`, same as today's one-line field.
pub fn field_height_lines(n: usize) -> f32 {
    16.0 + n.max(1) as f32 * 20.0
}

/// Greedy wrap. `\n` always starts a new row. A word longer than `cols` hard-breaks.
pub fn wrap_lines(text: &str, cols: usize) -> Vec<VisualLine> {
    let cols = cols.max(1);
    if text.is_empty() {
        return vec![VisualLine { start: 0, end: 0 }];
    }
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for (i, part) in text.split('\n').enumerate() {
        if i > 0 {
            offset += 1;
        }
        wrap_paragraph(part, offset, cols, &mut lines);
        offset += part.len();
    }
    if lines.is_empty() {
        lines.push(VisualLine { start: 0, end: 0 });
    }
    lines
}

/// `(visual_line, column)` for a byte index.
pub fn visual_pos(lines: &[VisualLine], index: usize) -> (usize, usize) {
    if lines.is_empty() {
        return (0, 0);
    }
    for (i, line) in lines.iter().enumerate() {
        let last = i + 1 == lines.len();
        if index < line.start {
            return (i, 0);
        }
        if index < line.end || (index == line.end && (last || lines[i + 1].start != index)) {
            return (i, index - line.start);
        }
    }
    let last = lines.len() - 1;
    (last, lines[last].end.saturating_sub(lines[last].start))
}

/// Byte index at `col` on visual `line`.
pub fn index_at_visual(lines: &[VisualLine], line: usize, col: usize) -> usize {
    let Some(vis) = lines.get(line.min(lines.len().saturating_sub(1))) else {
        return 0;
    };
    vis.start + col.min(vis.end.saturating_sub(vis.start))
}

/// Grow every body field so its height matches wrapped lines.
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
            let n = match width {
                Some(width) => wrap_lines(&text, cols_for(width)).len(),
                None => text.split('\n').count().max(1),
            };
            if let Some(layout) = world.get_mut::<Layout>(entity) {
                layout.height = Size::Px(field_height_lines(n));
            }
        }
    }
}

fn pane_inner_width(world: &World, pane: Entity) -> Option<f32> {
    let layout = world.get::<Layout>(pane)?;
    let rect = world.get::<Rect>(pane)?;
    Some((rect.w - layout.padding * 2.0).max(0.0))
}

fn wrap_paragraph(part: &str, origin: usize, cols: usize, lines: &mut Vec<VisualLine>) {
    if part.is_empty() {
        lines.push(VisualLine {
            start: origin,
            end: origin,
        });
        return;
    }
    let bytes = part.as_bytes();
    let mut at = 0usize;
    while at < bytes.len() {
        let left = bytes.len() - at;
        if left <= cols {
            lines.push(VisualLine {
                start: origin + at,
                end: origin + bytes.len(),
            });
            return;
        }
        let window = &bytes[at..at + cols];
        if let Some(space) = window.iter().rposition(|b| *b == b' ').filter(|i| *i > 0) {
            lines.push(VisualLine {
                start: origin + at,
                end: origin + at + space,
            });
            at += space + 1;
        } else {
            lines.push(VisualLine {
                start: origin + at,
                end: origin + at + cols,
            });
            at += cols;
        }
    }
}
