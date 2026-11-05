//! Cell navigation and rich clipboard boundary for the canonical tree.
use super::*;
use crate::structured::{Block, CellFragment, ContainerId, Marks, Run, TableFragment};

#[derive(Clone)]
struct CellClipboard {
    plain: String,
    fragment: TableFragment,
}

fn apply_operation(
    world: &mut World,
    entity: Entity,
    operation: Operation,
) -> Result<(), StructuredViewError> {
    let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
    doc.apply(Transaction {
        document: doc.id(),
        base: doc.revision(),
        origin: EditOrigin::Keyboard,
        group: None,
        operations: vec![operation],
    })?;
    Ok(())
}
fn blank_cell(header: bool, text: String) -> CellFragment {
    CellFragment {
        header,
        blocks: vec![Block {
            id: BlockId(0),
            kind: BlockKind::Paragraph,
            runs: if text.is_empty() {
                vec![]
            } else {
                vec![Run {
                    text,
                    marks: Marks::default(),
                }]
            },
            parent: None,
            attributes: Default::default(),
        }],
        containers: vec![],
    }
}
fn upper_left(
    doc: &StructuredDocument,
    anchor: ContainerId,
    head: ContainerId,
) -> Result<ContainerId, StructuredError> {
    let a = doc
        .cell_address(doc.cell_first_block(anchor)?)?
        .ok_or(StructuredError::InvalidPosition)?;
    let h = doc
        .cell_address(doc.cell_first_block(head)?)?
        .ok_or(StructuredError::InvalidPosition)?;
    if a.table != h.table {
        return Err(StructuredError::InvalidPosition);
    }
    Ok(doc.table_cells(a.table)?[a.row.min(h.row)][a.column.min(h.column)])
}

pub(super) fn apply_table_key(
    world: &mut World,
    entity: Entity,
    event: &KeyEvent,
    clip: &str,
) -> Result<Option<crate::typing::StepResult>, StructuredViewError> {
    let Some(doc) = document(world, entity) else {
        return Ok(None);
    };
    let selection = doc.selection();
    let address = match selection {
        StructuredSelection::Text(s) => doc.cell_address(s.head.block)?,
        StructuredSelection::Node(b) => doc.cell_address(b)?,
        StructuredSelection::Cells { head, .. } => doc.cell_address(doc.cell_first_block(head)?)?,
    };
    let Some(address) = address else {
        return Ok(None);
    };
    let bare = !event.modifiers.ctrl && !event.modifiers.super_key && !event.modifiers.alt;
    if bare && event.logical == LogicalKey::Named(NamedKey::Tab) {
        let backward = event.modifiers.shift;
        let cells = doc.table_cells(address.table)?;
        let edge = if backward {
            cells[0][0]
        } else {
            *cells.last().unwrap().last().unwrap()
        };
        let block = doc.cell_first_block(edge)?;
        let index = doc
            .blocks()
            .iter()
            .position(|b| b.id == block)
            .ok_or(StructuredError::InvalidPosition)?;
        let outside = if backward {
            doc.blocks()[..index]
                .iter()
                .rev()
                .find(|b| !b.is_atomic() && doc.cell_address(b.id).ok().flatten().is_none())
        } else {
            doc.blocks()[index + 1..]
                .iter()
                .find(|b| !b.is_atomic() && doc.cell_address(b.id).ok().flatten().is_none())
        }
        .map(|b| Position {
            block: b.id,
            offset: if backward { b.len() } else { 0 },
            affinity: Affinity::After,
        });
        let doc = document_mut(world, entity).ok_or(StructuredViewError::Unavailable)?;
        if !doc.navigate_cell(backward)? {
            if let Some(p) = outside {
                doc.set_selection(StructuredSelection::Text(TextSelection {
                    anchor: p,
                    head: p,
                }))?;
            }
        }
        return Ok(Some(Default::default()));
    }
    let action = crate::keys::standard_action(crate::keys::platform_of(world), event);
    if let StructuredSelection::Cells { anchor, head } = selection {
        if action == Some(EditAction::Copy) || action == Some(EditAction::Cut) {
            let plain = doc.cells_plain_text(selection)?;
            let fragment = doc.copy_cells(selection)?;
            let at = upper_left(doc, anchor, head)?;
            if action == Some(EditAction::Cut) {
                if crate::interaction::readonly(world, entity) || doc.is_readonly() {
                    return Err(StructuredError::Readonly.into());
                }
                let empty = TableFragment {
                    rows: fragment
                        .rows
                        .iter()
                        .map(|row| {
                            row.iter()
                                .map(|c| blank_cell(c.header, String::new()))
                                .collect()
                        })
                        .collect(),
                };
                apply_operation(
                    world,
                    entity,
                    Operation::PasteCells {
                        at,
                        fragment: empty,
                    },
                )?;
            }
            world.insert_resource(CellClipboard {
                plain: plain.clone(),
                fragment,
            });
            return Ok(Some(crate::typing::StepResult {
                copied: Some(plain),
                clipboard_action: true,
            }));
        }
        if bare
            && matches!(
                event.logical,
                LogicalKey::Named(
                    NamedKey::ArrowLeft
                        | NamedKey::ArrowRight
                        | NamedKey::ArrowUp
                        | NamedKey::ArrowDown
                        | NamedKey::Escape
                )
            )
        {
            let selection =
                if event.modifiers.shift && event.logical != LogicalKey::Named(NamedKey::Escape) {
                    let grid = doc.table_cells(address.table)?;
                    let (row, column) = match event.logical {
                        LogicalKey::Named(NamedKey::ArrowLeft) => {
                            (address.row, address.column.saturating_sub(1))
                        }
                        LogicalKey::Named(NamedKey::ArrowRight) => {
                            (address.row, (address.column + 1).min(grid[0].len() - 1))
                        }
                        LogicalKey::Named(NamedKey::ArrowUp) => {
                            (address.row.saturating_sub(1), address.column)
                        }
                        _ => ((address.row + 1).min(grid.len() - 1), address.column),
                    };
                    StructuredSelection::Cells {
                        anchor,
                        head: grid[row][column],
                    }
                } else {
                    let p = Position {
                        block: doc.cell_first_block(head)?,
                        offset: 0,
                        affinity: Affinity::After,
                    };
                    StructuredSelection::Text(TextSelection { anchor: p, head: p })
                };
            document_mut(world, entity)
                .ok_or(StructuredViewError::Unavailable)?
                .set_selection(selection)?;
            return Ok(Some(Default::default()));
        }
        if bare
            && matches!(
                event.logical,
                LogicalKey::Named(NamedKey::Backspace | NamedKey::Delete)
            )
        {
            if crate::interaction::readonly(world, entity) || doc.is_readonly() {
                return Err(StructuredError::Readonly.into());
            }
            let fragment = doc.copy_cells(selection)?;
            let at = upper_left(doc, anchor, head)?;
            let fragment = TableFragment {
                rows: fragment
                    .rows
                    .iter()
                    .map(|r| {
                        r.iter()
                            .map(|c| blank_cell(c.header, String::new()))
                            .collect()
                    })
                    .collect(),
            };
            apply_operation(world, entity, Operation::PasteCells { at, fragment })?;
            return Ok(Some(Default::default()));
        }
    }
    if action == Some(EditAction::Paste) {
        if crate::interaction::readonly(world, entity) || doc.is_readonly() {
            return Err(StructuredError::Readonly.into());
        }
        let at = match selection {
            StructuredSelection::Cells { anchor, head } => upper_left(doc, anchor, head)?,
            _ => address.cell,
        };
        let rich = world
            .resource::<CellClipboard>()
            .filter(|c| c.plain == clip)
            .map(|c| c.fragment.clone());
        if rich.is_some()
            || clip.contains('\t')
            || matches!(selection, StructuredSelection::Cells { .. })
        {
            let fragment = rich.unwrap_or_else(|| TableFragment {
                rows: crate::structured::normalize_plain_input(clip)
                    .split('\n')
                    .map(|row| {
                        row.split('\t')
                            .map(|text| blank_cell(false, text.into()))
                            .collect()
                    })
                    .collect(),
            });
            apply_operation(world, entity, Operation::PasteCells { at, fragment })?;
            return Ok(Some(crate::typing::StepResult {
                copied: None,
                clipboard_action: true,
            }));
        }
    }
    // A rectangle is not a disposable text range. Do not fall through to
    // scalar editing, IME or cross-cell replacement using projected offsets.
    if matches!(selection, StructuredSelection::Cells { .. }) {
        return Ok(Some(Default::default()));
    }
    Ok(None)
}
