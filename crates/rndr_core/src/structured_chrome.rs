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

/// Optional per-editor appearance overrides. Also accepted as a world resource
/// for application defaults. Component fields win over resource fields; missing
/// fields follow the editor foreground/background and current core theme.
/// These affect paint only; no document bytes, permissions or history change.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct StructuredChromeStyle {
    /// Quiet table/card boundary.
    pub border: Option<Color>,
    /// Atomic card surface.
    pub surface: Option<Color>,
    /// Header cell surface.
    pub header: Option<Color>,
    /// Secondary card label.
    pub muted: Option<Color>,
    /// Selected cell/card outline.
    pub accent: Option<Color>,
    /// Atomic card corner radius; absent uses six logical points.
    pub radius: Option<f32>,
}
impl Component for StructuredChromeStyle {}

/// Resolved shared palette, also usable by custom host renderers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StructuredChromePalette {
    /// Resting boundary.
    pub border: Color,
    /// Card surface.
    pub surface: Color,
    /// Header surface.
    pub header: Color,
    /// Secondary text.
    pub muted: Color,
    /// Selection outline.
    pub accent: Color,
    /// Card corner radius.
    pub radius: f32,
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::rgb(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
    )
}
/// Resolve against the actual editor surface, not a fixed light-only swatch.
pub fn chrome_palette(world: &World, view: Entity) -> StructuredChromePalette {
    let theme = theme_of(world);
    let bg = world.get::<Color>(view).copied().unwrap_or(theme.pane);
    let fg = world
        .get::<Label>(view)
        .and_then(|l| l.color)
        .unwrap_or(theme.text);
    let local = world
        .get::<StructuredChromeStyle>(view)
        .copied()
        .unwrap_or_default();
    let global = world
        .resource::<StructuredChromeStyle>()
        .copied()
        .unwrap_or_default();
    let style = StructuredChromeStyle {
        border: local.border.or(global.border),
        surface: local.surface.or(global.surface),
        header: local.header.or(global.header),
        muted: local.muted.or(global.muted),
        accent: local.accent.or(global.accent),
        radius: local.radius.or(global.radius),
    };
    StructuredChromePalette {
        border: style.border.unwrap_or_else(|| mix(bg, fg, 0.22)),
        surface: style.surface.unwrap_or_else(|| mix(bg, fg, 0.025)),
        header: style.header.unwrap_or_else(|| mix(bg, fg, 0.055)),
        muted: style.muted.unwrap_or_else(|| mix(bg, fg, 0.68)),
        accent: style.accent.unwrap_or(theme.caret),
        radius: style
            .radius
            .filter(|r| r.is_finite())
            .unwrap_or(6.0)
            .clamp(0.0, 24.0),
    }
}

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
    /// Border of one canonical table cell, including header semantics.
    TableCell {
        /// Explicit header semantics from the canonical cell container.
        header: bool,
        /// Membership in the current canonical rectangular cell selection.
        selected: bool,
    },
    /// Inert whole-node card; hosts own any payload or nested editor.
    Atomic,
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
    let palette = chrome_palette(world, view);
    let color_key = [
        palette.muted.r,
        palette.muted.g,
        palette.muted.b,
        palette.muted.a,
    ]
    .iter()
    .fold(0_u64, |key, c| key.rotate_left(11) ^ c.to_bits() as u64);
    doc.blocks().iter().fold(
        0xE610_u64 ^ doc.revision().0 ^ doc.id().0.rotate_left(29) ^ color_key,
        |key, block| {
            let depth = doc
                .ancestors(block.id)
                .unwrap_or_default()
                .iter()
                .filter(|c| matches!(c.kind, ContainerKind::Quote | ContainerKind::Item { .. }))
                .count();
            key.rotate_left(7) ^ depth as u64 ^ block.len() as u64
        },
    )
}

/// Whether this view needs spatial (rather than monotonically stacked) rows.
pub(crate) fn has_tables(world: &World, view: Entity) -> bool {
    world.has::<StructuredChromeGutter>(view)
        && crate::structured_native::attached_structured(world, view)
            .and_then(|handle| world.resource::<StructuredDocuments>()?.get(handle))
            .is_some_and(|doc| {
                doc.containers()
                    .iter()
                    .any(|c| c.kind == ContainerKind::Table)
            })
}

// Grid windows keep all source bytes and move canonical geometry by the exact
// scroll offset, so paint freshness must include that offset as well as source.
pub(crate) fn paint_offset_key(world: &World, view: Entity) -> u64 {
    if has_tables(world, view) {
        world
            .get::<Editor>(view)
            .map_or(0, |e| (e.offset.to_bits() as u64).rotate_left(31))
    } else {
        0
    }
}

const CELL_PADDING: f32 = 10.0;
const ATOMIC_HEIGHT: f32 = 96.0;
const CARD_GAP: f32 = 8.0;
const CARD_PADDING: f32 = 14.0;

type CellInfo = (
    crate::structured::ContainerId,
    crate::structured::ContainerId,
    usize,
    usize,
    bool,
);

// Canonical leaf order defines columns even after inserting a fresh container
// between older siblings. Compute membership once, not inside the paint loop.
fn cell_infos(
    doc: &crate::structured::StructuredDocument,
) -> std::collections::HashMap<BlockId, CellInfo> {
    let mut members = Vec::new();
    let mut rows = std::collections::HashMap::<_, Vec<_>>::new();
    for block in doc.blocks() {
        let Ok(ancestors) = doc.ancestors(block.id) else {
            continue;
        };
        let Some(cell) = ancestors
            .iter()
            .find(|c| matches!(c.kind, ContainerKind::Cell { .. }))
        else {
            continue;
        };
        let Some(row) = cell.parent else {
            continue;
        };
        let ContainerKind::Cell { header } = cell.kind else {
            continue;
        };
        let cells = rows.entry(row).or_default();
        if !cells.contains(&cell.id) {
            cells.push(cell.id);
        }
        let column = cells.iter().position(|id| *id == cell.id).unwrap_or(0);
        members.push((block.id, row, cell.id, column, header));
    }
    members
        .into_iter()
        .map(|(block, row, cell, column, header)| {
            (block, (row, cell, column, rows[&row].len(), header))
        })
        .collect()
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
    let cells = cell_infos(doc);
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
            (start, depth, block.id, block.kind)
        })
        .collect();
    let mut result = TextLayout::shaped(vec![], vec![], 0.0, 0.0);
    let mut start = 0;
    let mut active_row = None;
    let mut row_top = 0.0;
    let mut cell_heights = std::collections::HashMap::new();
    for paragraph in text.split('\n') {
        let source = if start <= preview_at {
            start
        } else {
            start.saturating_sub(preview_len).max(preview_at)
        } + base;
        let paragraph_index = paragraphs
            .partition_point(|(at, ..)| *at <= source)
            .saturating_sub(1);
        let depth = paragraphs
            .get(paragraph_index)
            .map_or(0, |(_, depth, ..)| *depth);
        let block = paragraphs.get(paragraph_index);
        let cell = block.and_then(|(_, _, id, _)| cells.get(id).copied());
        let row = cell.map(|c| c.0);
        if row.is_some() != active_row.is_some() {
            result.height += CARD_GAP;
        }
        if row != active_row {
            active_row = row;
            row_top = result.height;
            cell_heights.clear();
        }
        let column_width = cell.map_or(width, |c| width / c.3.max(1) as f32);
        let x = cell.map_or_else(
            || indent(depth, width),
            |c| {
                c.2 as f32 * column_width
                    + CELL_PADDING
                    + indent(depth, column_width - 2.0 * CELL_PADDING)
            },
        );
        let available = if cell.is_some() {
            column_width - 2.0 * CELL_PADDING - indent(depth, column_width - 2.0 * CELL_PADDING)
        } else {
            width - x
        };
        let y = cell.map_or(result.height, |c| {
            row_top + CELL_PADDING + cell_heights.get(&c.1).copied().unwrap_or(0.0)
        });
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
        let mut part = engine.layout(paragraph, &style, &local_spans, available.max(1.0));
        if cell.is_some() {
            for line in &mut part.lines {
                line.width = available.max(1.0);
            }
        }
        if block.is_some_and(|(_, _, _, kind)| *kind == BlockKind::Atomic) {
            if let Some(payload) = doc
                .blocks()
                .get(paragraph_index)
                .and_then(|b| b.atomic_payload())
            {
                let title = match payload.kind.as_str() {
                    "image" => "Image unavailable",
                    "embedded" => "Embedded content",
                    _ => "Attached content",
                };
                let caption: String = payload
                    .fallback
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(220)
                    .collect();
                let inner = (available - 2.0 * CARD_PADDING).max(1.0);
                let mut title_style = style.clone();
                title_style.size = (style.size * 0.68).max(10.0);
                title_style.weight = 600;
                title_style.paragraph_spacing = 0.0;
                let mut heading = engine.layout(title, &title_style, &[], inner);
                let title_height = title_style.size * 1.5;
                heading
                    .glyphs
                    .retain(|g| g.y + g.h <= title_height && g.x + g.w <= inner);
                for glyph in &mut heading.glyphs {
                    glyph.color = Some(chrome_palette(world, view).muted);
                }
                let mut visual = engine.layout(&caption, &style, &[], inner);
                // Reserve up to two fallback lines at the inherited text scale.
                // Longer fallback text remains intact in model/copy/semantics.
                let card_height = (2.0 * CARD_GAP
                    + 2.0 * CARD_PADDING
                    + title_height
                    + 4.0
                    + visual.height.min(2.0 * part.height))
                .max(ATOMIC_HEIGHT);
                part.height = card_height;
                for glyph in &mut visual.glyphs {
                    glyph.y += title_height + 4.0;
                }
                heading.glyphs.extend(visual.glyphs);
                heading.glyphs.retain(|g| {
                    g.y + g.h <= card_height - 2.0 * CARD_GAP - 2.0 * CARD_PADDING
                        && g.x + g.w <= inner
                });
                for glyph in &mut heading.glyphs {
                    // Inert captions never become editable source ranges.
                    glyph.start = 0;
                    glyph.end = 0;
                    glyph.x += CARD_PADDING;
                    glyph.y += CARD_GAP + CARD_PADDING;
                }
                part.glyphs = heading.glyphs;
            }
            part.height = part.height.max(ATOMIC_HEIGHT);
            if let Some(line) = part.lines.first_mut() {
                line.h = part.height;
            }
        }
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
        if let Some((_, id, ..)) = cell {
            *cell_heights.entry(id).or_insert(0.0) += part.height;
            result.height = result.height.max(y + part.height + CELL_PADDING);
            result.width = result.width.max(width);
        } else {
            result.height += part.height;
        }
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
    key ^= paint_offset_key(world, view);
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
fn selected_cells(
    world: &World,
    view: Entity,
) -> std::collections::HashSet<crate::structured::ContainerId> {
    let selection = (|| {
        let handle = crate::structured_native::attached_structured(world, view)?;
        let doc = world
            .resource::<crate::structured_native::StructuredDocuments>()?
            .get(handle)?;
        let crate::structured::StructuredSelection::Cells { anchor, head } = doc.selection() else {
            return None;
        };
        let a = doc
            .cell_address(doc.cell_first_block(anchor).ok()?)
            .ok()??;
        let h = doc.cell_address(doc.cell_first_block(head).ok()?).ok()??;
        if a.table != h.table {
            return None;
        }
        let grid = doc.table_cells(a.table).ok()?;
        let mut selected = std::collections::HashSet::new();
        for row in grid
            .iter()
            .take(a.row.max(h.row) + 1)
            .skip(a.row.min(h.row))
        {
            selected.extend(
                row.iter()
                    .take(a.column.max(h.column) + 1)
                    .skip(a.column.min(h.column))
                    .copied(),
            );
        }
        Some(selected)
    })();
    selection.unwrap_or_default()
}
/// Current clipped canonical chrome, including structural selection state.
pub fn structured_marks(world: &World, view: Entity) -> Vec<StructuredMark> {
    let selected_cells = selected_cells(world, view);
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
    let cells = cell_infos(doc);
    let mut row_bounds = std::collections::HashMap::new();
    for member in &display.blocks {
        let Some(cell) = cells.get(&member.block) else {
            continue;
        };
        let bounds = row_bounds
            .entry(cell.0)
            .or_insert((f32::INFINITY, f32::NEG_INFINITY));
        if let Some(caret) =
            crate::source_folding::source_position_box(world, view, member.range.start)
        {
            bounds.0 = bounds.0.min(caret.y);
            bounds.1 = bounds.1.max(caret.y + caret.h);
        }
        for fragment in crate::source_folding::source_range_boxes(
            world,
            view,
            member.range.start,
            member.range.end,
        ) {
            bounds.0 = bounds.0.min(fragment.y);
            bounds.1 = bounds.1.max(fragment.y + fragment.h);
        }
    }
    let mut marks = vec![];
    let mut seen_items = std::collections::HashSet::new();
    let mut seen_cells = std::collections::HashSet::new();
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
            let local_x = cells.get(&block.block).map_or_else(
                || indent(depth, width),
                |c| {
                    let column_width = width / c.3.max(1) as f32;
                    c.2 as f32 * column_width
                        + CELL_PADDING
                        + indent(depth, column_width - 2.0 * CELL_PADDING)
                },
            );
            let x = rect.x + TEXT_ORIGIN_X + local_x;
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
        if let Some((row_id, cell_id, column, columns, header)) = cells.get(&block.block).copied() {
            if seen_cells.insert(cell_id) {
                let (top, bottom) = row_bounds[&row_id];
                let width = crate::label_box(world, view).text_width(
                    rect.w,
                    crate::text::text_engine(world)
                        .unit_advance(&crate::text::style_for(world, view)),
                );
                let column_width = width / columns.max(1) as f32;
                let cell_rect = Rect {
                    x: rect.x + TEXT_ORIGIN_X + column as f32 * column_width,
                    y: rect.y + TEXT_ORIGIN_Y + top - CELL_PADDING,
                    w: column_width,
                    h: (bottom - top + 2.0 * CELL_PADDING).max(0.0),
                };
                if top.is_finite() && clip.intersect(Clip::from_rect(cell_rect)).h > 0.0 {
                    marks.push(StructuredMark {
                        block: block.block,
                        revision: display.revision,
                        kind: StructuredMarkKind::TableCell {
                            header,
                            selected: selected_cells.contains(&cell_id),
                        },
                        rect: cell_rect,
                        clip,
                    });
                }
            }
        }
        if block.kind == BlockKind::Atomic {
            let width = crate::label_box(world, view).text_width(
                rect.w,
                crate::text::text_engine(world).unit_advance(&crate::text::style_for(world, view)),
            );
            marks.push(StructuredMark {
                block: block.block,
                revision: display.revision,
                kind: StructuredMarkKind::Atomic,
                rect: Rect {
                    x: rect.x + TEXT_ORIGIN_X + row.x,
                    y: y + CARD_GAP,
                    w: cells.get(&block.block).map_or(width - row.x, |c| {
                        width / c.3.max(1) as f32 - 2.0 * CELL_PADDING
                    }),
                    h: (row.h.max(ATOMIC_HEIGHT) - 2.0 * CARD_GAP).max(0.0),
                },
                clip,
            });
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

/// One noninteractive structural paint layer, behind text and text selection.
#[derive(Clone, Copy, Debug)]
pub struct StructuredChromePaint {
    /// Absolute logical geometry.
    pub rect: Rect,
    /// Effective editor clip.
    pub clip: Clip,
    /// Theme-derived or host-overridden fill.
    pub color: Color,
    /// Corner radius in logical points.
    pub radius: f32,
}
/// Shared backgrounds and boundaries for document tables and inert cards.
/// Kept separate from text so glyphs, caret and selection always paint on top.
pub fn chrome_paint(world: &World, view: Entity) -> Vec<StructuredChromePaint> {
    let palette = chrome_palette(world, view);
    let selected = crate::structured_native::attached_structured(world, view)
        .and_then(|handle| world.resource::<StructuredDocuments>()?.get(handle))
        .and_then(|doc| match doc.selection() {
            crate::structured::StructuredSelection::Node(id) => Some(id),
            _ => None,
        });
    let mut out = Vec::new();
    for mark in structured_marks(world, view) {
        let mut add = |rect, color, radius| {
            out.push(StructuredChromePaint {
                rect,
                color,
                radius,
                clip: mark.clip,
            })
        };
        match mark.kind {
            StructuredMarkKind::TableCell { header, selected } => {
                if header || selected {
                    let fill = if selected {
                        mix(palette.header, palette.accent, 0.12)
                    } else {
                        palette.header
                    };
                    add(mark.rect, fill, 0.0);
                }
                let border = if selected {
                    palette.accent
                } else {
                    palette.border
                };
                for rect in mark_rectangles(&mark) {
                    add(rect, border, 0.0);
                }
            }
            StructuredMarkKind::Atomic => {
                let selected = selected == Some(mark.block);
                let stroke: f32 = if selected { 2.0 } else { 1.0 };
                let r = mark.rect;
                let radius = palette.radius.min(r.w * 0.5).min(r.h * 0.5);
                add(
                    r,
                    if selected {
                        palette.accent
                    } else {
                        palette.border
                    },
                    radius,
                );
                let inset = stroke.min(r.w * 0.5).min(r.h * 0.5);
                add(
                    Rect {
                        x: r.x + inset,
                        y: r.y + inset,
                        w: (r.w - 2.0 * inset).max(0.0),
                        h: (r.h - 2.0 * inset).max(0.0),
                    },
                    if selected {
                        mix(palette.surface, palette.accent, 0.06)
                    } else {
                        palette.surface
                    },
                    (radius - inset).max(0.0),
                );
            }
            _ => {}
        }
    }
    out
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
        StructuredMarkKind::TableCell { selected, .. } => {
            let border: f32 = if selected { 2.0 } else { 1.0 };
            add(0.0, 0.0, r.w, border.min(r.h));
            add(0.0, (r.h - border).max(0.0), r.w, border.min(r.h));
            add(0.0, 0.0, border.min(r.w), r.h);
            add((r.w - border).max(0.0), 0.0, border.min(r.w), r.h);
        }
        StructuredMarkKind::Atomic => {
            add(0.0, 0.0, r.w, 1.5);
            add(0.0, (r.h - 1.5).max(0.0), r.w, 1.5);
            add(0.0, 0.0, 1.5, r.h);
            add((r.w - 1.5).max(0.0), 0.0, 1.5, r.h);
        }
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
