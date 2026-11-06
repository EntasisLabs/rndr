//! Table operation algorithms behind the structured model's stable public API.
//! Normalization and transaction publication remain in the parent authority.
use super::*;

#[derive(Clone, Copy)]
pub(super) enum TableAxis {
    Row,
    Column,
}
#[derive(Clone, Copy)]
pub(super) enum TableEdit {
    Insert,
    Delete,
}

pub(super) fn edit_table(
    s: &mut State,
    next: &mut u64,
    table: ContainerId,
    index: usize,
    axis: TableAxis,
    edit: TableEdit,
) -> Result<(), StructuredError> {
    let grid = table_grid(s, table)?;
    let rows = children(s, table)?;
    let height = grid.len();
    let width = grid.first().ok_or(StructuredError::InvalidSchema)?.len();
    let (count, limit) = match axis {
        TableAxis::Row => (height, 256),
        TableAxis::Column => (width, 64),
    };
    if index > count
        || (matches!(edit, TableEdit::Delete) && index == count)
        || (matches!(edit, TableEdit::Insert) && count >= limit)
    {
        return Err(StructuredError::InvalidPosition);
    }
    let near = subtree_bounds(s, table)?.0;
    let rectangle = match s.selection {
        StructuredSelection::Cells { anchor, head } => {
            Some((address(s, anchor)?, address(s, head)?))
        }
        _ => None,
    };
    if matches!(edit, TableEdit::Delete) {
        if count == 1 {
            remove_subtrees(s, &[table]);
        } else if matches!(axis, TableAxis::Row) {
            remove_subtrees(s, &[rows[index]]);
        } else {
            remove_subtrees(s, &grid.iter().map(|r| r[index]).collect::<Vec<_>>());
        }
        if count > 1 {
            if let Some((a, h)) = rectangle.filter(|(a, h)| a.table == table && h.table == table) {
                let remaining = table_grid(s, table)?;
                let repair = |a: CellAddress| {
                    if container(s, a.cell).is_ok() {
                        a.cell
                    } else {
                        remaining[a.row.min(remaining.len() - 1)]
                            [a.column.min(remaining[0].len() - 1)]
                    }
                };
                s.selection = StructuredSelection::Cells {
                    anchor: repair(a),
                    head: repair(h),
                };
            }
        }
        repair_table_selection(s, next, near)?;
    } else if matches!(axis, TableAxis::Row) {
        let position = if index == height {
            subtree_bounds(s, table)?.1 + 1
        } else {
            subtree_bounds(s, rows[index])?.0
        };
        let row = fresh_container(s, next, Some(table), ContainerKind::Row);
        let mut leaves = vec![];
        for _ in 0..width {
            let cell = fresh_container(s, next, Some(row), ContainerKind::Cell { header: false });
            leaves.push(empty_leaf(next, Some(cell)));
        }
        s.blocks.splice(position..position, leaves);
    } else {
        for r in (0..height).rev() {
            let position = if index == width {
                subtree_bounds(s, rows[r])?.1 + 1
            } else {
                subtree_bounds(s, grid[r][index])?.0
            };
            let header = matches!(
                container(s, grid[r][index.min(width - 1)])?.kind,
                ContainerKind::Cell { header: true }
            );
            let cell = fresh_container(s, next, Some(rows[r]), ContainerKind::Cell { header });
            let leaf = empty_leaf(next, Some(cell));
            s.blocks.insert(position, leaf);
        }
    }
    Ok(())
}

pub(super) fn replace_cell_rectangle(
    s: &mut State,
    next: &mut u64,
    anchor: ContainerId,
    head: ContainerId,
    text: String,
) -> Result<(), StructuredError> {
    let (table, top, left, bottom, right) = cell_rectangle(s, anchor, head)?;
    let grid = table_grid(s, table)?;
    let mut rows = vec![];
    for (r, row) in grid.iter().enumerate().take(bottom + 1).skip(top) {
        let mut cells = vec![];
        for (c, &cell) in row.iter().enumerate().take(right + 1).skip(left) {
            let ContainerKind::Cell { header } = container(s, cell)?.kind else {
                unreachable!()
            };
            let plain = if r == top && c == left {
                normalize_plain_input(&text)
            } else {
                String::new()
            };
            let blocks = plain
                .split('\n')
                .enumerate()
                .map(|(i, text)| Block {
                    id: BlockId(i as u64 + 1),
                    parent: None,
                    attributes: Attributes::new(),
                    kind: BlockKind::Paragraph,
                    runs: if text.is_empty() {
                        vec![]
                    } else {
                        vec![Run {
                            text: text.into(),
                            marks: Marks::default(),
                        }]
                    },
                })
                .collect();
            cells.push(CellFragment {
                header,
                blocks,
                containers: vec![],
            });
        }
        rows.push(cells);
    }
    let first = grid[top][left];
    paste_cells(s, next, first, TableFragment { rows })?;
    let (_, end) = subtree_bounds(s, first)?;
    let b = &s.blocks[end];
    let p = Position {
        block: b.id,
        offset: b.len(),
        affinity: Affinity::After,
    };
    s.selection = StructuredSelection::Text(TextSelection { anchor: p, head: p });
    Ok(())
}
