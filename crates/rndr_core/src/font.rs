//! Glyph metrics for one face. Shaping goes through [`crate::text::TextEngine`];
//! this trait is the atlas the builtin engine asks for advances, kerning, and
//! coverage. A later face implements the same trait. Core still has no GPU types.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use crate::caret::{EditField, Field};
use crate::style::{Color, Label, CELL_H, CELL_W};
use crate::world::{Component, World};

impl Component for TextLayout {}
impl Component for Spans {}

/// One styled byte range on a [`Label`].
///
/// `color` tints the glyphs. `weight` overrides the run's font weight when set
/// (`None` keeps the style weight). `highlight` paints a bar behind the range.
/// `face` is reserved for a second atlas.
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpan {
    pub start: usize,
    pub end: usize,
    pub color: Color,
    pub face: u8,
    /// `None` inherits the style weight.
    pub weight: Option<u16>,
    /// Background behind this range. `None` draws no bar.
    pub highlight: Option<Color>,
}

impl TextSpan {
    pub fn new(start: usize, end: usize, color: Color) -> Self {
        Self {
            start,
            end,
            color,
            face: 0,
            weight: None,
            highlight: None,
        }
    }

    pub fn weight(mut self, weight: u16) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn highlight(mut self, color: Color) -> Self {
        self.highlight = Some(color);
        self
    }
}

/// Styled ranges painted over a label. Absent means one theme-colored run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spans {
    pub runs: Vec<TextSpan>,
}

/// Horizontal advance and painted box for one scalar, in logical points.
pub trait Font: Send + Sync {
    fn line_height(&self) -> f32;
    /// Width the pen moves after `ch`. A newline advances 0.
    fn advance(&self, ch: char) -> f32;
    /// Extra pen movement between `left` and `right`, in points. Zero when the
    /// face has no pair for those scalars.
    fn kerning(&self, _left: char, _right: char) -> f32 {
        0.0
    }
    /// Painted quad for `ch`. `None` still consumes [`Font::advance`] (a space).
    fn glyph_box(&self, ch: char) -> Option<GlyphBox>;
    /// This face can draw `ch` (including space and newline).
    fn covers(&self, ch: char) -> bool {
        ch == '\n' || ch == ' ' || self.glyph_box(ch).is_some()
    }
    /// Family name used by [`crate::text::TextEngine`] fallback lists.
    fn family_name(&self) -> &'static str {
        "bitmap"
    }
    /// Design size in points. A style size other than this scales advances.
    fn em_size(&self) -> f32 {
        16.0
    }
    /// Coverage rect in this face's atlas. The bitmap grid is the default.
    fn glyph_uv(&self, ch: char) -> [f32; 4] {
        bitmap_glyph_uv(ch)
    }
    /// White texel used by solid quads. Matches the bitmap atlas unless overridden.
    fn solid_uv(&self) -> [f32; 4] {
        bitmap_solid_uv()
    }
    /// Coverage atlas (one byte per pixel). `None` keeps the 8×8 bitmap atlas.
    fn atlas(&self) -> Option<Arc<FontAtlas>> {
        None
    }
    /// Color atlas (RGBA) for emoji and other color glyphs. The bitmap and Latin
    /// faces have none.
    fn color_atlas(&self) -> Option<Arc<ColorAtlas>> {
        None
    }
}

/// R8 coverage atlas owned by a face.
pub struct FontAtlas {
    pub width: u32,
    pub height: u32,
    pub r8: Arc<[u8]>,
}

/// RGBA atlas for emoji and other color glyphs. Empty until a face provides one.
pub struct ColorAtlas {
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
}

fn bitmap_glyph_uv(ch: char) -> [f32; 4] {
    let code = if ch.is_ascii() && (32..128).contains(&(ch as u8)) {
        ch as u32
    } else {
        b'?' as u32
    };
    let idx = code - 32;
    let col = idx % 16;
    let row = idx / 16;
    let u0 = col as f32 * 8.0 / 128.0;
    let v0 = row as f32 * 8.0 / 56.0;
    let u1 = (col + 1) as f32 * 8.0 / 128.0;
    let v1 = (row + 1) as f32 * 8.0 / 56.0;
    [u0, v0, u1, v1]
}

fn bitmap_solid_uv() -> [f32; 4] {
    let v = (56.0 - 0.5) / 56.0;
    [0.5 / 128.0, v, 0.5 / 128.0, v]
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

    fn family_name(&self) -> &'static str {
        "bitmap"
    }

    fn em_size(&self) -> f32 {
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
        Self(shared_bitmap())
    }
}

pub(crate) fn shared_bitmap() -> Arc<dyn Font> {
    static BITMAP: OnceLock<Arc<dyn Font>> = OnceLock::new();
    Arc::clone(BITMAP.get_or_init(|| Arc::new(BitmapFont)))
}

pub fn text_face(world: &World) -> Arc<dyn Font> {
    world
        .resource::<TextFace>()
        .map(|face| Arc::clone(&face.0))
        .unwrap_or_else(shared_bitmap)
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
    pub uv: [f32; 4],
    /// `None` paints with the label color, or the theme text color when the label
    /// has none.
    pub color: Option<Color>,
    pub face: u8,
    /// Resolved weight. A future engine picks a master from this.
    pub weight: u16,
    /// `0` samples the coverage atlas. `1` samples the color atlas.
    pub atlas: u8,
}

/// Background behind a styled range, relative to the text origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextHighlight {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: Color,
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
///
/// `cache_key` records the font, text, wrap width, spans, and IME preview
/// this run was built from. A matching key means the glyphs are still current.
#[derive(Clone, Debug)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub lines: Vec<TextLine>,
    pub highlights: Vec<TextHighlight>,
    pub width: f32,
    pub height: f32,
    pub(crate) cache_key: u64,
    /// Caret inside an IME preedit, as a byte index in the shaped string.
    /// `None` keeps the field caret.
    pub ime_cursor: Option<usize>,
    /// Byte range of the uncommitted preedit in the shaped string.
    pub ime_range: Option<(usize, usize)>,
}

impl TextLayout {
    /// Glyphs and lines for one shaped string. Highlight rects start empty;
    /// the framework records the cache key after shaping.
    pub fn shaped(glyphs: Vec<PlacedGlyph>, lines: Vec<TextLine>, width: f32, height: f32) -> Self {
        Self {
            glyphs,
            lines,
            highlights: Vec::new(),
            width,
            height,
            cache_key: 0,
            ime_cursor: None,
            ime_range: None,
        }
    }
}

impl PartialEq for TextLayout {
    fn eq(&self, other: &Self) -> bool {
        self.glyphs == other.glyphs
            && self.lines == other.lines
            && self.highlights == other.highlights
            && self.width == other.width
            && self.height == other.height
            && self.ime_cursor == other.ime_cursor
            && self.ime_range == other.ime_range
    }
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
    layout_text_spans(font, text, max_width, &[])
}

/// Like [`layout_text`], tinting each glyph from the span that covers its byte.
pub fn layout_text_spans(
    font: &dyn Font,
    text: &str,
    max_width: f32,
    spans: &[TextSpan],
) -> TextLayout {
    layout_text_weighted(font, text, max_width, spans, 400)
}

/// [`layout_text_spans`] with `default_weight` on bytes no span overrides.
pub(crate) fn layout_text_weighted(
    font: &dyn Font,
    text: &str,
    max_width: f32,
    spans: &[TextSpan],
    default_weight: u16,
) -> TextLayout {
    let ranges = wrap_ranges(font, text, max_width);
    let mut glyphs = Vec::new();
    let mut lines = Vec::new();
    let mut y = 0.0f32;
    let h = font.line_height().max(0.0);
    let mut width = 0.0f32;
    for (start, end) in ranges {
        let mut edges = vec![(start, 0.0)];
        let mut x = 0.0f32;
        let mut prev = None;
        for (i, ch) in text[start..end].char_indices() {
            let abs = start + i;
            if let Some(prev) = prev {
                x += font.kerning(prev, ch);
            }
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
                    uv: font.glyph_uv(ch),
                    color: span_color(spans, abs),
                    face: span_face(spans, abs),
                    weight: span_weight(spans, abs).unwrap_or(default_weight),
                    atlas: 0,
                });
            }
            x += adv;
            edges.push((abs + ch.len_utf8(), x));
            prev = Some(ch);
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
    let mut layout = TextLayout {
        glyphs,
        lines,
        highlights: Vec::new(),
        width,
        height: y,
        cache_key: 0,
        ime_cursor: None,
        ime_range: None,
    };
    for span in spans {
        let Some(color) = span.highlight else {
            continue;
        };
        for selection in layout.selection_boxes(span.start, span.end) {
            layout.highlights.push(TextHighlight {
                x: selection.x,
                y: selection.y,
                w: selection.w,
                h: selection.h,
                color,
            });
        }
    }
    layout
}

/// Width and line count for a wrap, without glyph quads or per-line edge buffers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextMeasure {
    pub width: f32,
    pub height: f32,
    pub line_count: usize,
}

/// Same wrap as [`layout_text`], keeping only the pen width and the line count.
pub fn measure_text(font: &dyn Font, text: &str, max_width: f32) -> TextMeasure {
    let ranges = wrap_ranges(font, text, max_width);
    let mut width = 0.0f32;
    for (start, end) in &ranges {
        width = width.max(measure(font, &text[*start..*end]));
    }
    let line_count = ranges.len().max(1);
    let h = font.line_height().max(0.0);
    TextMeasure {
        width,
        height: line_count as f32 * h,
        line_count,
    }
}

pub(crate) fn layout_cache_key(
    font: &dyn Font,
    text: &str,
    max_width: f32,
    spans: &[TextSpan],
    preview: &str,
    preview_at: usize,
) -> u64 {
    let mut hasher = DefaultHasher::new();
    let font_id = font as *const dyn Font as *const () as usize;
    font_id.hash(&mut hasher);
    text.hash(&mut hasher);
    max_width.to_bits().hash(&mut hasher);
    spans.len().hash(&mut hasher);
    for span in spans {
        span.start.hash(&mut hasher);
        span.end.hash(&mut hasher);
        span.color.r.to_bits().hash(&mut hasher);
        span.color.g.to_bits().hash(&mut hasher);
        span.color.b.to_bits().hash(&mut hasher);
        span.color.a.to_bits().hash(&mut hasher);
        span.face.hash(&mut hasher);
        span.weight.hash(&mut hasher);
        match span.highlight {
            Some(color) => {
                1u8.hash(&mut hasher);
                color.r.to_bits().hash(&mut hasher);
                color.g.to_bits().hash(&mut hasher);
                color.b.to_bits().hash(&mut hasher);
                color.a.to_bits().hash(&mut hasher);
            }
            None => 0u8.hash(&mut hasher),
        }
    }
    if !preview.is_empty() {
        preview.hash(&mut hasher);
        preview_at.hash(&mut hasher);
    }
    hasher.finish()
}

impl TextLayout {
    /// Byte index under a point relative to the text origin.
    pub fn index_at(&self, x: f32, y: f32) -> usize {
        if self.lines.is_empty() {
            return 0;
        }
        let mut chosen = self.lines.len() - 1;
        for (i, line) in self.lines.iter().enumerate() {
            let h = if line.h > 0.0 { line.h } else { 0.001 };
            if y < line.y + h {
                chosen = i;
                break;
            }
        }
        self.index_on_line(chosen, x)
    }

    /// Byte index on `line` nearest to pen `x`.
    ///
    /// Edges are caret stops, not a left-to-right walk, so a right-to-left run
    /// still hits the cluster under the pointer. A tie goes to the stop with
    /// the greater x, which matches the old midpoint test on a left-to-right line.
    pub fn index_on_line(&self, line: usize, x: f32) -> usize {
        let Some(span) = self.lines.get(line.min(self.lines.len().saturating_sub(1))) else {
            return 0;
        };
        let Some((mut best_i, mut best_x)) = span.edges.first().copied() else {
            return span.start;
        };
        let mut best_d = (best_x - x).abs();
        for &(i, ex) in &span.edges[1..] {
            let d = (ex - x).abs();
            let closer = d < best_d;
            let tie =
                (d - best_d).abs() <= f32::EPSILON && (ex > best_x || (ex == best_x && i > best_i));
            if closer || tie {
                best_d = d;
                best_i = i;
                best_x = ex;
            }
        }
        best_i
    }

    /// Pen position of `index`, relative to the text origin.
    pub fn caret_xy(&self, index: usize) -> (f32, f32) {
        if self.lines.is_empty() {
            return (0.0, 0.0);
        }
        let (line, col) = line_col(&self.line_ranges(), index);
        let span = &self.lines[line];
        let byte = span.start + col;
        let mut best: Option<(usize, f32)> = None;
        for &(i, ex) in &span.edges {
            if i <= byte && best.is_none_or(|(at, _)| i >= at) {
                best = Some((i, ex));
            }
        }
        (best.map(|(_, x)| x).unwrap_or(span.width), span.y)
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
            let mut spans = Vec::new();
            let mut edges = line.edges.clone();
            edges.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
            for pair in edges.windows(2) {
                let (i0, x0) = pair[0];
                let (i1, x1) = pair[1];
                if i1 <= start || i0 >= end {
                    continue;
                }
                spans.push((x0.min(x1), x0.max(x1)));
            }
            spans.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut merged: Vec<(f32, f32)> = Vec::new();
            for (left, right) in spans {
                let touches = merged.last().is_some_and(|last| left <= last.1 + 0.01);
                if touches {
                    let last = merged.last_mut().expect("span just checked");
                    last.1 = last.1.max(right);
                } else {
                    merged.push((left, right));
                }
            }
            for (left, right) in merged {
                boxes.push(SelectionBox {
                    x: left,
                    y: line.y,
                    w: (right - left).max(0.0),
                    h: line.h,
                });
            }
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

fn span_color(spans: &[TextSpan], index: usize) -> Option<Color> {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .map(|span| span.color)
}

fn span_face(spans: &[TextSpan], index: usize) -> u8 {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .map(|span| span.face)
        .unwrap_or(0)
}

fn span_weight(spans: &[TextSpan], index: usize) -> Option<u16> {
    spans
        .iter()
        .find(|span| index >= span.start && index < span.end)
        .and_then(|span| span.weight)
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
///
/// A label whose text, wrap width, spans, face, and IME preview are unchanged
/// keeps the layout it already has.
pub fn apply_text_layout(world: &mut World) {
    let style = crate::text::style_of(world);
    let engine = crate::text::text_engine(world);
    let font = text_face(world);
    let unit = engine.unit_advance(&style);
    let labels = world.entities_with::<Label>();
    for entity in labels {
        let wrap = world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == EditField::Body);
        let max_width = if wrap {
            world
                .get::<crate::layout::Rect>(entity)
                .map(|rect| field_text_width(rect.w, unit))
                .unwrap_or(f32::INFINITY)
        } else {
            f32::INFINITY
        };
        let Some(label) = world.get::<crate::style::Label>(entity) else {
            continue;
        };
        let preview = world
            .get::<crate::field::ImePreview>(entity)
            .map(|preview| preview.text.clone())
            .unwrap_or_default();
        let at = world
            .get::<crate::caret::Caret>(entity)
            .map(|caret| caret.index.min(label.text.len()))
            .unwrap_or(label.text.len());
        let spans = world
            .get::<Spans>(entity)
            .map(|spans| spans.runs.clone())
            .unwrap_or_default();
        let source = label.text.clone();
        let cursor = world
            .get::<crate::field::ImePreview>(entity)
            .and_then(|preview| preview.cursor);
        let mut key = layout_cache_key(font.as_ref(), &source, max_width, &spans, &preview, at);
        if !preview.is_empty() {
            if let Some((start, end)) = cursor {
                key ^= (start as u64)
                    .wrapping_shl(1)
                    .wrapping_add((end as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15));
            }
        }
        let insert_at = floor_preview(at, &source);
        let mut text = source;
        if !preview.is_empty() {
            text.insert_str(insert_at, &preview);
        }
        // Always shape through the engine so the line cache records this frame.
        let mut layout = engine.layout(&text, &style, &spans, max_width);
        let epoch = engine.cache_epoch();
        if epoch != 0 {
            key ^= epoch.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
        if !preview.is_empty() {
            let end = cursor
                .map(|(_, end)| end.min(preview.len()))
                .unwrap_or(preview.len());
            layout.ime_range = Some((insert_at, insert_at + preview.len()));
            layout.ime_cursor = Some(insert_at + end);
        }
        if world
            .get::<TextLayout>(entity)
            .is_some_and(|stored| stored.cache_key == key)
        {
            continue;
        }
        layout.cache_key = key;
        world.insert(entity, layout);
    }
}

fn floor_preview(index: usize, text: &str) -> usize {
    let mut i = index.min(text.len());
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub(crate) fn wrap_ranges(font: &dyn Font, text: &str, max_width: f32) -> Vec<(usize, usize)> {
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
        let mut prev = None;
        for (i, ch) in rest.char_indices() {
            let adv = step(font, prev, ch);
            if accepted > 0 && pen + adv > max_width {
                break;
            }
            pen += adv;
            if ch == ' ' && i > 0 {
                last_space = Some(i);
            }
            accepted = i + ch.len_utf8();
            prev = Some(ch);
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
    let mut width = 0.0f32;
    let mut prev = None;
    for ch in text.chars() {
        width += step(font, prev, ch);
        prev = Some(ch);
    }
    width
}

fn step(font: &dyn Font, prev: Option<char>, ch: char) -> f32 {
    let kern = prev.map(|prev| font.kerning(prev, ch)).unwrap_or(0.0);
    kern + font.advance(ch)
}

#[cfg(test)]
mod tests {
    use super::{TextLayout, TextLine};

    fn rtl_line() -> TextLayout {
        // Byte 0 sits on the right, byte 2 in the middle, byte 4 on the left.
        TextLayout::shaped(
            Vec::new(),
            vec![TextLine {
                start: 0,
                end: 4,
                y: 0.0,
                h: 16.0,
                width: 20.0,
                edges: vec![(0, 20.0), (2, 10.0), (4, 0.0)],
            }],
            20.0,
            16.0,
        )
    }

    #[test]
    fn rtl_selection_follows_visual_edges() {
        let layout = rtl_line();
        assert!((layout.caret_xy(0).0 - 20.0).abs() < 0.01);
        assert!((layout.caret_xy(2).0 - 10.0).abs() < 0.01);
        assert!((layout.caret_xy(4).0 - 0.0).abs() < 0.01);

        let first = layout.selection_boxes(0, 2);
        assert_eq!(first.len(), 1);
        assert!((first[0].x - 10.0).abs() < 0.01, "x {}", first[0].x);
        assert!((first[0].w - 10.0).abs() < 0.01, "w {}", first[0].w);

        let second = layout.selection_boxes(2, 4);
        assert_eq!(second.len(), 1);
        assert!(second[0].x.abs() < 0.01);
        assert!((second[0].w - 10.0).abs() < 0.01);

        let all = layout.selection_boxes(0, 4);
        assert_eq!(all.len(), 1);
        assert!(all[0].x.abs() < 0.01);
        assert!((all[0].w - 20.0).abs() < 0.01);
    }
}
