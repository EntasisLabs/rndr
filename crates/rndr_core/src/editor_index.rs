//! Per-view paragraph height index.
#![allow(clippy::too_many_arguments)]
//!
//! Scroll position, anchors, and tail edits use paragraph heights. Caret edges
//! are shaped for the one paragraph that needs them, not stored for every row.
use super::geometry::{self, Cache, Work};
use crate::{TextEngine, TextLayout, TextSpan, TextStyle};
use std::ops::Range;

#[derive(Clone)]
struct Para {
    start: usize,
    end: usize,
    ink: f32,
    gap: f32,
    span_fp: u64,
    /// Absolute start, exclusive end, and height of each unfolded visual line.
    lines: Box<[LineRec]>,
    /// Index of the first line that is not inside a fold. `lines.len()` if all hidden.
    shown: Box<[u16]>,
    y: f32,
    vis: u32,
    visible_h: f32,
}

#[derive(Clone, Copy)]
struct LineRec {
    start: usize,
    end: usize,
    h: f32,
}

#[derive(Clone)]
pub(super) struct LineInfo {
    pub start: usize,
    pub end: usize,
    pub y: f32,
    pub h: f32,
}

#[derive(Clone)]
pub(super) struct RowIndex {
    paras: Vec<Para>,
    pub visual_lines: u32,
    pub max_top: f32,
}

pub(super) struct Built {
    pub index: RowIndex,
    pub cache: Cache,
    pub work: Work,
}

fn local_spans(spans: &[TextSpan], start: usize, end: usize) -> Vec<TextSpan> {
    spans
        .iter()
        .filter_map(|span| {
            let from = span.start.max(start);
            let to = span.end.min(end);
            if from >= to {
                return None;
            }
            let mut span = span.clone();
            span.start = from - start;
            span.end = to - start;
            Some(span)
        })
        .collect()
}

fn span_fp(spans: &[TextSpan]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    spans.len().hash(&mut hasher);
    for span in spans {
        span.start.hash(&mut hasher);
        span.end.hash(&mut hasher);
        span.weight.hash(&mut hasher);
        span.italic.hash(&mut hasher);
        span.underline.hash(&mut hasher);
        span.family.hash(&mut hasher);
        span.color.r.to_bits().hash(&mut hasher);
        span.color.g.to_bits().hash(&mut hasher);
        span.color.b.to_bits().hash(&mut hasher);
        span.color.a.to_bits().hash(&mut hasher);
        span.highlight.is_some().hash(&mut hasher);
    }
    hasher.finish()
}

fn paragraph_ranges(text: &str, trailing_empty: bool) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut base = 0usize;
    for part in text.split_inclusive('\n') {
        let end = base + part.len();
        ranges.push((base, end));
        base = end;
    }
    if trailing_empty {
        ranges.push((text.len(), text.len()));
    }
    if ranges.is_empty() {
        ranges.push((0, 0));
    }
    ranges
}

fn hidden(folds: &[Range<usize>], at: usize) -> bool {
    let i = folds.partition_point(|range| range.end <= at);
    folds.get(i).is_some_and(|range| range.contains(&at))
}

fn para_from_layout(start: usize, end: usize, gap: f32, span_fp: u64, layout: &TextLayout) -> Para {
    let lines: Vec<LineRec> = if layout.lines.is_empty() {
        vec![LineRec {
            start,
            end,
            h: layout.height.max(1.0),
        }]
    } else {
        layout
            .lines
            .iter()
            .map(|line| LineRec {
                start: start + line.start,
                end: start + line.end,
                h: line.h,
            })
            .collect()
    };
    Para {
        start,
        end,
        ink: layout.width,
        gap,
        span_fp,
        lines: lines.into_boxed_slice(),
        shown: Box::new([]),
        y: 0.0,
        vis: 0,
        visible_h: 0.0,
    }
}

fn shape_range(
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    start: usize,
    end: usize,
    gap: f32,
    spans: &[TextSpan],
    previous: Option<&Cache>,
    cache: &mut Cache,
    work: &mut Work,
) -> Para {
    let part = &text[start..end];
    let local = local_spans(spans, start, end);
    let fp = span_fp(&local);
    let (paragraph, fresh) =
        geometry::shape_piece(engine, font, style, width, part, &local, previous, cache);
    if fresh {
        work.paragraphs += 1;
        work.bytes += part.len() as u64;
    } else {
        work.reused += 1;
    }
    para_from_layout(start, end, gap, fp, paragraph.layout())
}

fn gap_of(style: &TextStyle, start: usize) -> f32 {
    if start == 0 {
        0.0
    } else {
        style.paragraph_spacing
    }
}

fn recompute(paras: &mut [Para], folds: &[Range<usize>]) -> (u32, f32) {
    let mut y = 0.0f32;
    let mut vis = 0u32;
    let mut max_top = 0.0f32;
    let pack = !folds.is_empty();
    for para in paras.iter_mut() {
        // Reuse the existing line list when visibility did not change. A tail
        // edit must not allocate a new box for every untouched paragraph.
        if folds.is_empty() {
            if para.shown.len() != para.lines.len() {
                para.shown = (0..para.lines.len() as u16)
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
            }
        } else {
            let mut shown = Vec::with_capacity(para.lines.len());
            for (index, line) in para.lines.iter().enumerate() {
                if !hidden(folds, line.start) {
                    shown.push(index as u16);
                }
            }
            if para.shown.as_ref() != shown.as_slice() {
                para.shown = shown.into_boxed_slice();
            }
        }
        if para.shown.is_empty() {
            para.y = y;
            para.vis = vis;
            para.visible_h = 0.0;
            continue;
        }
        if !pack && para.gap != 0.0 {
            y += para.gap;
        }
        para.y = y;
        para.vis = vis;
        let mut height = 0.0f32;
        for (n, &index) in para.shown.iter().enumerate() {
            if n > 0 || vis > 0 {
                max_top = y + height;
            } else {
                max_top = y;
            }
            height += para.lines[index as usize].h;
        }
        para.visible_h = height;
        y += height;
        vis += para.shown.len() as u32;
    }
    if vis == 0 {
        max_top = 0.0;
    }
    (vis, max_top)
}

fn shift_para(para: &mut Para, delta: isize) {
    let shift = |at: usize| (at as isize + delta).max(0) as usize;
    para.start = shift(para.start);
    para.end = shift(para.end);
    for line in para.lines.iter_mut() {
        line.start = shift(line.start);
        line.end = shift(line.end);
    }
}

impl RowIndex {
    pub(super) fn line_count(&self) -> usize {
        self.visual_lines as usize
    }

    pub(super) fn line(&self, index: usize) -> LineInfo {
        if self.visual_lines == 0 || self.paras.is_empty() {
            return LineInfo {
                start: 0,
                end: 0,
                y: 0.0,
                h: 1.0,
            };
        }
        let index = index.min(self.line_count() - 1);
        let para_i = self
            .paras
            .partition_point(|para| para.vis as usize <= index)
            .saturating_sub(1)
            .min(self.paras.len() - 1);
        let para = &self.paras[para_i];
        let local = index - para.vis as usize;
        let local = local.min(para.shown.len().saturating_sub(1));
        let raw = para.shown[local] as usize;
        let rec = para.lines[raw];
        let start = rec.start;
        let h = rec.h;
        let end = rec.end;
        let mut y = para.y;
        for &prior in &para.shown[..local] {
            y += para.lines[prior as usize].h;
        }
        LineInfo { start, end, y, h }
    }

    pub(super) fn row_at_y(&self, y: f32) -> usize {
        if self.visual_lines == 0 {
            return 0;
        }
        // Paragraph tops are non-decreasing. Hidden paragraphs share the next
        // visible top, so the last top at or below `y` is the visible one.
        let mut para_i = self
            .paras
            .partition_point(|para| para.y <= y)
            .saturating_sub(1);
        while para_i > 0 && self.paras[para_i].visible_h <= 0.0 {
            para_i -= 1;
        }
        let para = &self.paras[para_i];
        let mut top = para.y;
        let row = para.vis as usize;
        for (n, &raw) in para.shown.iter().enumerate() {
            let h = para.lines[raw as usize].h;
            if y < top + h || n + 1 == para.shown.len() {
                return row + n;
            }
            top += h;
        }
        row
    }

    pub(super) fn row_at_byte(&self, byte: usize) -> usize {
        if self.paras.is_empty() || self.visual_lines == 0 {
            return 0;
        }
        let para_i = self
            .paras
            .partition_point(|para| para.start <= byte)
            .saturating_sub(1)
            .min(self.paras.len() - 1);
        let para = &self.paras[para_i];
        let mut chosen = para.vis as usize;
        for (n, &raw) in para.shown.iter().enumerate() {
            let start = para.lines[raw as usize].start;
            let end = para.lines[raw as usize].end;
            let next_keeps_boundary = n + 1 < para.shown.len()
                && para.lines[para.shown[n + 1] as usize].start == end
                && end == byte;
            if byte >= start
                && (byte < end
                    || (byte == end && !next_keeps_boundary && n + 1 == para.shown.len()))
            {
                // Shared edge belongs to the following line.
                if byte == end
                    && n + 1 < para.shown.len()
                    && para.lines[para.shown[n + 1] as usize].start == end
                {
                    continue;
                }
                return para.vis as usize + n;
            }
            if byte >= end && n + 1 < para.shown.len() {
                continue;
            }
            chosen = para.vis as usize + n;
        }
        chosen.min(self.line_count().saturating_sub(1))
    }

    pub(super) fn top_at_most(&self, y: f32) -> f32 {
        if self.visual_lines == 0 {
            return 0.0;
        }
        let line = self.line(self.row_at_y(y));
        if line.y <= y {
            line.y
        } else {
            0.0
        }
    }

    pub(super) fn line_for_byte(&self, byte: usize) -> LineInfo {
        self.line(self.row_at_byte(byte))
    }

    pub(super) fn para_for_row(&self, row: usize) -> usize {
        if self.paras.is_empty() {
            return 0;
        }
        self.paras
            .partition_point(|para| para.vis as usize <= row)
            .saturating_sub(1)
            .min(self.paras.len() - 1)
    }

    pub(super) fn para_range(&self, para: usize) -> (usize, usize) {
        let para = &self.paras[para.min(self.paras.len().saturating_sub(1))];
        (para.start, para.end)
    }
}

pub(super) fn build(
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    spans: &[TextSpan],
    folds: &[Range<usize>],
    previous: Option<&Cache>,
) -> Built {
    let mut cache = Cache::default();
    let mut work = Work::default();
    let mut paras = Vec::new();
    for (start, end) in paragraph_ranges(text, text.is_empty() || text.ends_with('\n')) {
        paras.push(shape_range(
            engine,
            font,
            style,
            width,
            text,
            start,
            end,
            gap_of(style, start),
            spans,
            previous,
            &mut cache,
            &mut work,
        ));
    }
    let (visual_lines, max_top) = recompute(&mut paras, folds);
    Built {
        index: RowIndex {
            paras,
            visual_lines,
            max_top,
        },
        cache,
        work,
    }
}

/// Reuse paragraph heights outside a single text replacement. `previous` is the
/// cache from the last build; unchanged paragraphs are not shaped again.
pub(super) fn update_text(
    mut index: RowIndex,
    mut cache: Cache,
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    old: &str,
    new: &str,
    spans: &[TextSpan],
    folds: &[Range<usize>],
) -> Built {
    if new.is_empty() {
        return build(engine, font, style, width, new, spans, folds, Some(&cache));
    }
    let (old_lo, old_hi, _) = change_span(old, new);
    let delta = new.len() as isize - old.len() as isize;
    let mut work = Work::default();
    // Expand one replacement to the paragraphs it joins. A paragraph that
    // merely shifts, including one that begins at a newline-terminated insert,
    // stays in place. A paragraph the edit merges into is shaped with it.
    let unchanged = old == new;
    let first_dirty = if unchanged {
        index.paras.len()
    } else {
        index
            .paras
            .iter()
            .position(|para| para.end > old_lo || (para.start == para.end && para.start == old_lo))
            .unwrap_or(index.paras.len())
    };
    let mut suffix_from = index
        .paras
        .iter()
        .position(|para| para.start >= old_hi)
        .unwrap_or(index.paras.len());
    if suffix_from < first_dirty {
        suffix_from = first_dirty;
    }
    while suffix_from < index.paras.len() {
        let new_start = (index.paras[suffix_from].start as isize + delta) as usize;
        let boundary =
            new_start == 0 || new.as_bytes().get(new_start.wrapping_sub(1)) == Some(&b'\n');
        if boundary || new_start > new.len() {
            break;
        }
        suffix_from += 1;
    }
    let origin = index
        .paras
        .get(first_dirty)
        .map(|para| para.start)
        .unwrap_or(new.len());
    let middle_end = index
        .paras
        .get(suffix_from)
        .map(|para| (para.start as isize + delta) as usize)
        .unwrap_or(new.len())
        .max(origin);
    for para in &mut index.paras[..first_dirty] {
        if spans_match(para, new, spans) {
            work.reused += 1;
        } else {
            let (start, end) = (para.start, para.end);
            *para = shape_range(
                engine,
                font,
                style,
                width,
                new,
                start,
                end,
                gap_of(style, start),
                spans,
                None,
                &mut cache,
                &mut work,
            );
        }
    }
    for para in &mut index.paras[suffix_from..] {
        shift_para(para, delta);
        if spans_match(para, new, spans) {
            work.reused += 1;
        } else {
            let (start, end, gap) = (para.start, para.end, para.gap);
            *para = shape_range(
                engine, font, style, width, new, start, end, gap, spans, None, &mut cache,
                &mut work,
            );
        }
    }
    let trailing_empty = first_dirty < index.paras.len()
        && suffix_from == index.paras.len()
        && (new.is_empty() || new.ends_with('\n'));
    let mut middle = Vec::new();
    if origin < middle_end || trailing_empty {
        for (start, end) in paragraph_ranges(&new[origin..middle_end], trailing_empty) {
            middle.push(shape_range(
                engine,
                font,
                style,
                width,
                new,
                origin + start,
                origin + end,
                gap_of(style, origin + start),
                spans,
                None,
                &mut cache,
                &mut work,
            ));
        }
    }
    index.paras.splice(first_dirty..suffix_from, middle);
    let (visual_lines, max_top) = recompute(&mut index.paras, folds);
    index.visual_lines = visual_lines;
    index.max_top = max_top;
    Built { index, cache, work }
}

fn spans_match(para: &Para, text: &str, spans: &[TextSpan]) -> bool {
    let end = para.end.min(text.len());
    let start = para.start.min(end);
    span_fp(&local_spans(spans, start, end)) == para.span_fp
}

fn change_span(old: &str, new: &str) -> (usize, usize, usize) {
    let mut prefix = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(prefix) || !new.is_char_boundary(prefix) {
        prefix = prefix.saturating_sub(1);
    }
    let mut suffix = old[prefix..]
        .bytes()
        .rev()
        .zip(new[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while prefix + suffix > old.len()
        || prefix + suffix > new.len()
        || !old.is_char_boundary(old.len() - suffix)
        || !new.is_char_boundary(new.len() - suffix)
    {
        suffix = suffix.saturating_sub(1);
    }
    (prefix, old.len() - suffix, new.len() - suffix)
}

/// Keep one-line paragraphs that still fit. Shape only paragraphs the new width wraps.
pub(super) fn refit_width(
    mut index: RowIndex,
    mut cache: Cache,
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    spans: &[TextSpan],
    folds: &[Range<usize>],
) -> Built {
    let mut work = Work::default();
    for para in &mut index.paras {
        if para.lines.len() == 1 && para.ink <= width {
            work.reused += 1;
            continue;
        }
        let (start, end, gap) = (para.start, para.end, para.gap);
        *para = shape_range(
            engine, font, style, width, text, start, end, gap, spans, None, &mut cache, &mut work,
        );
    }
    let (visual_lines, max_top) = recompute(&mut index.paras, folds);
    index.visual_lines = visual_lines;
    index.max_top = max_top;
    Built { index, cache, work }
}

pub(super) fn refold(mut index: RowIndex, folds: &[Range<usize>]) -> RowIndex {
    let (visual_lines, max_top) = recompute(&mut index.paras, folds);
    index.visual_lines = visual_lines;
    index.max_top = max_top;
    index
}

pub(super) fn window_layout(
    index: &RowIndex,
    engine: &dyn TextEngine,
    font: &dyn crate::Font,
    style: &TextStyle,
    width: f32,
    text: &str,
    spans: &[TextSpan],
    first: usize,
    last: usize,
    previous: Option<&Cache>,
    cache: &mut Cache,
    work: &mut Work,
) -> TextLayout {
    if index.visual_lines == 0 {
        return TextLayout::shaped(Vec::new(), Vec::new(), 0.0, 0.0);
    }
    let last = last.min(index.line_count().saturating_sub(1));
    let first = first.min(last);
    let mut lines = Vec::new();
    let mut row = first;
    let mut shaped_para = usize::MAX;
    let mut paragraph = None;
    while row <= last {
        let info = index.line(row);
        let para_i = index.para_for_row(row);
        if para_i != shaped_para {
            let (start, end) = index.para_range(para_i);
            let local = local_spans(spans, start, end);
            let (shaped, fresh) = geometry::shape_piece(
                engine,
                font,
                style,
                width,
                &text[start..end],
                &local,
                previous,
                cache,
            );
            if fresh {
                work.paragraphs += 1;
                work.bytes += (end - start) as u64;
            } else {
                work.reused += 1;
            }
            paragraph = Some(shaped);
            shaped_para = para_i;
        }
        let (start, _) = index.para_range(para_i);
        if let Some(src) = paragraph.as_ref().and_then(|item| {
            item.layout()
                .lines
                .iter()
                .find(|line| start + line.start == info.start)
        }) {
            let mut line = src.clone();
            line.start += start;
            line.end += start;
            line.y = info.y;
            for edge in &mut line.edges {
                edge.0 += start;
            }
            lines.push(line);
        }
        row += 1;
    }
    let height = lines.last().map_or(0.0, |line| line.y + line.h);
    let widest = lines.iter().map(|line| line.width).fold(0.0f32, f32::max);
    TextLayout::shaped(Vec::new(), lines, widest, height)
}
