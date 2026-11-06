//! Contracts for operation extraction: public transaction boundary, private identity state.
use super::*;

fn apply(d: &mut StructuredDocument, operations: Vec<Operation>) -> Result<bool, StructuredError> {
    d.apply(Transaction {
        document: d.id(),
        base: d.revision(),
        origin: EditOrigin::Keyboard,
        group: None,
        operations,
    })
}
fn table(rows: usize, columns: usize) -> (StructuredDocument, ContainerId) {
    let mut d = StructuredDocument::new("");
    let at = d.position_at(0, Affinity::After).unwrap();
    apply(
        &mut d,
        vec![Operation::InsertTable {
            at,
            rows,
            columns,
            header: true,
        }],
    )
    .unwrap();
    let id = d
        .containers()
        .iter()
        .find(|c| c.kind == ContainerKind::Table)
        .unwrap()
        .id;
    (d, id)
}

#[test]
fn structured_operation_table_axes_keep_ids_headers_selection_and_inverses() {
    for row_axis in [true, false] {
        for insert in [true, false] {
            for index in 0..=2 {
                let (mut d, id) = table(2, 2);
                let grid = d.table_cells(id).unwrap();
                d.set_selection(StructuredSelection::Cells {
                    anchor: grid[1][1],
                    head: grid[0][0],
                })
                .unwrap();
                let before = d.state.clone();
                let next = d.next_block;
                let revision = d.revision();
                let operation = match (row_axis, insert) {
                    (true, true) => Operation::InsertRow { table: id, index },
                    (true, false) => Operation::DeleteRow { table: id, index },
                    (false, true) => Operation::InsertColumn { table: id, index },
                    (false, false) => Operation::DeleteColumn { table: id, index },
                };
                let result = apply(&mut d, vec![operation]);
                if !insert && index == 2 {
                    assert_eq!(result, Err(StructuredError::InvalidPosition));
                    assert_eq!(d.state, before);
                    assert_eq!(d.next_block, next);
                    assert_eq!(d.revision(), revision);
                    continue;
                }
                assert_eq!(result, Ok(true));
                let after = d.state.clone();
                let cells = d.table_cells(id).unwrap();
                assert_eq!(
                    cells.len(),
                    if row_axis {
                        if insert {
                            3
                        } else {
                            1
                        }
                    } else {
                        2
                    }
                );
                assert_eq!(
                    cells[0].len(),
                    if row_axis {
                        2
                    } else if insert {
                        3
                    } else {
                        1
                    }
                );
                for (r, row) in cells.iter().enumerate() {
                    for &cell in row {
                        let header = matches!(
                            container(&d.state, cell).unwrap().kind,
                            ContainerKind::Cell { header: true }
                        );
                        let expected = if row_axis && insert {
                            r == usize::from(index == 0)
                        } else if row_axis && !insert {
                            index != 0
                        } else {
                            r == 0
                        };
                        assert_eq!(header, expected);
                    }
                }
                if insert {
                    assert_eq!(d.selection(), before.selection);
                    assert!(grid
                        .iter()
                        .flatten()
                        .all(|id| cells.iter().flatten().any(|c| c == id)));
                }
                validate_selection(&d.state, d.selection()).unwrap();
                assert!(d.undo().unwrap());
                assert_eq!(d.state, before);
                assert!(d.redo().unwrap());
                assert_eq!(d.state, after);
            }
        }
    }
}

#[test]
fn structured_operation_last_axis_removal_repairs_selection_and_refusal_is_atomic() {
    for row_axis in [true, false] {
        let (mut d, id) = table(1, 1);
        let cell = d.table_cells(id).unwrap()[0][0];
        d.set_selection(StructuredSelection::Cells {
            anchor: cell,
            head: cell,
        })
        .unwrap();
        let before = d.state.clone();
        let next = d.next_block;
        let revision = d.revision();
        let remove = if row_axis {
            Operation::DeleteRow {
                table: id,
                index: 0,
            }
        } else {
            Operation::DeleteColumn {
                table: id,
                index: 0,
            }
        };
        let invalid = Operation::DeleteColumn {
            table: id,
            index: 99,
        };
        assert_eq!(
            apply(&mut d, vec![remove.clone(), invalid]),
            Err(StructuredError::InvalidSchema)
        );
        assert_eq!(d.state, before);
        assert_eq!(d.next_block, next);
        assert_eq!(d.revision(), revision);
        apply(&mut d, vec![remove]).unwrap();
        assert!(d.table_cells(id).is_err());
        assert!(matches!(d.selection(), StructuredSelection::Text(_)));
        validate_selection(&d.state, d.selection()).unwrap();
        d.undo().unwrap();
        assert_eq!(d.state, before);
    }
}

#[test]
fn structured_operation_replace_cells_normalizes_text_preserves_headers_and_history() {
    let (mut d, id) = table(2, 2);
    let grid = d.table_cells(id).unwrap();
    d.set_selection(StructuredSelection::Cells {
        anchor: grid[1][1],
        head: grid[0][0],
    })
    .unwrap();
    let before = d.state.clone();
    apply(
        &mut d,
        vec![Operation::ReplaceCells {
            anchor: grid[1][1],
            head: grid[0][0],
            text: "é\r\n😀\r".into(),
        }],
    )
    .unwrap();
    assert_eq!(d.table_cells(id).unwrap(), grid);
    for (r, row) in grid.iter().enumerate() {
        for (c, &cell) in row.iter().enumerate() {
            let blocks: Vec<_> = d
                .blocks()
                .iter()
                .filter(|b| descends(&d.state, b.parent, cell))
                .collect();
            let text = blocks
                .iter()
                .map(|b| b.text())
                .collect::<Vec<_>>()
                .join("\n");
            assert_eq!(text, if r == 0 && c == 0 { "é\n😀\n" } else { "" });
            assert_eq!(
                container(&d.state, cell).unwrap().kind,
                ContainerKind::Cell { header: r == 0 }
            );
            assert!(blocks
                .iter()
                .all(|b| !before.blocks.iter().any(|old| old.id == b.id)));
        }
    }
    let last = d
        .blocks()
        .iter()
        .rfind(|b| b.parent == Some(grid[0][0]))
        .unwrap();
    let selection = d.text_selection().unwrap();
    assert_eq!(selection.anchor, selection.head);
    assert_eq!(selection.head.block, last.id);
    assert_eq!(selection.head.offset, 0);
    let after = d.state.clone();
    d.undo().unwrap();
    assert_eq!(d.state, before);
    d.redo().unwrap();
    assert_eq!(d.state, after);
}

#[test]
fn structured_operation_marks_validate_before_positions_and_roll_back_prior_allocations() {
    let mut d = StructuredDocument::new("é");
    let at = d.position_at(0, Affinity::After).unwrap();
    let bad = Position {
        block: BlockId(u64::MAX),
        ..at
    };
    let before = d.state.clone();
    let next = d.next_block;
    assert_eq!(
        apply(
            &mut d,
            vec![
                Operation::Split { at },
                Operation::SetMarks {
                    range: TextSelection {
                        anchor: bad,
                        head: bad
                    },
                    marks: Marks {
                        link: Some("javascript:bad".into()),
                        ..Marks::default()
                    }
                },
            ]
        ),
        Err(StructuredError::InvalidLink)
    );
    assert_eq!(d.state, before);
    assert_eq!(d.next_block, next);
    assert_eq!(d.revision(), Revision(0));
    assert!(d.undo.is_empty());
}
