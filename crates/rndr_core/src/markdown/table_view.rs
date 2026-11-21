//! Cells share the block's logical text for selection and copy, but shape and
//! wrap independently. Equal-width columns remain stable across virtual rows.
use super::*;

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
pub(super) fn height(world: &World, base: &TextStyle, block: &MarkdownBlock, width: f32) -> f32 {
    let engine = text_engine(world);
    let width = width / block.cells.len() as f32;
    block
        .cells
        .iter()
        .map(|(r, align)| {
            let style = cell_style(base, block, *align);
            let wrap = field_text_width(width, engine.unit_advance(&style));
            TEXT_ORIGIN_Y
                + engine.line_height(&style)
                    * engine
                        .measure(&block.text[r.clone()], &style, &cell_runs(block, r), wrap)
                        .line_count
                        .max(1) as f32
        })
        .fold(0., f32::max)
}
pub(super) fn spawn(
    world: &mut World,
    view: Entity,
    index: usize,
    block: &MarkdownBlock,
    outer: f32,
    theme: &MarkdownTheme,
    base: &TextStyle,
) -> Entity {
    let row = world.spawn((
        TableRow,
        box_layout(Axis::Row, Size::Fill, Size::Px(outer)),
        Piece {
            view,
            index: index as u32,
        },
    ));
    set_parent(world, row, view);
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
        set_parent(world, label, row);
    }
    row
}
