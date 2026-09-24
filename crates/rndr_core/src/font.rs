//! Text measurement. The bitmap face is the first backend.
//!
//! Caret, wrap, and paint ask a [`Font`] for advances and glyph boxes.
//! [`layout_text`] turns that into a [`TextLayout`] of positioned glyphs.
//! A later face implements the same trait. Core still has no GPU types.

use std::sync::Arc;

use crate::caret::{EditField, Field};
use crate::style::{Label, CELL_H, CELL_W};
use crate::world::{Component, World};

impl Component for TextLayout {}

/// Horizontal advance and painted box for one scalar, in logical points.
pub trait Font: Send + Sync {
    fn line_height(&self) -> f32;
    /// Width the pen moves after `ch`. A newline advances 0.
    fn advance(&self, ch: char) -> f32;
    /// Painted quad for `ch`. `None` still consumes [`Font::advance`] (a space).
    fn glyph_box(&self, ch: char) -> Option<GlyphBox>;
}

/// Painted size of one glyph. Offsets are from the pen, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlyphBox {
    pub w: f32,
    pub h: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
}

/// 8×8 atlas drawn at [`CELL_W`] by [`CELL_H`]. Every scalar except a newline
/// advances one cell. Only printable ASCII other than space paints a quad.
#[derive(Clone, Copy, Debug, Default)]
pub struct BitmapFont;

impl Font for BitmapFont {
    fn line_height(&self) -> f32 {
        CELL_H
    }

    fn advance(&self, ch: char) -> f32 {
        if ch == '\n' {
            0.0
        } else {
            CELL_W
        }
    }

    fn glyph_box(&self, ch: char) -> Option<GlyphBox> {
        if !ch.is_ascii() {
            return None;
        }
        let code = ch as u8;
        if (0x21..=0x7e).contains(&code) {
            Some(GlyphBox {
                w: CELL_W,
                h: CELL_H,
                bearing_x: 0.0,
                bearing_y: 0.0,
            })
        } else {
            None
        }
    }
}

/// The face layout and paint measure with. Absent means [`BitmapFont`].
pub struct TextFace(pub Arc<dyn Font>);

impl TextFace {
    pub fn bitmap() -> Self {
        Self(Arc::new(BitmapFont))
    }
}

pub fn text_face(world: &World) -> Arc<dyn Font> {
    world
        .resource::<TextFace>()
        .map(|face| Arc::clone(&face.0))
        .unwrap_or_else(|| Arc::new(BitmapFont))
}

/// One painted glyph. `x` and `y` are relative to the text origin, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub start: usize,
    pub end: usize,
    pub ch: char,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// One visual line. `edges` is `(byte index, pen x)` from the line start through
/// the position after the last scalar.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLine {
    pub start: usize,
    pub end: usize,
    pub y: f32,
    pub h: f32,
    pub width: f32,
    pub edges: Vec<(usize, f32)>,
}

/// Glyph run for one string wrapped to `max_width` points.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub lines: Vec<TextLine>,
    pub width: f32,
    pub height: f32,
}

/// Highlight rectangle relative to the text origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectionBox {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Wrap `text` to `max_width` points and place each painted glyph.
/// `f32::INFINITY` keeps hard breaks only.
pub fn layout_text(font: &dyn Font, text: &str, max_width: f32) -> TextLayout {
    let ranges = wrap_ranges(font, text, max_width);
    let mut glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut y = 0.0f32;
    let h = font.line_height().max(0.0);
    let mut width = 0.0f32;
    for (start, end) in ranges {
        let mut edges = vec![(start, 0.0)];
        let mut x = 0.0f32;
        for (i, ch) in text[start..end].char_indices() {
            let abs = start + i;
            let adv = font.advance(ch);
            if let Some(glyph) = font.glyph_box(ch) {
                glyphs.push(PlacedGlyph {
                    start: abs,
                    end: abs + ch.len_utf8(),
                    ch,
                    x: x + glyph.bearing_x,
                    y: y + glyph.bearing_y,
                    w: glyph.w,
                    h: glyph.h,
                });
            }
            x += adv;
            edges.push((abs + ch.len_utf8(), x));
        }
        width = width.max(x);
        lines.push(TextLine {
            start,
            end,
            y,
            h,
            width: x,
            edges,
        });
        y += h;
    }
    if lines.is_empty() {
        lines.push(TextLine {
            start: 0,
            end: 0,
            y: 0.0,
            h,
            width: 0.0,
            edges: vec![(0, 0.0)],
        });
        y = h;
    }
    TextLayout {
        glyphs,
        lines,
        width,
        height: y,
    }
}

impl TextLayout {
    /// Byte index under a point relative to the text origin.
    pub fn index_at(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let h = self.lines[0].h.max(0.001);
        let line = (y / h).floor().clamp(0.0, (self.lines.len() - 1) as f32) as usize;
        self.index_on_line(line, x)
    }

    /// Byte index on `line` nearest to pen `x`.
    pub fn index_on_line(&self, line: usize, x: f32) -> usize {
        let Some(span) = self.lines.get(line.min(self.lines.len().saturating_sub(1))) else {
            return 0;
        };
        if span.edges.len() < 2 {
            return span.start;
        }
        for pair in span.edges.windows(2) {
            let (i0, x0) = pair[0];
            let (_i1, x1) = pair[1];
            let mid = (x0 + x1) * 0.5;
            if x < mid {
                return i0;
            }
        }
        span.end
    }

    /// Pen position of `index`, relative to the text origin.
    pub fn caret_xy(&self, index: usize) -> (f32, f32) {
        if self.lines.is_empty() {
            return (0.0, 0.0);
        }
        let (line, col) = line_col(&self.line_ranges(), index);
        let span = &self.lines[line];
        let byte = span.start + col;
        let x = span
            .edges
            .iter()
            .find(|(i, _)| *i == byte)
            .map(|(_, x)| *x)
            .unwrap_or(span.width);
        (x, span.y)
    }

    /// Selected runs in paint order. An empty visual line inside the range is 2 points wide.
    pub fn selection_boxes(&self, anchor: usize, index: usize) -> Vec<SelectionBox> {
        let start = anchor.min(index);
        let end = anchor.max(index);
        if start == end {
            return Vec::new();
        }
        let mut boxes = Vec::new();
        for line in &self.lines {
            if line.start == line.end {
                if line.start >= start && line.start < end {
                    boxes.push(SelectionBox {
                        x: 0.0,
                        y: line.y,
                        w: 2.0,
                        h: line.h,
                    });
                }
                continue;
            }
            if line.end <= start || line.start >= end {
                continue;
            }
            let from = start.max(line.start);
            let to = end.min(line.end);
            if from >= to {
                continue;
            }
            let x0 = self.caret_xy(from).0;
            let x1 = edge_on(line, to);
            boxes.push(SelectionBox {
                x: x0,
                y: line.y,
                w: (x1 - x0).max(0.0),
                h: line.h,
            });
        }
        boxes
    }

    fn line_ranges(&self) -> Vec<(usize, usize)> {
        self.lines
            .iter()
            .map(|line| (line.start, line.end))
            .collect()
    }
}

fn edge_on(line: &TextLine, byte: usize) -> f32 {
    line.edges
        .iter()
        .find(|(i, _)| *i == byte)
        .map(|(_, x)| *x)
        .unwrap_or(line.width)
}

/// `(line, byte column)` for `index`. Column is a byte offset on that line.
pub fn line_col(lines: &[(usize, usize)], index: usize) -> (usize, usize) {
    if lines.is_empty() {
        return (0, 0);
    }
    for (i, (start, end)) in lines.iter().copied().enumerate() {
        let last = i + 1 == lines.len();
        if index < start {
            return (i, 0);
        }
        if index < end || (index == end && (last || lines[i + 1].0 != index)) {
            return (i, index - start);
        }
    }
    let last = lines.len() - 1;
    (last, lines[last].1.saturating_sub(lines[last].0))
}

/// Inner text width for a field of `field_width` points. At least one sample advance.
pub fn field_text_width(field_width: f32, unit: f32) -> f32 {
    let unit = if unit.is_finite() && unit > 0.0 {
        unit
    } else {
        1.0
    };
    (field_width - 2.0 * crate::style::TEXT_ORIGIN_X).max(unit)
}

/// Write a [`TextLayout`] onto every [`Label`] after rects are final.
pub fn apply_text_layout(world: &mut World) {
    let font = text_face(world);
    let labels = world.entities_with::<Label>();
    for entity in labels {
        let text = world
            .get::<Label>(entity)
            .map(|label| label.text.clone())
            .unwrap_or_default();
        let wrap = world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == EditField::Body);
        let max_width = if wrap {
            world
                .get::<crate::layout::Rect>(entity)
                .map(|rect| field_text_width(rect.w, font.advance('0')))
                .unwrap_or(f32::INFINITY)
        } else {
            f32::INFINITY
        };
        let layout = layout_text(font.as_ref(), &text, max_width);
        world.insert(entity, layout);
    }
}

fn wrap_ranges(font: &dyn Font, text: &str, max_width: f32) -> Vec<(usize, usize)> {
    if text.is_empty() {
        return vec![(0, 0)];
    }
    let mut lines = Vec::new();
    let mut offset = 0usize;
    for (i, part) in text.split('\n').enumerate() {
        if i > 0 {
            offset += 1;
        }
        wrap_part(font, part, offset, max_width, &mut lines);
        offset += part.len();
    }
    if lines.is_empty() {
        lines.push((0, 0));
    }
    lines
}

fn wrap_part(
    font: &dyn Font,
    part: &str,
    origin: usize,
    max_width: f32,
    lines: &mut Vec<(usize, usize)>,
) {
    if part.is_empty() || max_width.is_infinite() {
        lines.push((origin, origin + part.len()));
        return;
    }
    let mut at = 0usize;
    while at < part.len() {
        let rest = &part[at..];
        if measure(font, rest) <= max_width {
            lines.push((origin + at, origin + part.len()));
            return;
        }
        let mut pen = 0.0f32;
        let mut accepted = 0usize;
        let mut last_space: Option<usize> = None;
        for (i, ch) in rest.char_indices() {
            let adv = font.advance(ch);
            if accepted > 0 && pen + adv > max_width {
                break;
            }
            pen += adv;
            if ch == ' ' && i > 0 {
                last_space = Some(i);
            }
            accepted = i + ch.len_utf8();
            if pen > max_width {
                break;
            }
        }
        if accepted == 0 {
            return;
        }
        if let Some(space) = last_space {
            lines.push((origin + at, origin + at + space));
            at += space + ' '.len_utf8();
        } else {
            lines.push((origin + at, origin + at + accepted));
            at += accepted;
        }
    }
}

fn measure(font: &dyn Font, text: &str) -> f32 {
    text.chars().map(|ch| font.advance(ch)).sum()
}
