//! Text box geometry. Painting and picking use the same layout, so the
//! measure that shapes glyphs should also be the one passed here.

use crate::schema::{Content, FontStyle, Node, TextAlign, TextLayout, TextSpan};

/// Measures one line of text in canvas units.
pub trait TextMeasure {
    fn width(&self, line: &str, font: &Font<'_>) -> f64;

    /// Width of `line` where `runs` (sorted, non-overlapping) change the
    /// weight or style of `font`. The default measures each piece on its own;
    /// a shaper should measure the line as one styled run.
    fn styled_width(&self, line: &str, font: &Font<'_>, runs: &[StyledRun]) -> f64 {
        let mut total = 0.0;
        let mut at = 0;
        for run in runs {
            if run.start > at {
                total += self.width(&line[at..run.start], font);
            }
            let styled = Font {
                weight: run.weight,
                style: run.style,
                ..*font
            };
            total += self.width(&line[run.start..run.end], &styled);
            at = run.end;
        }
        if at < line.len() || runs.is_empty() {
            total += self.width(&line[at..], font);
        }
        total
    }
}

/// Part of a line drawn with an inline span's weight or style. Offsets are
/// bytes into the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StyledRun {
    pub start: usize,
    pub end: usize,
    pub weight: u16,
    pub style: FontStyle,
}

/// Font properties of a text node.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Font<'a> {
    pub family: &'a str,
    pub size: f64,
    pub weight: u16,
    pub style: FontStyle,
}

/// Assumes every character is 0.6 em wide. Good enough for headless bounds
/// and tests; real renderers supply their shaper.
#[derive(Debug, Clone, Copy, Default)]
pub struct EstimateMeasure;

impl TextMeasure for EstimateMeasure {
    fn width(&self, line: &str, font: &Font<'_>) -> f64 {
        line.chars().count() as f64 * font.size * 0.6
    }
}

/// Laid-out lines of a text node in its local coordinates.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub lines: Vec<String>,
    /// UTF-16 offset of each line's first character in the text.
    pub starts: Vec<u32>,
    /// Top of each line, before padding.
    pub ys: Vec<f64>,
    pub align: TextAlign,
    /// Inset on every side, 0.06 em.
    pub padding: f64,
    pub line_height: f64,
    pub width: f64,
    pub height: f64,
}

/// Default line height, in ems, when a text node has no layout.
pub const DEFAULT_LINE_HEIGHT: f64 = 1.3;

/// Lays out a text node; `None` for other content.
pub fn layout_node(node: &Node, measure: &dyn TextMeasure) -> Option<TextBox> {
    let Content::Text {
        text,
        font_family,
        font_size,
        font_weight,
        font_style,
        ..
    } = &node.content
    else {
        return None;
    };
    let font = Font {
        family: font_family,
        size: *font_size,
        weight: font_weight.unwrap_or(400),
        style: font_style.unwrap_or(FontStyle::Normal),
    };
    Some(layout_text(text, &font, node.text_layout.as_ref(), measure))
}

/// Breaks text on hard newlines and, when the layout has a width, wraps
/// between words (or inside words longer than a line).
pub fn layout_text(
    text: &str,
    font: &Font<'_>,
    layout: Option<&TextLayout>,
    measure: &dyn TextMeasure,
) -> TextBox {
    let size = font.size;
    let padding = size * 0.06;
    let line_height = size * layout.map_or(DEFAULT_LINE_HEIGHT, |l| l.line_height);
    let spacing = layout.and_then(|l| l.letter_spacing).unwrap_or(0.0);
    let paragraph = layout.and_then(|l| l.paragraph_spacing).unwrap_or(0.0);
    let spans = layout.map_or(&[][..], |l| l.spans.as_slice());
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let read = |start: u32, line: &str| {
        let runs = styled_runs(&normalized, spans, start, line, font);
        (measure.styled_width(line, font, &runs) + line.chars().count() as f64 * spacing).max(0.0)
    };
    let (mut lines, mut starts, mut ys) = (Vec::new(), Vec::new(), Vec::new());
    let (mut offset, mut y) = (0u32, 0.0);
    for hard in normalized.split('\n') {
        let wrapped = match layout.and_then(|l| l.width) {
            Some(width) => wrap(hard, offset, (width - padding * 2.0).max(1.0), &read),
            None => vec![hard.to_string()],
        };
        for line in wrapped {
            starts.push(offset);
            offset += line.encode_utf16().count() as u32;
            lines.push(line);
            ys.push(y);
            y += line_height;
        }
        offset += 1;
        y += paragraph;
    }
    let widest = lines
        .iter()
        .zip(&starts)
        .map(|(l, &start)| read(start, l))
        .fold(size * 0.3, f64::max);
    TextBox {
        width: layout
            .and_then(|l| l.width)
            .unwrap_or(widest + padding * 2.0),
        height: line_height.max(y - paragraph) + padding * 2.0,
        align: layout.map_or(TextAlign::Left, |l| l.align),
        lines,
        starts,
        ys,
        padding,
        line_height,
    }
}

/// Keeps whitespace, like a CSS `pre-wrap` box. `offset` is the UTF-16
/// offset of `text` in the node, so `read` can apply inline spans.
fn wrap(text: &str, offset: u32, width: f64, read: &dyn Fn(u32, &str) -> f64) -> Vec<String> {
    let mut result = Vec::new();
    let mut line = String::new();
    let mut start = offset;
    let push = |line: &mut String, start: &mut u32, result: &mut Vec<String>| {
        *start += line.encode_utf16().count() as u32;
        result.push(std::mem::take(line));
    };
    for token in tokens(text) {
        if !line.is_empty() && read(start, &format!("{line}{}", token.trim_end())) > width {
            push(&mut line, &mut start, &mut result);
        }
        let token_start = start + line.encode_utf16().count() as u32;
        if read(token_start, token.trim_end()) <= width {
            line.push_str(token);
            continue;
        }
        for ch in token.chars() {
            if !line.is_empty()
                && !ch.is_whitespace()
                && read(start, &format!("{line}{ch}")) > width
            {
                push(&mut line, &mut start, &mut result);
            }
            line.push(ch);
        }
    }
    result.push(line);
    result
}

/// Pieces of `line`, which starts at UTF-16 `start` in `text`, whose span
/// changes the weight or style. The first span covering a piece wins.
fn styled_runs(
    text: &str,
    spans: &[TextSpan],
    start: u32,
    line: &str,
    font: &Font<'_>,
) -> Vec<StyledRun> {
    if spans.is_empty() || line.is_empty() {
        return Vec::new();
    }
    let base = utf16_to_byte(text, start);
    let end = base + line.len();
    let bytes: Vec<(usize, usize, &TextSpan)> = spans
        .iter()
        .map(|span| {
            (
                utf16_to_byte(text, span.start),
                utf16_to_byte(text, span.end),
                span,
            )
        })
        .collect();
    let mut points = vec![base, end];
    for &(from, to, _) in &bytes {
        points.extend([from, to].into_iter().filter(|&p| p > base && p < end));
    }
    points.sort_unstable();
    points.dedup();
    let mut runs: Vec<StyledRun> = Vec::new();
    for pair in points.windows(2) {
        let Some(&(_, _, span)) = bytes
            .iter()
            .find(|&&(from, to, _)| from <= pair[0] && to >= pair[1])
        else {
            continue;
        };
        let weight = span.font_weight.unwrap_or(font.weight);
        let style = span.font_style.unwrap_or(font.style);
        if weight == font.weight && style == font.style {
            continue;
        }
        let (a, b) = (pair[0] - base, pair[1] - base);
        match runs.last_mut() {
            Some(last) if last.end == a && last.weight == weight && last.style == style => {
                last.end = b
            }
            _ => runs.push(StyledRun {
                start: a,
                end: b,
                weight,
                style,
            }),
        }
    }
    runs
}

/// Byte offset of a UTF-16 offset, clamped to the text.
fn utf16_to_byte(text: &str, offset: u32) -> usize {
    let mut units = 0u32;
    for (byte, ch) in text.char_indices() {
        if units >= offset {
            return byte;
        }
        units += ch.len_utf16() as u32;
    }
    text.len()
}

/// Words with their trailing whitespace, or a leading whitespace run.
fn tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = false;
    for (i, ch) in text.char_indices() {
        let space = ch.is_whitespace();
        if !space && in_space && i > start {
            out.push(&text[start..i]);
            start = i;
        }
        in_space = space;
    }
    if start < text.len() || out.is_empty() {
        out.push(&text[start..]);
    }
    out
}
