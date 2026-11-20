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
    /// An image whose alt is the text. The picture itself is not loaded.
    Image {
        /// Alt bytes, or the embed name when the alt is empty.
        alt: String,
    },
    /// An Obsidian callout. `tone` is the bracket word, lowercased.
    Callout {
        /// `[!tone]`.
        tone: String,
    },
    /// A footnote definition.
    Footnote {
        /// The id as written.
        id: String,
    },
    /// A `::block::` region. Fence lines stay hidden until the caret is inside.
    Styled,
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

/// The block as the caret sees it.
///
/// Markers of the inline span under `caret` are shown. Every other block is
/// the same projection [`project`] builds, with those markers hidden.
pub fn present(
    block: &DisplayBlock,
    bytes: &str,
    root: &Green,
    caret: usize,
    palette: &Palette,
) -> DisplayBlock {
    if caret < block.span.start || caret >= block.span.end {
        return block.clone();
    }
    let Some(node) = find_span(root, 0, block.span.start, block.span.end) else {
        return block.clone();
    };
    let mut out = Vec::new();
    project_node(
        bytes,
        node,
        block.span.start,
        Ctx {
            depth: block.depth,
            quote: block.quote,
        },
        palette,
        Some(caret),
        &mut out,
    );
    let Some(mut shown) = out.into_iter().next() else {
        return block.clone();
    };
    if block.marker.is_some() {
        shown.kind = block.kind.clone();
        shown.chrome.clone_from(&block.chrome);
        shown.marker.clone_from(&block.marker);
        shown.depth = block.depth;
        shown.quote = block.quote;
    }
    shown
}

fn find_span(node: &Green, at: usize, start: usize, end: usize) -> Option<&Green> {
    let node_end = at + node.width();
    if at == start && node_end == end {
        return Some(node);
    }
    if start < at || end > node_end {
        return None;
    }
    let mut child_at = at;
    for child in node.children() {
        if let Some(found) = find_span(child, child_at, start, end) {
            return Some(found);
        }
        child_at += child.width();
    }
    None
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
        project_node(self.bytes, node, at, ctx, self.palette, None, self.out);
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
    focus: Option<usize>,
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
        SyntaxKind::Heading => project_heading(bytes, node, at, palette, focus),
        SyntaxKind::FencedCode => project_fence(bytes, node, at, false),
        SyntaxKind::LiquidFence => project_fence(bytes, node, at, true),
        SyntaxKind::Table => project_table(bytes, node, at, palette),
        SyntaxKind::Callout => project_callout(bytes, node, at, focus),
        SyntaxKind::FootnoteDef => project_footnote(bytes, node, at, focus),
        SyntaxKind::StyledBlock => project_styled(bytes, node, at, palette, focus),
        SyntaxKind::HtmlBlock
        | SyntaxKind::IndentedCode
        | SyntaxKind::OpaqueBlock
        | SyntaxKind::LinkRefDef => project_raw(bytes, node, at),
        _ => project_flow(bytes, node, at, palette, focus),
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

fn project_heading(
    bytes: &str,
    node: &Green,
    at: usize,
    palette: &Palette,
    focus: Option<usize>,
) -> DisplayBlock {
    let mut block = project_flow(bytes, node, at, palette, focus);
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

fn project_flow(
    bytes: &str,
    node: &Green,
    at: usize,
    palette: &Palette,
    focus: Option<usize>,
) -> DisplayBlock {
    let mut acc = Acc::new(at, palette, focus);
    let children = node.children();
    for (index, child) in children.iter().enumerate() {
        let last = index + 1 == children.len();
        if last && child.kind() == SyntaxKind::LineEnding {
            acc.skip(child.width());
            continue;
        }
        flow_child(bytes, child, &mut acc);
    }
    let mut block = acc.finish(node.kind(), at..at + node.width());
    let focused = focus.is_some_and(|caret| at <= caret && caret < at + node.width());
    if !focused {
        if let Some(alt) = lone_image_alt(node) {
            block.kind = DisplayKind::Image { alt };
        }
    }
    block
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
    let mut acc = Acc::new(at, palette, None);
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
    if let Some(checked) = task_checked(trimmed) {
        let chrome = if checked { "☑ " } else { "☐ " };
        return (DisplayKind::Task(checked), chrome.into());
    }
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if let Ok(number) = digits.parse::<u64>() {
        if !digits.is_empty() {
            return (DisplayKind::Ordered(number), format!("{number}. "));
        }
    }
    (DisplayKind::Bullet, "• ".into())
}

/// A bullet marker whose checkbox is exactly `[ ]`, `[x]`, or `[X]`.
fn task_checked(trimmed: &str) -> Option<bool> {
    let bytes = trimmed.as_bytes();
    if bytes
        .first()
        .is_none_or(|b| !matches!(b, b'-' | b'+' | b'*'))
    {
        return None;
    }
    let rest = trimmed[1..].trim_start();
    let box_bytes = rest.as_bytes();
    if box_bytes.len() < 3 || box_bytes[0] != b'[' || box_bytes[2] != b']' {
        return None;
    }
    if !matches!(box_bytes[1], b' ' | b'x' | b'X') {
        return None;
    }
    let after = &rest[3..];
    if !(after.is_empty() || after.starts_with([' ', '\t'])) {
        return None;
    }
    Some(box_bytes[1] == b'x' || box_bytes[1] == b'X')
}

#[derive(Clone)]
struct Ink {
    italic: bool,
    weight: Option<u16>,
    strike: bool,
    code: bool,
    link: bool,
    color: Color,
    highlight: Option<Color>,
    family: Option<String>,
    custom: bool,
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
            highlight: None,
            family: None,
            custom: false,
        }
    }

    fn marked(&self) -> bool {
        self.italic
            || self.weight.is_some()
            || self.strike
            || self.code
            || self.link
            || self.custom
            || self.highlight.is_some()
            || self.family.is_some()
    }
}

struct Acc {
    base: usize,
    text: String,
    segments: Vec<Segment>,
    spans: Vec<TextSpan>,
    palette: Palette,
    cursor: usize,
    focus: Option<usize>,
}

impl Acc {
    fn new(base: usize, palette: &Palette, focus: Option<usize>) -> Self {
        Self {
            base,
            text: String::new(),
            segments: Vec::new(),
            spans: Vec::new(),
            palette: *palette,
            cursor: base,
            focus,
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
        if let Some(family) = &ink.family {
            span.family = Some(family.clone());
        }
        if let Some(highlight) = ink.highlight {
            span.highlight = Some(highlight);
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
    walk_inline(bytes, node, acc, &Ink::plain(acc.palette.text), false);
}

fn walk_inline(_bytes: &str, node: &Green, acc: &mut Acc, ink: &Ink, reveal: bool) {
    let start = acc.cursor;
    let width = node.width();
    let active = acc
        .focus
        .is_some_and(|caret| caret >= start && caret < start + width);
    if node.kind() == SyntaxKind::Delimiter {
        if reveal {
            let mut muted = ink.clone();
            muted.color = acc.palette.muted;
            muted.link = false;
            muted.custom = true;
            acc.push_styled(node.text(), width, &muted);
        } else {
            acc.skip(width);
        }
        return;
    }
    if hide_inline(node) {
        acc.skip(width);
        return;
    }
    if node.children().is_empty() {
        let shown = if active && node.kind() == SyntaxKind::Autolink {
            node.text().to_string()
        } else {
            shown_leaf(node.kind(), node.text())
        };
        let paint = if node.kind() == SyntaxKind::Autolink {
            let mut link = ink.clone();
            link.link = true;
            link
        } else {
            ink.clone()
        };
        acc.push_styled(&shown, width, &paint);
        return;
    }
    let mut next = ink.clone();
    match node.kind() {
        SyntaxKind::Image if !active => {
            emit_image_chip(node, acc);
            return;
        }
        SyntaxKind::WikiEmbed if !active => {
            emit_embed_chip(node, acc);
            return;
        }
        SyntaxKind::WikiLink if !active => {
            next.link = true;
            emit_named_chip(node, acc, &next, wiki_visible(node));
            return;
        }
        SyntaxKind::FootnoteRef if !active => {
            next.link = true;
            emit_named_chip(node, acc, &next, footnote_visible(node));
            return;
        }
        SyntaxKind::FootnoteInline if !active => {
            next.link = true;
            emit_named_chip(node, acc, &next, text_child(node));
            return;
        }
        SyntaxKind::Emphasis => next.italic = true,
        SyntaxKind::Strong => next.weight = Some(FONT_WEIGHT_BOLD),
        SyntaxKind::Strikethrough => next.strike = true,
        SyntaxKind::InlineCode => next.code = true,
        SyntaxKind::Link
        | SyntaxKind::Autolink
        | SyntaxKind::WikiLink
        | SyntaxKind::FootnoteRef
        | SyntaxKind::FootnoteInline => next.link = true,
        SyntaxKind::Highlight => {
            next.highlight = Some(Color::rgba(0.93, 0.78, 0.18, 0.45));
            next.custom = true;
        }
        SyntaxKind::Math => {
            next.family = Some("monospace".into());
            next.italic = true;
            next.custom = true;
        }
        SyntaxKind::StyleSpan => apply_style(&mut next, style_token(node)),
        _ => {}
    }
    let skip_dest = matches!(node.kind(), SyntaxKind::Link | SyntaxKind::Image) && !active;
    let mut seen_close = false;
    for child in node.children() {
        if skip_dest && seen_close {
            acc.skip(child.width());
            continue;
        }
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" && !active {
            seen_close = true;
            acc.skip(child.width());
            continue;
        }
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" {
            seen_close = true;
        }
        let child_reveal = active && child.kind() == SyntaxKind::Delimiter && is_mark(node.kind());
        walk_inline(_bytes, child, acc, &next, child_reveal);
    }
}

fn is_mark(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Emphasis
            | SyntaxKind::Strong
            | SyntaxKind::Strikethrough
            | SyntaxKind::InlineCode
            | SyntaxKind::Link
            | SyntaxKind::Image
            | SyntaxKind::Highlight
            | SyntaxKind::Math
            | SyntaxKind::StyleSpan
            | SyntaxKind::WikiLink
            | SyntaxKind::WikiEmbed
            | SyntaxKind::FootnoteRef
            | SyntaxKind::FootnoteInline
    )
}

fn hide_inline(node: &Green) -> bool {
    matches!(
        node.kind(),
        SyntaxKind::AtxMarker
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

fn emit_image_chip(node: &Green, acc: &mut Acc) {
    let (skip, len, alt) = image_alt_span(node);
    acc.skip(skip);
    acc.push_chrome("🖼 ");
    let mut ink = Ink::plain(acc.palette.text);
    ink.highlight = Some(acc.palette.code);
    ink.custom = true;
    let empty = alt.is_empty();
    let shown = if empty { "Image".to_string() } else { alt };
    let source_len = if empty { 0 } else { len };
    acc.push_styled(&shown, source_len, &ink);
    let rest = node.width().saturating_sub(skip + source_len);
    acc.skip(rest);
}

fn image_alt_span(node: &Green) -> (usize, usize, String) {
    let mut at = 0;
    let mut start = None;
    let mut end = 0;
    let mut alt = String::new();
    let mut open = true;
    for child in node.children() {
        let child_end = at + child.width();
        if child.kind() == SyntaxKind::Delimiter && child.text() == "]" {
            open = false;
        } else if open && child.kind() != SyntaxKind::Delimiter {
            if start.is_none() {
                start = Some(at);
            }
            end = child_end;
            if child.children().is_empty() {
                alt.push_str(&shown_leaf(child.kind(), child.text()));
            } else {
                alt.push_str(&child.concat());
            }
        }
        at = child_end;
    }
    let start = start.unwrap_or(0);
    (start, end.saturating_sub(start), alt)
}

fn emit_embed_chip(node: &Green, acc: &mut Acc) {
    let image = embed_is_image(node);
    let (label, skip, len) = embed_visible(node);
    if image {
        acc.skip(skip);
        acc.push_chrome("🖼 ");
        let mut ink = Ink::plain(acc.palette.text);
        ink.highlight = Some(acc.palette.code);
        ink.custom = true;
        let shown = if label.is_empty() {
            "Image".into()
        } else {
            label
        };
        acc.push_styled(&shown, len, &ink);
        acc.skip(node.width().saturating_sub(skip + len));
        return;
    }
    let mut ink = Ink::plain(acc.palette.link);
    ink.link = true;
    emit_named_chip(node, acc, &ink, label);
}

fn emit_named_chip(node: &Green, acc: &mut Acc, ink: &Ink, label: String) {
    let (skip, len, shown) = matching_text(node, &label);
    acc.skip(skip);
    acc.push_styled(&shown, len, ink);
    acc.skip(node.width().saturating_sub(skip + len));
}

fn matching_text(node: &Green, label: &str) -> (usize, usize, String) {
    let mut at = 0;
    let mut first = None;
    for child in node.children() {
        let end = at + child.width();
        if child.kind() == SyntaxKind::Text {
            if child.text() == label {
                return (at, child.width(), label.to_string());
            }
            if first.is_none() {
                first = Some((at, child.width(), child.text().to_string()));
            }
        }
        at = end;
    }
    first.unwrap_or_else(|| (0, 0, label.to_string()))
}

fn text_child(node: &Green) -> String {
    node.children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Text)
        .map(|child| child.text().to_string())
        .unwrap_or_default()
}

fn wiki_visible(node: &Green) -> String {
    let texts: Vec<&str> = node
        .children()
        .iter()
        .filter(|child| child.kind() == SyntaxKind::Text)
        .map(Green::text)
        .collect();
    texts
        .get(1)
        .copied()
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| texts.first().copied().unwrap_or(""))
        .to_string()
}

fn footnote_visible(node: &Green) -> String {
    text_child(node)
}

fn embed_visible(node: &Green) -> (String, usize, usize) {
    let mut at = 0;
    let mut texts = Vec::new();
    for child in node.children() {
        let end = at + child.width();
        if child.kind() == SyntaxKind::Text {
            texts.push((at, child.width(), child.text().to_string()));
        }
        at = end;
    }
    let target = texts.first().cloned().unwrap_or((0, 0, String::new()));
    if let Some(extra) = texts.get(1) {
        if !extra.2.is_empty() && !is_image_size(&extra.2) {
            return (extra.2.clone(), extra.0, extra.1);
        }
    }
    let name = target
        .2
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&target.2)
        .to_string();
    (name, target.0, target.1)
}

fn embed_is_image(node: &Green) -> bool {
    node.children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Text)
        .is_some_and(|child| is_image_target(child.text()))
}

fn is_image_target(target: &str) -> bool {
    let path = target.split(['?', '#']).next().unwrap_or(target);
    let lower = path.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".bmp", ".ico", ".heic", ".heif", ".avif",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

fn is_image_size(raw: &str) -> bool {
    let token = raw.trim();
    let lower = token.to_ascii_lowercase();
    let (width, height) = match lower.split_once('x') {
        Some((width, height)) => (width, Some(height)),
        None => (lower.as_str(), None),
    };
    positive_dim(width) && height.is_none_or(positive_dim)
}

fn positive_dim(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= 4
        && raw.bytes().all(|b| b.is_ascii_digit())
        && raw.bytes().any(|b| b != b'0')
}

fn style_token(node: &Green) -> &str {
    node.children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Delimiter && child.text().starts_with("{{"))
        .and_then(|child| {
            child
                .text()
                .strip_prefix("{{")
                .and_then(|token| token.strip_suffix('|'))
        })
        .unwrap_or("")
}

fn apply_style(ink: &mut Ink, token: &str) {
    let lower = token.to_ascii_lowercase();
    if let Some(color) = named_color(&lower) {
        ink.color = color;
        ink.custom = true;
        return;
    }
    if let Some(color) = hex_color(&lower) {
        ink.color = color;
        ink.custom = true;
        return;
    }
    if let Some(font) = lower.strip_prefix("font:") {
        ink.family = Some(
            match font {
                "serif" => "serif",
                "mono" => "monospace",
                _ => "sans-serif",
            }
            .to_string(),
        );
        ink.custom = true;
        return;
    }
    if let Some(size) = lower.strip_prefix("size:") {
        ink.weight = Some(size_weight(size));
        ink.custom = true;
    }
}

fn named_color(name: &str) -> Option<Color> {
    Some(match name {
        "red" => Color::rgb(0.97, 0.44, 0.44),
        "orange" => Color::rgb(0.98, 0.57, 0.24),
        "yellow" => Color::rgb(0.98, 0.80, 0.08),
        "green" => Color::rgb(0.29, 0.87, 0.50),
        "blue" => Color::rgb(0.38, 0.65, 0.98),
        "purple" => Color::rgb(0.75, 0.52, 0.99),
        "pink" => Color::rgb(0.96, 0.45, 0.71),
        _ => return None,
    })
}

fn hex_color(token: &str) -> Option<Color> {
    let hex = token.strip_prefix('#')?;
    let expanded = match hex.len() {
        3 | 4 => hex
            .chars()
            .take(3)
            .flat_map(|ch| [ch, ch])
            .collect::<String>(),
        6 | 8 => hex.chars().take(6).collect(),
        _ => return None,
    };
    if expanded.len() != 6 || !expanded.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&expanded[0..2], 16).ok()?;
    let g = u8::from_str_radix(&expanded[2..4], 16).ok()?;
    let b = u8::from_str_radix(&expanded[4..6], 16).ok()?;
    Some(Color::rgb(
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ))
}

fn size_weight(token: &str) -> u16 {
    match token {
        "sm" => 400,
        "md" => 500,
        "lg" | "xl" => FONT_WEIGHT_BOLD,
        _ => {
            let digits: String = token.chars().take_while(|c| c.is_ascii_digit()).collect();
            let value: f32 = digits.parse().unwrap_or(16.0);
            if value >= 20.0 {
                FONT_WEIGHT_BOLD
            } else {
                500
            }
        }
    }
}

fn lone_image_alt(node: &Green) -> Option<String> {
    let mut alt = None;
    for child in node.children() {
        match child.kind() {
            SyntaxKind::LineEnding | SyntaxKind::SoftBreak => {}
            SyntaxKind::Text if child.text().trim().is_empty() => {}
            SyntaxKind::Image => {
                if alt.is_some() {
                    return None;
                }
                let (_, _, text) = image_alt_span(child);
                alt = Some(if text.is_empty() {
                    "Image".into()
                } else {
                    text
                });
            }
            SyntaxKind::WikiEmbed if embed_is_image(child) => {
                if alt.is_some() {
                    return None;
                }
                alt = Some(embed_visible(child).0);
            }
            _ => return None,
        }
    }
    alt
}

fn project_callout(bytes: &str, node: &Green, at: usize, focus: Option<usize>) -> DisplayBlock {
    let span = at..at + node.width();
    let raw = bytes.get(span.clone()).unwrap_or("");
    let (tone, title) = callout_parts(raw);
    let active = focus.is_some_and(|caret| span.start <= caret && caret < span.end);
    if active {
        return raw_block(span, raw, DisplayKind::Callout { tone });
    }
    let mut text = String::new();
    let mut segments = Vec::new();
    let mut offset = 0;
    let raw_bytes = raw.as_bytes();
    let mut first = true;
    while offset < raw.len() {
        let line_start = offset;
        let mut line_end = offset;
        while line_end < raw.len() && raw_bytes[line_end] != b'\n' && raw_bytes[line_end] != b'\r' {
            line_end += 1;
        }
        let nl_end = if line_end < raw.len() {
            if raw_bytes[line_end] == b'\r'
                && line_end + 1 < raw.len()
                && raw_bytes[line_end + 1] == b'\n'
            {
                line_end + 2
            } else {
                line_end + 1
            }
        } else {
            line_end
        };
        let line = &raw[line_start..line_end];
        let mut content = line_start + quote_content_offset(line);
        if first {
            if let Some(skip) = callout_marker_len(&raw[content..line_end]) {
                content += skip;
            }
            first = false;
        }
        push_mapped(&mut text, &mut segments, &raw[content..line_end], content);
        if nl_end > line_end && nl_end < raw.len() {
            text.push('\n');
        }
        offset = nl_end;
    }
    if text.ends_with('\n') {
        text.pop();
    }
    let mut block = empty_paragraph(span);
    block.kind = DisplayKind::Callout { tone: tone.clone() };
    block.chrome = format!("{}  ", callout_label(&tone));
    block.text = if text.is_empty() { title } else { text };
    block.segments = segments;
    block.caret_end = raw.trim_end_matches(['\n', '\r']).len();
    block
}

fn project_footnote(bytes: &str, node: &Green, at: usize, focus: Option<usize>) -> DisplayBlock {
    let span = at..at + node.width();
    let raw = bytes.get(span.clone()).unwrap_or("");
    let id = node
        .children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Delimiter)
        .map(Green::text)
        .unwrap_or("")
        .trim_start_matches("[^")
        .trim_end_matches(':')
        .trim_end_matches(']')
        .to_string();
    let active = focus.is_some_and(|caret| span.start <= caret && caret < span.end);
    if active {
        return raw_block(span, raw, DisplayKind::Footnote { id: id.clone() });
    }
    let marker = node
        .children()
        .iter()
        .find(|child| child.kind() == SyntaxKind::Delimiter)
        .map(Green::width)
        .unwrap_or(0);
    let after = raw.get(marker..).unwrap_or("");
    let lead = after.len() - after.trim_start_matches([' ', '\t']).len();
    let body = after[lead..].trim_end_matches(['\n', '\r']);
    let source_start = marker + lead;
    let source_end = source_start + body.len();
    let mut block = empty_paragraph(span);
    block.kind = DisplayKind::Footnote { id: id.clone() };
    block.chrome = format!("{id}. ");
    block.text = body.to_string();
    if !body.is_empty() {
        block.segments.push(Segment {
            display_start: 0,
            display_end: body.len(),
            source_start,
            source_end,
        });
    }
    block.caret_end = source_end;
    block
}

fn project_styled(
    bytes: &str,
    node: &Green,
    at: usize,
    palette: &Palette,
    focus: Option<usize>,
) -> DisplayBlock {
    let span = at..at + node.width();
    let raw = bytes.get(span.clone()).unwrap_or("");
    let active = focus.is_some_and(|caret| span.start <= caret && caret < span.end);
    if active {
        return raw_block(span, raw, DisplayKind::Styled);
    }
    let mut acc = Acc::new(at, palette, None);
    for child in node.children() {
        if child.kind() == SyntaxKind::FenceLine {
            acc.skip(child.width());
            continue;
        }
        flow_child(bytes, child, &mut acc);
        if !acc.text.ends_with('\n') {
            acc.push_shown("\n", 0);
        }
    }
    let mut block = acc.finish(SyntaxKind::StyledBlock, span);
    block.kind = DisplayKind::Styled;
    block.chrome = "Block  ".into();
    block
}

fn raw_block(span: Range<usize>, raw: &str, kind: DisplayKind) -> DisplayBlock {
    let text = raw.trim_end_matches(['\n', '\r']).to_string();
    let mut block = empty_paragraph(span);
    block.kind = kind;
    block.text.clone_from(&text);
    if !text.is_empty() {
        block.segments.push(Segment {
            display_start: 0,
            display_end: text.len(),
            source_start: 0,
            source_end: text.len(),
        });
    }
    block.caret_end = text.len();
    block
}

fn callout_parts(raw: &str) -> (String, String) {
    let line = raw.lines().next().unwrap_or("");
    let rest = line.trim_start();
    let rest = rest.strip_prefix('>').unwrap_or(rest).trim_start();
    let Some(end) = rest.find(']') else {
        return ("note".into(), String::new());
    };
    if !rest.starts_with("[!") || end < 2 {
        return ("note".into(), String::new());
    }
    let tone = rest[2..end].to_ascii_lowercase();
    let title = rest[end + 1..].trim().to_string();
    (tone, title)
}

fn callout_label(tone: &str) -> String {
    let known = match tone {
        "note" => "Note",
        "abstract" | "summary" | "tldr" => "Abstract",
        "info" => "Info",
        "todo" => "Todo",
        "tip" | "hint" | "important" => "Tip",
        "success" | "check" | "done" => "Success",
        "question" | "help" | "faq" => "Question",
        "warning" | "caution" | "attention" => "Warning",
        "failure" | "fail" | "missing" => "Failure",
        "danger" | "error" => "Danger",
        "bug" => "Bug",
        "example" => "Example",
        "quote" | "cite" => "Quote",
        _ => "",
    };
    if !known.is_empty() {
        return known.to_string();
    }
    let mut chars = tone.chars();
    let Some(first) = chars.next() else {
        return "Note".into();
    };
    first.to_uppercase().collect::<String>() + chars.as_str()
}

fn quote_content_offset(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut i = 0;
    let mut spaces = 0;
    while i < bytes.len() && bytes[i] == b' ' && spaces < 3 {
        i += 1;
        spaces += 1;
    }
    if i < bytes.len() && bytes[i] == b'>' {
        i += 1;
        if i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
    }
    i
}

fn callout_marker_len(rest: &str) -> Option<usize> {
    let bytes = rest.as_bytes();
    if bytes.len() < 4 || bytes[0] != b'[' || bytes[1] != b'!' {
        return None;
    }
    let end = rest.find(']')?;
    let mut len = end + 1;
    if rest.as_bytes().get(len) == Some(&b' ') {
        len += 1;
    }
    Some(len)
}

fn push_mapped(text: &mut String, segments: &mut Vec<Segment>, shown: &str, source_start: usize) {
    if shown.is_empty() {
        return;
    }
    let display_start = text.len();
    text.push_str(shown);
    segments.push(Segment {
        display_start,
        display_end: text.len(),
        source_start,
        source_end: source_start + shown.len(),
    });
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
