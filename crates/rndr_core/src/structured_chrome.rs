//! Revision-fenced structural chrome, never inferred from display prefix bytes.
use crate::structured::{BlockId, BlockKind, ContainerKind, ListKind};
use crate::structured_native::{structured_display, StructuredDocuments};
use crate::*;

/// Opt in to depth-aware paragraph layout and noneditable structural chrome.
/// The host reserves a small margin immediately left of the editor; markers
/// occupy the indentation before their paragraph, not a fixed marker column.
#[derive(Clone, Copy, Debug)]
pub struct StructuredChromeGutter {
    /// Positive finite reserved width in logical points.
    pub width: f32,
}
impl Component for StructuredChromeGutter {}

/// Explicit pointer ownership for the separate reserved gutter entity.
/// Hosts must remove this link when detaching or reparenting the editor.
#[derive(Clone, Copy, Debug)]
pub struct StructuredChromeTarget(pub Entity);
impl Component for StructuredChromeTarget {}

/// Authoritative semantic purpose of a projected mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredMarkKind {
    /// Unordered list item.
    Bullet,
    /// Explicit list ordinal, including the list's configured start.
    Ordered(u32),
    /// Task state; hosts may toggle the owning block using a fenced command.
    Task(bool),
    /// Quote ancestry bar.
    Quote,
    /// Whole-node horizontal rule.
    HorizontalRule,
}
/// One noneditable structural mark; pointer hosts use the same clipped geometry as paint.
#[derive(Clone, Debug)]
pub struct StructuredMark {
    /// Owning leaf identity, never a display offset.
    pub block: BlockId,
    /// Exact revision required for any subsequent host action.
    pub revision: crate::editing::Revision,
    /// Typed semantic purpose.
    pub kind: StructuredMarkKind,
    /// Absolute logical-point hit rectangle.
    pub rect: Rect,
    /// Parent clipping, restricted to this editor and its small left reservation.
    pub clip: Clip,
}

/// Logical indentation step shared by text, chrome and native hit geometry.
pub const STRUCTURED_INDENT: f32 = 24.0;

pub(crate) fn layout_key(world: &World, view: Entity) -> u64 {
    if !world.has::<StructuredChromeGutter>(view) {
        return 0;
    }
    let Some(handle) = crate::structured_native::attached_structured(world, view) else {
        return 0;
    };
    let Some(doc) = world
        .resource::<StructuredDocuments>()
        .and_then(|d| d.get(handle))
    else {
        return 0;
    };
    doc.blocks().iter().fold(0xE610_u64, |key, block| {
        let depth = doc
            .ancestors(block.id)
            .unwrap_or_default()
            .iter()
            .filter(|c| matches!(c.kind, ContainerKind::Quote | ContainerKind::Item { .. }))
            .count();
        key.rotate_left(7) ^ depth as u64 ^ block.len() as u64
    })
}

fn indent(depth: usize, width: f32) -> f32 {
    (depth as f32 * STRUCTURED_INDENT).min((width - 48.0).max(0.0))
}

// Shape each logical paragraph at its own available width. Shift both glyphs
// and caret edges: paint, selection, pointer, keyboard, IME and accessibility
// consequently consume one canonical layout, never synthetic buffer prefixes.
pub(crate) fn text_layout(
    world: &World,
    view: Entity,
    spans: &[TextSpan],
    text: &str,
    width: f32,
    (base, preview_at, preview_len): (usize, usize, usize),
) -> Option<TextLayout> {
    if !world.has::<StructuredChromeGutter>(view) {
        return None;
    }
    let handle = crate::structured_native::attached_structured(world, view)?;
    let doc = world.resource::<StructuredDocuments>()?.get(handle)?;
    let engine = crate::text::text_engine(world);
    let style = crate::text::style_for(world, view);
    let mut offset = 0;
    let paragraphs: Vec<_> = doc
        .blocks()
        .iter()
        .map(|block| {
            let start = offset;
            offset += block.len() + 1;
            let depth = doc
                .ancestors(block.id)
                .unwrap_or_default()
                .iter()
                .filter(|c| matches!(c.kind, ContainerKind::Quote | ContainerKind::Item { .. }))
                .count();
            (start, depth)
        })
        .collect();
    let mut result = TextLayout::shaped(vec![], vec![], 0.0, 0.0);
    let mut start = 0;
    for paragraph in text.split('\n') {
        let source = if start <= preview_at {
            start
        } else {
            start.saturating_sub(preview_len).max(preview_at)
        } + base;
        let paragraph_index = paragraphs
            .partition_point(|(at, _)| *at <= source)
            .saturating_sub(1);
        let depth = paragraphs
            .get(paragraph_index)
            .map_or(0, |(_, depth)| *depth);
        let x = indent(depth, width);
        let end = start + paragraph.len();
        let local_spans: Vec<_> = spans
            .iter()
            .filter_map(|s| {
                if s.start >= end || s.end <= start {
                    return None;
                }
                let mut s = s.clone();
                s.start = s.start.saturating_sub(start);
                s.end = s.end.min(end) - start;
                Some(s)
            })
            .collect();
        let mut part = engine.layout(paragraph, &style, &local_spans, (width - x).max(1.0));
        let y = result.height;
        for glyph in &mut part.glyphs {
            glyph.start += start;
            glyph.end += start;
            glyph.x += x;
            glyph.y += y;
        }
        for line in &mut part.lines {
            line.start += start;
            line.end += start;
            line.y += y;
            for edge in &mut line.edges {
                edge.0 += start;
                edge.1 += x;
            }
        }
        for highlight in &mut part.highlights {
            highlight.x += x;
            highlight.y += y;
        }
        result.width = result.width.max(x + part.width);
        result.height += part.height;
        result.glyphs.extend(part.glyphs);
        result.lines.extend(part.lines);
        result.highlights.extend(part.highlights);
        start = end + 1;
    }
    Some(result)
}

// Match the shape system's cache fence. Reject before layout catches up after
// a projection, width/style/span change, or raw Label mutation.
fn fresh_layout(world: &World, view: Entity) -> bool {
    let (Some(label), Some(layout), Some(rect)) = (
        world.get::<Label>(view),
        world.get::<TextLayout>(view),
        world.get::<Rect>(view),
    ) else {
        return false;
    };
    if world.has::<ImePreview>(view) {
        return false;
    }
    let engine = crate::text::text_engine(world);
    let style = crate::text::style_for(world, view);
    let width = crate::label_box(world, view).text_width(rect.w, engine.unit_advance(&style));
    let spans = crate::editor::spans_for_text(world, view, &label.text);
    let at = world
        .get::<Caret>(view)
        .map_or(label.text.len(), |c| c.index.min(label.text.len()));
    let mut key = crate::font::layout_cache_key(
        crate::font::text_face(world).as_ref(),
        &label.text,
        width,
        &spans,
        "",
        at,
    );
    key ^= crate::text::style_key(&style);
    key ^= layout_key(world, view);
    key ^= match label.overflow {
        TextOverflow::Visible => 0,
        TextOverflow::Wrap => 0xA5A5_A5A5_A5A5_A5A5,
        TextOverflow::Ellipsis => return false,
    };
    key ^= engine.cache_epoch().wrapping_mul(0x9E37_79B9_7F4A_7C15);
    layout.cache_key == key
}

/// Read current structural chrome without mutating ECS, buffers, or the document.
/// Detached, stale, composing, unshaped and non-opted-in views return no marks.
/// Coordinates use native editor rows, including wrapping and scrolling.
pub fn structured_marks(world: &World, view: Entity) -> Vec<StructuredMark> {
    let Some(gutter) = world.get::<StructuredChromeGutter>(view) else {
        return vec![];
    };
    if !gutter.width.is_finite()
        || gutter.width <= 0.0
        || !world.has::<Editor>(view)
        || !fresh_layout(world, view)
    {
        return vec![];
    }
    let Some(display) = structured_display(world, view) else {
        return vec![];
    };
    let base = crate::editor::window_base(world, view);
    let Some(label) = world.get::<Label>(view) else {
        return vec![];
    };
    if display
        .text
        .get(base..base.saturating_add(label.text.len()))
        != Some(label.text.as_str())
    {
        return vec![];
    }
    let Some(doc) = world
        .resource::<StructuredDocuments>()
        .and_then(|d| d.get(display.document))
    else {
        return vec![];
    };
    if doc.is_composing()
        || world
            .resource::<Input>()
            .is_some_and(|i| !i.ime_preedit.is_empty())
    {
        return vec![];
    }
    let Some(rect) = world.get::<Rect>(view).copied() else {
        return vec![];
    };
    let reserved = Clip {
        x: rect.x - gutter.width,
        y: rect.y,
        w: rect.w + gutter.width,
        h: rect.h,
    };
    let parent_clip = world
        .get::<Parent>(view)
        .and_then(|p| world.get::<Clip>(p.entity))
        .copied()
        .unwrap_or_else(|| world.get::<Clip>(view).copied().unwrap_or(reserved));
    let clip = reserved.intersect(parent_clip);
    let mut marks = vec![];
    let mut seen_items = std::collections::HashSet::new();
    let mut ordinals = std::collections::HashMap::new();
    for block in &display.blocks {
        let Ok(ancestors) = doc.ancestors(block.block) else {
            continue;
        };
        let mut pending = vec![];
        let mut depth = 0usize;
        for c in ancestors.iter().rev() {
            match c.kind {
                ContainerKind::Quote => {
                    pending.push((depth, StructuredMarkKind::Quote));
                    depth += 1;
                }
                ContainerKind::Item { checked } => {
                    // Continuation blocks retain indentation/quote bars but no repeated item marker.
                    if seen_items.insert(c.id) {
                        if let Some(list) = c
                            .parent
                            .and_then(|id| doc.containers().iter().find(|p| p.id == id))
                        {
                            let kind = match list.kind {
                                ContainerKind::List(ListKind::Bullet) => {
                                    Some(StructuredMarkKind::Bullet)
                                }
                                ContainerKind::List(ListKind::Task) => {
                                    Some(StructuredMarkKind::Task(checked.unwrap_or(false)))
                                }
                                ContainerKind::List(ListKind::Ordered { start }) => {
                                    let n = ordinals.entry(list.id).or_insert(start);
                                    let kind = StructuredMarkKind::Ordered(*n);
                                    *n = n.saturating_add(1);
                                    Some(kind)
                                }
                                _ => None,
                            };
                            if let Some(kind) = kind {
                                pending.push((depth, kind));
                            }
                        }
                    }
                    depth += 1;
                }
                _ => {}
            }
        }
        let Some(row) = crate::source_folding::source_position_box(world, view, block.range.start)
        else {
            continue;
        };
        let y = rect.y + TEXT_ORIGIN_Y + row.y;
        for (depth, kind) in pending {
            let width = crate::label_box(world, view).text_width(
                rect.w,
                crate::text::text_engine(world).unit_advance(&crate::text::style_for(world, view)),
            );
            let x = rect.x + TEXT_ORIGIN_X + indent(depth, width);
            let mark_rect = if kind == StructuredMarkKind::Quote {
                Rect {
                    x,
                    y,
                    w: 2.0,
                    h: crate::source_folding::source_range_boxes(
                        world,
                        view,
                        block.range.start,
                        block.range.end,
                    )
                    .iter()
                    .map(|b| b.y + b.h - row.y)
                    .fold(row.h, f32::max),
                }
            } else {
                Rect {
                    x,
                    y: y + (row.h - 12.0).max(0.0) * 0.5,
                    w: match kind {
                        StructuredMarkKind::Ordered(_) => STRUCTURED_INDENT - 4.0,
                        _ => 12.0,
                    },
                    h: 12.0,
                }
            };
            if clip.intersect(Clip::from_rect(mark_rect)).h > 0.0 {
                marks.push(StructuredMark {
                    block: block.block,
                    revision: display.revision,
                    kind,
                    rect: mark_rect,
                    clip,
                });
            }
        }
        if block.kind == BlockKind::HorizontalRule {
            marks.push(StructuredMark {
                block: block.block,
                revision: display.revision,
                kind: StructuredMarkKind::HorizontalRule,
                rect: Rect {
                    x: rect.x + TEXT_ORIGIN_X + row.x,
                    y: y + row.h * 0.5,
                    w: (rect.w - 2.0 * TEXT_ORIGIN_X - row.x).max(0.0),
                    h: 2.0,
                },
                clip,
            });
        }
    }
    marks
}

/// Solid rectangles for a mark. Seven-segment decimal ordinals remain visibly
/// numeric without adding synthetic text, spans, or editable marker characters.
pub fn mark_rectangles(mark: &StructuredMark) -> Vec<Rect> {
    let r = mark.rect;
    let mut out = vec![];
    let mut add = |x, y, w, h| {
        out.push(Rect {
            x: r.x + x,
            y: r.y + y,
            w,
            h,
        })
    };
    match mark.kind {
        StructuredMarkKind::Quote | StructuredMarkKind::HorizontalRule => return vec![r],
        StructuredMarkKind::Bullet => add(4.0, 4.0, 4.0, 4.0),
        StructuredMarkKind::Task(checked) => {
            add(0.0, 0.0, 12.0, 1.5);
            add(0.0, 10.5, 12.0, 1.5);
            add(0.0, 0.0, 1.5, 12.0);
            add(10.5, 0.0, 1.5, 12.0);
            if checked {
                add(3.0, 3.0, 6.0, 6.0);
            }
        }
        StructuredMarkKind::Ordered(n) => {
            let text = n.to_string();
            let cell = (r.w / (text.len() as f32 + 1.0)).min(7.0);
            let masks = [
                0b1111110, 0b0110000, 0b1101101, 0b1111001, 0b0110011, 0b1011011, 0b1011111,
                0b1110000, 0b1111111, 0b1111011,
            ];
            let segments = [
                (1., 0., 4., 1.),
                (5., 1., 1., 4.),
                (5., 6., 1., 4.),
                (1., 10., 4., 1.),
                (0., 6., 1., 4.),
                (0., 1., 1., 4.),
                (1., 5., 4., 1.),
            ];
            for (i, d) in text.bytes().enumerate() {
                let mask = masks[(d - b'0') as usize];
                for (j, &(x, y, w, h)) in segments.iter().enumerate() {
                    if mask & (1 << (6 - j)) != 0 {
                        add((i as f32 + x / 7.0) * cell, y, w * cell / 7.0, h);
                    }
                }
            }
            add(text.len() as f32 * cell, 10.0, 1.5, 1.5);
        }
    }
    out
}
