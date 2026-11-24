//! Cells share the block's logical text for selection and copy, but shape and
//! wrap independently. Equal-width columns remain stable across virtual rows.
//!
//! Each row draws its own share of a collapsed grid out of one-point rules:
//! side and column rules beside its cells, its bottom rule, and for the
//! header the top one too. Stacked rows never double a line, a row needs no
//! knowledge of its neighbours, and cell fills stay independent of the rules,
//! so a host with a transparent page surface still gets clean cells.
use super::*;

/// Grid rule thickness, logical points.
const GRID: f32 = 1.0;

pub(super) struct TableRow;
impl Component for TableRow {}
struct CellOffset(usize);
impl Component for CellOffset {}
pub(super) fn offset(world: &World, label: Entity) -> usize {
    world.get::<CellOffset>(label).map_or(0, |o| o.0)
}
pub(super) fn labels(world: &World, row: Entity) -> Vec<Entity> {
    if world.has::<TableRow>(row) {
        children_of(world, row)
            .into_iter()
            .flat_map(|frame| children_of(world, frame))
            .flat_map(|cells| children_of(world, cells))
            .filter(|cell| world.has::<Label>(*cell))
            .collect()
    } else {
        label_entity(world, row).into_iter().collect()
    }
}
fn cell_runs(block: &MarkdownBlock, r: &std::ops::Range<usize>) -> Vec<TextSpan> {
    block
        .spans
        .iter()
        .filter_map(|s| {
            let a = s.start.max(r.start);
            let b = s.end.min(r.end);
            (a < b).then(|| TextSpan {
                start: a - r.start,
                end: b - r.start,
                ..s.clone()
            })
        })
        .collect()
}
fn cell_style(base: &TextStyle, block: &MarkdownBlock, align: crate::text::TextAlign) -> TextStyle {
    let mut style = block_style(base, block);
    style.align = align;
    if block.table_header {
        style.weight = FONT_WEIGHT_BOLD;
    }
    style
}
/// Rules a row draws above and below its cells.
fn rules(block: &MarkdownBlock) -> f32 {
    if block.table_header {
        2.0 * GRID
    } else {
        GRID
    }
}
/// Tallest cell: text with the label inset above and below it.
fn cells_height(world: &World, base: &TextStyle, block: &MarkdownBlock, width: f32) -> f32 {
    let engine = text_engine(world);
    let n = block.cells.len() as f32;
    let width = ((width - GRID * (n + 1.0)) / n).max(1.0);
    block
        .cells
        .iter()
        .map(|(r, align)| {
            let style = cell_style(base, block, *align);
            let wrap = field_text_width(width, engine.unit_advance(&style));
            2.0 * TEXT_ORIGIN_Y
                + engine.line_height(&style)
                    * engine
                        .measure(&block.text[r.clone()], &style, &cell_runs(block, r), wrap)
                        .line_count
                        .max(1) as f32
        })
        .fold(0., f32::max)
}
pub(super) fn height(world: &World, base: &TextStyle, block: &MarkdownBlock, width: f32) -> f32 {
    cells_height(world, base, block, width) + rules(block)
}
fn rule(world: &mut World, parent: Entity, color: Color, width: Size, height: Size) {
    let rule = world.spawn((box_layout(Axis::Column, width, height), color));
    set_parent(world, rule, parent);
}
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn(
    world: &mut World,
    view: Entity,
    index: usize,
    block: &MarkdownBlock,
    content: f32,
    outer: f32,
    theme: &MarkdownTheme,
    base: &TextStyle,
) -> Entity {
    // The row spans the block's trailing gap; the frame stops at the grid.
    let row = world.spawn((
        TableRow,
        box_layout(Axis::Column, Size::Fill, Size::Px(outer)),
        Piece {
            view,
            index: index as u32,
        },
    ));
    set_parent(world, row, view);
    let frame = world.spawn(box_layout(Axis::Column, Size::Fill, Size::Px(content)));
    set_parent(world, frame, row);
    let grid = theme.grid;
    if block.table_header {
        rule(world, frame, grid, Size::Fill, Size::Px(GRID));
    }
    let cells = world.spawn(box_layout(
        Axis::Row,
        Size::Fill,
        Size::Px(content - rules(block)),
    ));
    set_parent(world, cells, frame);
    rule(world, cells, grid, Size::Px(GRID), Size::Fill);
    let fill = if block.table_header {
        theme.header
    } else {
        theme.surface
    };
    for (r, align) in &block.cells {
        let label = world.spawn((
            box_layout(Axis::Column, Size::Fill, Size::Fill),
            CellOffset(r.start),
            Label::new(&block.text[r.clone()])
                .with_color(theme.text)
                .overflow(TextOverflow::Wrap),
            Spans {
                runs: cell_runs(block, r),
            },
            cell_style(base, block, *align),
            Piece {
                view,
                index: index as u32,
            },
        ));
        world.insert(label, fill);
        set_parent(world, label, cells);
        rule(world, cells, grid, Size::Px(GRID), Size::Fill);
    }
    rule(world, frame, grid, Size::Fill, Size::Px(GRID));
    row
}
