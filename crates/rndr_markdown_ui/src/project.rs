//! Visible blocks projected from a markdown concrete tree.
//!
//! Offsets inside a block are relative to [`DisplayBlock::span`], so a later
//! block can slide forward after an edit without rewriting its text. The
//! session bytes stay the authority; this projection owns neither history
//! nor ids.

use std::collections::VecDeque;
use std::ops::Range;

use rndr_core::{Color, TextEngine, TextSpan, TextStyle, FONT_WEIGHT_BOLD};
use rndr_markdown::{EditSpan, Green, SyntaxKind};

/// Colors for marks. The label color paints bytes that have no span.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Body glyphs.
    pub text: Color,
    /// Markers, comments, and rules.
    pub muted: Color,
    /// Link glyphs.
    pub link: Color,
    /// Inline code and fence fill.
    pub code: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            text: Color::rgb(0.93, 0.94, 0.95),
            muted: Color::rgb(0.62, 0.64, 0.68),
            link: Color::rgb(0.45, 0.68, 0.95),
            code: Color::rgba(0.55, 0.58, 0.62, 0.22),
        }
    }
}

/// One visible run of a block. Source offsets are relative to the block span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// First displayed byte.
    pub display_start: usize,
    /// Byte after the displayed run.
    pub display_end: usize,
    /// First source byte, relative to [`DisplayBlock::span`].
    pub source_start: usize,
    /// Source byte after the run, relative to [`DisplayBlock::span`].
    pub source_end: usize,
}

/// How a block is shown. The source spelling lives on the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DisplayKind {
    /// Prose.
    Paragraph,
    /// ATX or setext. The marker is [`DisplayBlock::marker`].
    Heading(u8),
    /// A bullet item. The marker is chrome.
    Bullet,
    /// An ordered item. The value is the number as written.
    Ordered(u64),
    /// A task item.
    Task(bool),
    /// A fenced code block that is not Liquid.
    Code {
        /// Info string as written, without the fence.
        info: String,
    },
    /// A Liquid fence. `valid` is whether `parse_fence` accepts the body.
    Liquid {
        /// Canonical kind. Aliases stay in the source info string.
        canonical: String,
        /// The body parsed.
        valid: bool,
    },
    /// A thematic break.
    Rule,
    /// A blank line owned by the parent. Not a caret target.
    Gap,
    /// A GFM table, cells joined for display.
    Table,
    /// An image whose alt is the text.
    Image {
        /// Alt bytes.
        alt: String,
    },
    /// HTML, indented code, a link reference, or bytes the view does not structure.
    Opaque,
}

/// One painted block.
#[derive(Clone, Debug, PartialEq)]
pub struct DisplayBlock {
    /// Source range of the concrete node, absolute.
    pub span: Range<usize>,
    /// Visual kind.
    pub kind: DisplayKind,
    /// Marker drawn before the text. It is not part of [`Self::text`] and it
    /// has no source segment; a click on it places the caret at the text start.
    pub chrome: String,
    /// Visible text. Markers, fence lines, and a trailing line ending are omitted.
    pub text: String,
    /// Map from [`Self::text`] bytes onto the source.
    pub segments: Vec<Segment>,
    /// Inline styles. Ranges are bytes of [`Self::text`], not the chrome.
    pub spans: Vec<TextSpan>,
    /// Source range of a heading or list marker. Backspace at the start deletes it.
    pub marker: Option<Range<usize>>,
    /// List nesting. Zero is not a list item.
    pub depth: u8,
    /// Quote nesting.
    pub quote: u8,
    /// Last measured height in points. Zero means not measured yet.
    pub height: f32,
    /// Relative source offset where a caret at the end of [`Self::text`] sits.
    pub caret_end: usize,
}

impl DisplayBlock {
    /// Chrome plus text, which is what the label draws.
    pub fn label_text(&self) -> String {
        let mut out = String::with_capacity(self.chrome.len() + self.text.len());
        out.push_str(&self.chrome);
        out.push_str(&self.text);
        out
    }

    /// Source offset for a byte index into [`Self::label_text`].
    pub fn display_to_source(&self, label_index: usize) -> usize {
        let index = label_index
            .saturating_sub(self.chrome.len())
            .min(self.text.len());
        self.span.start + self.rel_source(index)
    }

    /// Label byte index for a source offset inside this block.
    pub fn source_to_label(&self, source: usize) -> usize {
        let rel = source.saturating_sub(self.span.start);
        self.chrome.len() + self.rel_display(rel)
    }

    fn rel_source(&self, display: usize) -> usize {
        if self.text.is_empty() {
            return self.caret_end;
        }
        for seg in &self.segments {
            if display < seg.display_start {
                return seg.source_start;
            }
            if display <= seg.display_end {
                let shown = seg.display_end - seg.display_start;
                let src = seg.source_end - seg.source_start;
                if shown == src {
                    return seg.source_start + (display - seg.display_start);
                }
                if display == seg.display_end {
                    return seg.source_end;
                }
                return seg.source_start;
            }
        }
        self.caret_end
    }

    fn rel_display(&self, rel: usize) -> usize {
        if rel >= self.caret_end {
            return self.text.len();
        }
        for seg in &self.segments {
            if rel < seg.source_start {
                return seg.display_start;
            }
            if rel <= seg.source_end {
                let shown = seg.display_end - seg.display_start;
                let src = seg.source_end - seg.source_start;
                if shown == src {
                    return seg.display_start + (rel - seg.source_start);
                }
                if rel == seg.source_end {
                    return seg.display_end;
                }
                return seg.display_start;
            }
        }
        self.text.len()
    }

    /// Source range of the display bytes `[from, to)` in the label text.
    pub fn display_range(&self, from: usize, to: usize) -> Range<usize> {
        let start = self.display_to_source(from.min(to));
        let end = self.display_to_source(from.max(to));
        start..end.max(start)
    }
}

#[derive(Clone, Copy)]
struct Ctx {
    depth: u8,
    quote: u8,
}

/// Project every block of `root`.
pub fn project(bytes: &str, root: &Green, palette: &Palette) -> Vec<DisplayBlock> {
    let mut out = Vec::new();
    walk_fresh(
        bytes,
        root,
        0,
        Ctx { depth: 0, quote: 0 },
        palette,
        &mut out,
    );
    if out.is_empty() {
        out.push(empty_paragraph(0..0));
    }
    out
}

/// Rebuild `blocks` after an edit.
///
/// Blocks that do not touch the edit, and are not the immediate neighbor of
/// one that does, keep their text. Their spans slide by the edit's delta.
/// The result matches [`project`] of the new tree.
pub fn update_projection(
    blocks: &mut Vec<DisplayBlock>,
    bytes: &str,
    root: &Green,
    span: EditSpan,
    palette: &Palette,
) {
    let stale = stale_flags(blocks, span);
    let delta = span.new_end as isize - span.old_end as isize;
    let mut pool = VecDeque::new();
    for (block, stale) in std::mem::take(blocks).into_iter().zip(stale) {
        if stale {
            continue;
        }
        let mut block = block;
        if block.span.start >= span.old_end {
            slide(&mut block.span, delta);
            if let Some(marker) = block.marker.as_mut() {
                slide(marker, delta);
            }
        }
        pool.push_back(block);
    }
    let mut out = Vec::new();
    let dirty = span.old_start..span.new_end.max(span.old_start.saturating_add(1));
    let mut walk = Walk {
        bytes,
        palette,
        dirty,
        pool: &mut pool,
        out: &mut out,
    };
    walk.node(root, 0, Ctx { depth: 0, quote: 0 });
    if out.is_empty() {
        out.push(empty_paragraph(0..bytes.len()));
    }
    *blocks = out;
}

/// Height of one block at `width`, using the same text engine paint will use.
pub fn measure_block(
    engine: &dyn TextEngine,
    block: &DisplayBlock,
    width: f32,
    style: &TextStyle,
) -> f32 {
    match block.kind {
        DisplayKind::Gap => 8.0,
        DisplayKind::Rule => 16.0,
        _ => {
            let text = block.label_text();
            let mut spans = block.spans.clone();
            let chrome = block.chrome.len();
            for span in &mut spans {
                span.start += chrome;
                span.end += chrome;
            }
            engine
                .measure(&text, style, &spans, width.max(1.0))
                .height
                .max(style.size)
        }
    }
}

fn stale_flags(blocks: &[DisplayBlock], span: EditSpan) -> Vec<bool> {
    let point = span.old_start;
    let end = span.old_end.max(point);
    let mut flags: Vec<bool> = blocks
        .iter()
        .map(|block| block.span.start < end && block.span.end > point)
        .collect();
    let mut dilated = flags.clone();
    for (index, flag) in flags.iter().enumerate() {
        if *flag {
            if index > 0 {
                dilated[index - 1] = true;
            }
            if index + 1 < dilated.len() {
                dilated[index + 1] = true;
            }
        }
    }
    // A point edit on a boundary also dirties the block that starts there.
    for (index, block) in blocks.iter().enumerate() {
        if block.span.start == point || block.span.end == point {
            dilated[index] = true;
            if index > 0 {
                dilated[index - 1] = true;
            }
        }
    }
    flags = dilated;
    flags
}

fn slide(range: &mut Range<usize>, delta: isize) {
    let start = range.start as isize + delta;
    let end = range.end as isize + delta;
    let start = start.max(0) as usize;
    let end = end.max(start as isize) as usize;
    *range = start..end;
}

struct Walk<'a> {
    bytes: &'a str,
    palette: &'a Palette,
    dirty: Range<usize>,
    pool: &'a mut VecDeque<DisplayBlock>,
    out: &'a mut Vec<DisplayBlock>,
}

impl Walk<'_> {
    fn node(&mut self, node: &Green, at: usize, ctx: Ctx) {
        let span = at..at + node.width();
        if is_container(node.kind()) {
            if !overlaps(&span, &self.dirty) && reuse_inside(self.pool, &span, self.out) {
                return;
            }
            if node.kind() == SyntaxKind::ListItem {
                self.item(node, at, ctx);
                return;
            }
            let mut child_ctx = ctx;
            if node.kind() == SyntaxKind::BlockQuote {
                child_ctx.quote = child_ctx.quote.saturating_add(1);
            }
            if node.kind() == SyntaxKind::List {
                child_ctx.depth = child_ctx.depth.saturating_add(1);
            }
            let mut child_at = at;
            for child in node.children() {
                self.node(child, child_at, child_ctx);
                child_at += child.width();
            }
            return;
        }
        drop_passed(self.pool, span.start);
        if !overlaps(&span, &self.dirty)
            && self.pool.front().is_some_and(|block| block.span == span)
        {
            self.out.push(self.pool.pop_front().unwrap());
            return;
        }
        project_node(self.bytes, node, at, ctx, self.palette, self.out);
    }

    fn item(&mut self, node: &Green, at: usize, ctx: Ctx) {
        let mut child_at = at;
        let mut marker = None;
        let mut marker_text = String::new();
        let before = self.out.len();
        for child in node.children() {
            let child_end = child_at + child.width();
            if child.kind() == SyntaxKind::ListMarker && marker.is_none() {
                marker = Some(child_at..child_end);
                marker_text = child.text().to_string();
            } else {
                self.node(child, child_at, ctx);
            }
            child_at = child_end;
        }
        if self.out.len() == before {
            let mut block = empty_paragraph(at..child_at);
            block.depth = ctx.depth;
            block.quote = ctx.quote;
            self.out.push(block);
        }
        if let Some(marker) = marker {
            if let Some(first) = self.out.get_mut(before) {
                let (kind, chrome) = marker_chrome(&marker_text);
                first.kind = kind;
                first.chrome = chrome;
                first.marker = Some(marker);
                first.depth = ctx.depth;
            }
        }
    }
}

fn walk_fresh(
    bytes: &str,
    node: &Green,
    at: usize,
    ctx: Ctx,
    palette: &Palette,
    out: &mut Vec<DisplayBlock>,
) {
    let mut pool = VecDeque::new();
    let mut walk = Walk {
        bytes,
        palette,
        dirty: 0..usize::MAX,
        pool: &mut pool,
        out,
    };
    walk.node(node, at, ctx);
}

fn is_container(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Document | SyntaxKind::BlockQuote | SyntaxKind::List | SyntaxKind::ListItem
    )
}

fn overlaps(span: &Range<usize>, dirty: &Range<usize>) -> bool {
    span.start < dirty.end && span.end > dirty.start
}

fn drop_passed(pool: &mut VecDeque<DisplayBlock>, start: usize) {
    while pool.front().is_some_and(|block| block.span.end <= start) {
        pool.pop_front();
    }
}

fn reuse_inside(
    pool: &mut VecDeque<DisplayBlock>,
    span: &Range<usize>,
    out: &mut Vec<DisplayBlock>,
) -> bool {
    drop_passed(pool, span.start);
    let mut took = false;
    while pool
        .front()
        .is_some_and(|block| block.span.start >= span.start && block.span.end <= span.end)
    {
        out.push(pool.pop_front().unwrap());
        took = true;
    }
    took
}

fn project_node(
    bytes: &str,
    node: &Green,
    at: usize,
    ctx: Ctx,
    palette: &Palette,
    out: &mut Vec<DisplayBlock>,
) {
    let span = at..at + node.width();
    let mut block = match node.kind() {
        SyntaxKind::BlockGap => {
            let mut gap = empty_paragraph(span);
            gap.kind = DisplayKind::Gap;
            gap.caret_end = 0;
            gap
        }
        SyntaxKind::ThematicBreak => {
            let mut rule = empty_paragraph(span);
            rule.kind = DisplayKind::Rule;
            rule.chrome = "—".into();
            rule
        }
        SyntaxKind::Heading => project_heading(bytes, node, at, palette),
        SyntaxKind::FencedCode => project_fence(bytes, node, at, false),
        SyntaxKind::LiquidFence => project_fence(bytes, node, at, true),
        SyntaxKind::Table => project_table(bytes, node, at, palette),
        SyntaxKind::HtmlBlock
        | SyntaxKind::IndentedCode
        | SyntaxKind::OpaqueBlock
        | SyntaxKind::LinkRefDef => project_raw(bytes, node, at),
        _ => project_flow(bytes, node, at, palette),
    };
    block.depth = ctx.depth;
    block.quote = ctx.quote;
    out.push(block);
}

fn empty_paragraph(span: Range<usize>) -> DisplayBlock {
    DisplayBlock {
        caret_end: span.end - span.start,
        span,
        kind: DisplayKind::Paragraph,
        chrome: String::new(),
        text: String::new(),
        segments: Vec::new(),
        spans: Vec::new(),
        marker: None,
        depth: 0,
        quote: 0,
        height: 0.0,
    }
}

fn project_heading(bytes: &str, node: &Green, at: usize, palette: &Palette) -> DisplayBlock {
    let mut block = project_flow(bytes, node, at, palette);
    block.kind = DisplayKind::Heading(heading_level(node));
    let mut child_at = at;
    for child in node.children() {
        let end = child_at + child.width();
        if child.kind() == SyntaxKind::AtxMarker {
            block.marker = Some(child_at..end);
            break;
        }
        child_at = end;
    }
    block
}

fn heading_level(node: &Green) -> u8 {
    let marker = node
        .children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::AtxMarker)
        .map(Green::text)
        .unwrap_or("");
    let hashes = marker.bytes().filter(|byte| *byte == b'#').count();
    if (1..=6).contains(&hashes) {
        hashes as u8
    } else if node
        .children()
        .iter()
        .any(|child| child.kind() == SyntaxKind::SetextUnderline && child.text().contains('='))
    {
        1
    } else {
        2
    }
}

fn project_flow(bytes: &str, node: &Green, at: usize, palette: &Palette) -> DisplayBlock {
    let mut acc = Acc::new(at, palette);
    let children = node.children();
    for (index, child) in children.iter().enumerate() {
        let last = index + 1 == children.len();
        if last && child.kind() == SyntaxKind::LineEnding {
            acc.skip(child.width());
            continue;
        }
        flow_child(bytes, child, &mut acc);
    }
    acc.finish(node.kind(), at..at + node.width())
}

fn project_raw(bytes: &str, node: &Green, at: usize) -> DisplayBlock {
    let span = at..at + node.width();
    let text = bytes
        .get(span.clone())
        .unwrap_or("")
        .trim_end_matches(['\n', '\r']);
    let mut block = empty_paragraph(span.clone());
    block.kind = DisplayKind::Opaque;
    block.text = text.to_string();
    if !text.is_empty() {
        block.segments.push(Segment {
            display_start: 0,
            display_end: text.len(),
            source_start: 0,
            source_end: text.len(),
        });
    }
    block.caret_end = text.len();
    let mut code = TextSpan::new(0, text.len(), Color::rgb(0.75, 0.78, 0.82));
    code.family = Some("monospace".into());
    if !text.is_empty() {
        block.spans.push(code);
    }
    block
}

fn project_fence(bytes: &str, node: &Green, at: usize, liquid: bool) -> DisplayBlock {
    let span = at..at + node.width();
    let mut child_at = at;
    let mut info = String::new();
    let mut body_start = None;
    let mut body_end = None;
    for child in node.children() {
        let end = child_at + child.width();
        if child.kind() == SyntaxKind::FenceLine && info.is_empty() && body_start.is_none() {
            info = fence_info(child.text());
            body_start = Some(end);
        } else if child.kind() == SyntaxKind::FenceLine {
            body_end = Some(child_at);
        }
        child_at = end;
    }
    let body_start = body_start.unwrap_or(at);
    let body_end = body_end.unwrap_or(span.end);
    if liquid {
        let canonical = canonical_kind(&info);
        let body = bytes.get(body_start..body_end).unwrap_or("");
        let valid = rndr_liquid::parse_fence(&canonical, body.trim_end_matches(['\n', '\r']))
            .is_some()
            || rndr_liquid::parse_fence(&info, body.trim_end_matches(['\n', '\r'])).is_some();
        if valid {
            return project_liquid_fields(bytes, node, at, &canonical);
        }
        let mut block = project_raw_range(bytes, body_start..body_end, span);
        block.kind = DisplayKind::Liquid {
            canonical,
            valid: false,
        };
        return block;
    }
    let mut block = project_raw_range(bytes, body_start..body_end, span);
    block.kind = DisplayKind::Code { info };
    block
}

fn project_raw_range(bytes: &str, body: Range<usize>, span: Range<usize>) -> DisplayBlock {
    let text = bytes
        .get(body.clone())
        .unwrap_or("")
        .trim_end_matches(['\n', '\r']);
    let mut block = empty_paragraph(span);
    block.text = text.to_string();
    let source_start = body.start - block.span.start;
    if !text.is_empty() {
        block.segments.push(Segment {
            display_start: 0,
            display_end: text.len(),
            source_start,
            source_end: source_start + text.len(),
        });
    }
    block.caret_end = source_start + text.len();
    let mut code = TextSpan::new(0, text.len(), Color::rgb(0.75, 0.78, 0.82));
    code.family = Some("monospace".into());
    if !text.is_empty() {
        block.spans.push(code);
    }
    block
}

fn project_liquid_fields(bytes: &str, node: &Green, at: usize, canonical: &str) -> DisplayBlock {
    let span = at..at + node.width();
    let mut block = empty_paragraph(span);
    block.kind = DisplayKind::Liquid {
        canonical: canonical.to_string(),
        valid: true,
    };
    block.chrome = format!("{canonical}\n");
    let mut child_at = at;
    for child in node.children() {
        let end = child_at + child.width();
        if child.kind() != SyntaxKind::FenceLine {
            append_field(bytes, child, child_at, at, &mut block);
        }
        child_at = end;
    }
    if block.text.ends_with('\n') {
        block.text.pop();
    }
    block.caret_end = block.segments.last().map(|seg| seg.source_end).unwrap_or(0);
    block
}

fn append_field(bytes: &str, node: &Green, at: usize, base: usize, block: &mut DisplayBlock) {
    match node.kind() {
        SyntaxKind::Field => {
            let mut cursor = at;
            let mut key = "";
            let mut value: Option<Range<usize>> = None;
            for child in node.children() {
                let end = cursor + child.width();
                if child.kind() == SyntaxKind::FieldKey {
                    key = child.text();
                }
                if child.kind() == SyntaxKind::FieldValue {
                    value = Some(cursor..end);
                }
                cursor = end;
            }
            let line_start = block.text.len();
            block.text.push_str(key.trim());
            block.text.push_str("  ");
            if let Some(value) = value {
                let shown = bytes.get(value.clone()).unwrap_or("");
                let display_start = block.text.len();
                block.text.push_str(shown);
                block.segments.push(Segment {
                    display_start,
                    display_end: block.text.len(),
                    source_start: value.start - base,
                    source_end: value.end - base,
                });
            }
            let mut span = TextSpan::new(
                line_start,
                line_start + key.trim().len(),
                Color::rgb(0.62, 0.64, 0.68),
            );
            span.weight = Some(FONT_WEIGHT_BOLD);
            block.spans.push(span);
            block.text.push('\n');
        }
        SyntaxKind::FieldComment => {
            let shown = node.concat();
            let display_start = block.text.len();
            let text = shown.trim_end_matches(['\n', '\r']);
            block.text.push_str(text);
            block.segments.push(Segment {
                display_start,
                display_end: block.text.len(),
                source_start: at - base,
                source_end: at - base + text.len(),
            });
            block.spans.push(
                TextSpan::new(display_start, block.text.len(), Color::rgb(0.5, 0.55, 0.5)).italic(),
            );
            block.text.push('\n');
        }
        _ => {
            let mut cursor = at;
            for child in node.children() {
                append_field(bytes, child, cursor, base, block);
                cursor += child.width();
            }
        }
    }
}

fn project_table(bytes: &str, node: &Green, at: usize, palette: &Palette) -> DisplayBlock {
    let mut acc = Acc::new(at, palette);
    let mut child_at = at;
    for child in node.children() {
        if child.kind() == SyntaxKind::TableRow {
            let mut cell_at = child_at;
            let mut first = true;
            for cell in child.children() {
                let cell_end = cell_at + cell.width();
                if cell.kind() == SyntaxKind::TableCell {
                    if !first {
                        acc.push_chrome(" | ");
                    }
                    first = false;
                    flow_child(bytes, cell, &mut acc);
                }
                cell_at = cell_end;
            }
            acc.push_shown("\n", 0);
        }
        child_at += child.width();
    }
    if acc.text.ends_with('\n') {
        acc.text.pop();
    }
    let mut block = acc.finish(SyntaxKind::Table, at..at + node.width());
    block.kind = DisplayKind::Table;
    block
}

fn fence_info(line: &str) -> String {
    let line = line.trim_end_matches(['\n', '\r']).trim();
    let line = line.trim_start_matches(['`', '~']);
    line.split_whitespace().next().unwrap_or("").to_string()
}

fn canonical_kind(info: &str) -> String {
    match info.to_lowercase().as_str() {
        "action_row" => "actions".into(),
        "chip_group" => "chips".into(),
        "procedure" => "recipe".into(),
        other => rndr_liquid::FENCE_LANGS
            .iter()
            .find(|lang| **lang == other)
            .map(|lang| (*lang).to_string())
            .unwrap_or_else(|| "code".into()),
    }
}

fn marker_chrome(text: &str) -> (DisplayKind, String) {
    let trimmed = text.trim();
    if trimmed.contains("[x]") || trimmed.contains("[X]") {
        return (DisplayKind::Task(true), "☑ ".into());
    }
    if trimmed.contains("[ ]") {
        return (DisplayKind::Task(false), "☐ ".into());
    }
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if let Ok(number) = digits.parse::<u64>() {
        if !digits.is_empty() {
            return (DisplayKind::Ordered(number), format!("{number}. "));
        }
    }
    (DisplayKind::Bullet, "• ".into())
}

#[derive(Clone)]
struct Ink {
    italic: bool,
    weight: Option<u16>,
    strike: bool,
    code: bool,
    link: bool,
    color: Color,
}

impl Ink {
    fn plain(color: Color) -> Self {
        Self {
            italic: false,
            weight: None,
            strike: false,
            code: false,
            link: false,
            color,
        }
    }

    fn marked(&self) -> bool {
        self.italic || self.weight.is_some() || self.strike || self.code || self.link
    }
}

struct Acc {
    base: usize,
    text: String,
    segments: Vec<Segment>,
    spans: Vec<TextSpan>,
    palette: Palette,
    cursor: usize,
}

impl Acc {
    fn new(base: usize, palette: &Palette) -> Self {
        Self {
            base,
            text: String::new(),
            segments: Vec::new(),
            spans: Vec::new(),
            palette: *palette,
            cursor: base,
        }
    }

    fn skip(&mut self, width: usize) {
        self.cursor += width;
    }

    fn push_chrome(&mut self, chrome: &str) {
        self.text.push_str(chrome);
    }

    fn push_shown(&mut self, shown: &str, source_len: usize) {
        if shown.is_empty() && source_len == 0 {
            return;
        }
        let display_start = self.text.len();
        self.text.push_str(shown);
        let source_start = self.cursor - self.base;
        self.cursor += source_len;
        if !shown.is_empty() {
            self.segments.push(Segment {
                display_start,
                display_end: self.text.len(),
                source_start,
                source_end: source_start + source_len,
            });
        }
    }

    fn push_styled(&mut self, shown: &str, source_len: usize, ink: &Ink) {
        let start = self.text.len();
        self.push_shown(shown, source_len);
        if shown.is_empty() || !ink.marked() {
            return;
        }
        let mut span = TextSpan::new(start, self.text.len(), ink.color);
        span.italic = ink.italic;
        span.weight = ink.weight;
        span.strike = ink.strike;
        span.underline = ink.link;
        if ink.code {
            span.family = Some("monospace".into());
            span.highlight = Some(self.palette.code);
        }
        if ink.link {
            span.color = self.palette.link;
        }
        self.spans.push(span);
    }

    fn finish(mut self, _kind: SyntaxKind, span: Range<usize>) -> DisplayBlock {
        if self.text.ends_with('\n') {
            self.text.pop();
            if let Some(seg) = self.segments.last_mut() {
                if seg.display_end == self.text.len() + 1 {
                    if seg.display_start == self.text.len() {
                        self.cursor = self.base + seg.source_start;
                        self.segments.pop();
                    } else {
                        seg.display_end -= 1;
                        if seg.source_end > seg.source_start {
                            seg.source_end -= 1;
                        }
                        self.cursor = self.base + seg.source_end;
                    }
                }
            }
            self.spans.retain(|span| span.start < self.text.len());
            for span in &mut self.spans {
                if span.end > self.text.len() {
                    span.end = self.text.len();
                }
            }
        }
        let caret_end = self.cursor - self.base;
        DisplayBlock {
            span,
            kind: DisplayKind::Paragraph,
            chrome: String::new(),
            text: self.text,
            segments: self.segments,
            spans: self.spans,
            marker: None,
            depth: 0,
            quote: 0,
            height: 0.0,
            caret_end,
        }
    }
}

fn flow_child(bytes: &str, node: &Green, acc: &mut Acc) {
    walk_inline(bytes, node, acc, &Ink::plain(acc.palette.text));
}

fn walk_inline(bytes: &str, node: &Green, acc: &mut Acc, ink: &Ink) {
    if hide_inline(node) {
        acc.skip(node.width());
        return;
    }
    if node.children().is_empty() {
        let shown = shown_leaf(node.kind(), node.text());
        acc.push_styled(&shown, node.width(), ink);
        return;
    }
    let mut next = ink.clone();
    match node.kind() {
        SyntaxKind::Emphasis => next.italic = true,
        SyntaxKind::Strong => next.weight = Some(FONT_WEIGHT_BOLD),
        SyntaxKind::Strikethrough => next.strike = true,
        SyntaxKind::InlineCode => next.code = true,
        SyntaxKind::Link | SyntaxKind::Autolink => next.link = true,
        SyntaxKind::Image => {
            let alt = image_alt(bytes, node, acc.cursor);
            acc.push_styled(&alt, 0, &{
                let mut ink = next.clone();
                ink.italic = true;
                ink
            });
            // The destination and markers stay in the source. Skip the node.
            acc.skip(node.width());
            return;
        }
        _ => {}
    }
    let mut child_at_skip_dest = false;
    if matches!(node.kind(), SyntaxKind::Link | SyntaxKind::Autolink) {
        child_at_skip_dest = true;
    }
    let mut seen_close = false;
    for child in node.children() {
        if child_at_skip_dest && seen_close {
            acc.skip(child.width());
            continue;
        }
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" {
            seen_close = true;
            acc.skip(child.width());
            continue;
        }
        walk_inline(bytes, child, acc, &next);
    }
}

fn hide_inline(node: &Green) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::Delimiter
            | SyntaxKind::AtxMarker
            | SyntaxKind::SetextUnderline
            | SyntaxKind::LinePrefix
            | SyntaxKind::ListMarker
            | SyntaxKind::Pipe
            | SyntaxKind::Bom
    )
}

fn shown_leaf(kind: SyntaxKind, text: &str) -> String {
    match kind {
        SyntaxKind::Escape => text
            .chars()
            .nth(1)
            .map(|ch| ch.to_string())
            .unwrap_or_else(|| text.to_string()),
        SyntaxKind::Entity => decode_entity(text),
        SyntaxKind::SoftBreak | SyntaxKind::HardBreak | SyntaxKind::LineEnding => "\n".into(),
        SyntaxKind::Autolink => text.trim_matches(['<', '>']).to_string(),
        _ => text.to_string(),
    }
}

fn decode_entity(raw: &str) -> String {
    let inner = raw.trim_start_matches('&').trim_end_matches(';');
    let code = if let Some(hex) = inner
        .strip_prefix("#x")
        .or_else(|| inner.strip_prefix("#X"))
    {
        u32::from_str_radix(hex, 16).ok()
    } else if let Some(dec) = inner.strip_prefix('#') {
        dec.parse().ok()
    } else {
        match inner {
            "amp" => Some('&' as u32),
            "lt" => Some('<' as u32),
            "gt" => Some('>' as u32),
            "quot" => Some('"' as u32),
            "apos" => Some('\'' as u32),
            "nbsp" => Some(0xA0),
            _ => None,
        }
    };
    code.and_then(char::from_u32)
        .map(|ch| ch.to_string())
        .unwrap_or_else(|| raw.to_string())
}

fn image_alt(bytes: &str, node: &Green, at: usize) -> String {
    let mut alt = String::new();
    let mut cursor = at;
    let mut open = true;
    for child in node.children() {
        let end = cursor + child.width();
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" {
            open = false;
        } else if open && child.kind() != SyntaxKind::Delimiter {
            if child.children().is_empty() {
                alt.push_str(&shown_leaf(child.kind(), child.text()));
            } else if let Some(slice) = bytes.get(cursor..end) {
                alt.push_str(slice);
            }
        }
        cursor = end;
    }
    if alt.is_empty() {
        "image".into()
    } else {
        alt
    }
}

/// Index of the block whose span contains `source`, or the nearest later block.
pub fn block_index(blocks: &[DisplayBlock], source: usize) -> Option<usize> {
    let mut found = None;
    for (index, block) in blocks.iter().enumerate() {
        if block.kind == DisplayKind::Gap {
            continue;
        }
        if block.span.start <= source && source <= block.span.end {
            found = Some(index);
        }
    }
    found.or_else(|| {
        blocks
            .iter()
            .position(|block| block.kind != DisplayKind::Gap && block.span.start >= source)
    })
}
