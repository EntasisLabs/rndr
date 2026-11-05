use super::*;

fn origin() -> EditOrigin {
    EditOrigin::Keyboard
}
fn op(d: &mut StructuredDocument, operation: Operation) -> Result<bool, StructuredError> {
    d.apply(Transaction {
        document: d.id(),
        base: d.revision(),
        origin: origin(),
        group: None,
        operations: vec![operation],
    })
}
fn table(d: &StructuredDocument) -> ContainerId {
    d.containers()
        .iter()
        .find(|c| c.kind == ContainerKind::Table)
        .unwrap()
        .id
}
fn caret(d: &mut StructuredDocument, cell: ContainerId) {
    let block = d.cell_first_block(cell).unwrap();
    let p = Position {
        block,
        offset: 0,
        affinity: Affinity::After,
    };
    d.set_selection(StructuredSelection::Text(TextSelection {
        anchor: p,
        head: p,
    }))
    .unwrap();
}
fn valid(d: &StructuredDocument) {
    normalize(&mut d.state.clone()).unwrap();
    validate_selection(&d.state, d.selection()).unwrap();
}
fn payload(name: &str) -> AtomicPayload {
    AtomicPayload {
        kind: "image".into(),
        fallback: name.into(),
        attributes: Attributes::from([("resource".into(), "opaque-host-key".into())]),
    }
}
#[test]
fn structured_table_creation_schema_and_navigation() {
    let mut d = StructuredDocument::new("left😀right");
    let p = Position {
        block: d.blocks()[0].id,
        offset: 4,
        affinity: Affinity::After,
    };
    d.set_selection(StructuredSelection::Text(TextSelection {
        anchor: p,
        head: p,
    }))
    .unwrap();
    d.insert_table(2, 3, true, origin()).unwrap();
    let t = table(&d);
    let cells = d.table_cells(t).unwrap();
    assert_eq!(d.blocks()[0].text(), "left");
    assert_eq!(d.blocks().last().unwrap().text(), "😀right");
    assert_eq!(cells.len(), 2);
    assert_eq!(cells[0].len(), 3);
    for (r, row) in cells.iter().enumerate() {
        for (c, &cell) in row.iter().enumerate() {
            let block = d.cell_first_block(cell).unwrap();
            assert_eq!(
                d.cell_address(block).unwrap(),
                Some(CellAddress {
                    table: t,
                    cell,
                    row: r,
                    column: c
                })
            );
            assert_eq!(
                container(&d.state, cell).unwrap().kind,
                ContainerKind::Cell { header: r == 0 }
            );
        }
    }
    assert!(!d.navigate_cell(true).unwrap());
    let rev = d.revision();
    for _ in 1..6 {
        assert!(d.navigate_cell(false).unwrap());
    }
    assert!(!d.navigate_cell(false).unwrap());
    assert_eq!(d.revision(), rev);
    for _ in 1..6 {
        assert!(d.navigate_cell(true).unwrap());
    }
    valid(&d);
    d.undo().unwrap();
    assert_eq!(d.plain_text(), "left😀right");
    d.redo().unwrap();
    assert_eq!(table(&d), t);
    assert_eq!(d.table_cells(t).unwrap(), cells);
    valid(&d);
}
#[test]
fn structured_table_all_row_column_edits_inverse_and_repair() {
    for row_axis in [true, false] {
        for insert in [true, false] {
            for index in 0..2 {
                let mut d = StructuredDocument::new("");
                d.insert_table(2, 2, true, origin()).unwrap();
                let t = table(&d);
                let grid = d.table_cells(t).unwrap();
                let selection = StructuredSelection::Cells {
                    anchor: grid[0][0],
                    head: grid[1][1],
                };
                d.set_selection(selection).unwrap();
                let before = d.state.clone();
                let operation = match (row_axis, insert) {
                    (true, true) => Operation::InsertRow { table: t, index },
                    (true, false) => Operation::DeleteRow { table: t, index },
                    (false, true) => Operation::InsertColumn { table: t, index },
                    (false, false) => Operation::DeleteColumn { table: t, index },
                };
                op(&mut d, operation).unwrap();
                let after = d.state.clone();
                valid(&d);
                d.undo().unwrap();
                assert_eq!(d.state.blocks, before.blocks);
                assert_eq!(d.state.containers, before.containers);
                assert_eq!(d.selection(), selection);
                d.redo().unwrap();
                assert_eq!(d.state.blocks, after.blocks);
                assert_eq!(d.state.containers, after.containers);
                valid(&d);
            }
        }
    }
}
#[test]
fn structured_table_last_axis_removal_and_nonreused_id() {
    let mut d = StructuredDocument::new("");
    d.insert_table(1, 1, false, origin()).unwrap();
    let old = table(&d);
    op(
        &mut d,
        Operation::DeleteColumn {
            table: old,
            index: 0,
        },
    )
    .unwrap();
    assert!(d.containers().is_empty());
    valid(&d);
    d.undo().unwrap();
    assert_eq!(table(&d), old);
    d.undo().unwrap();
    d.insert_table(1, 1, false, origin()).unwrap();
    assert_ne!(table(&d), old);
}
#[test]
fn structured_table_rich_rectangle_copy_paste_and_history() {
    let mut d = StructuredDocument::new("");
    d.insert_table(2, 2, true, origin()).unwrap();
    let t = table(&d);
    let grid = d.table_cells(t).unwrap();
    d.replace_selection("é😀", origin()).unwrap();
    let b = d.text_selection().unwrap().head.block;
    let a = Position {
        block: b,
        offset: 0,
        affinity: Affinity::Before,
    };
    let h = Position { offset: 6, ..a };
    d.set_selection(StructuredSelection::Text(TextSelection {
        anchor: a,
        head: h,
    }))
    .unwrap();
    d.set_marks(
        Marks {
            strong: true,
            ..Marks::default()
        },
        origin(),
    )
    .unwrap();
    d.wrap(ContainerKind::Quote, origin()).unwrap();
    let selected = StructuredSelection::Cells {
        anchor: grid[0][0],
        head: grid[0][0],
    };
    let fragment = d.copy_cells(selected).unwrap();
    assert_eq!(d.cells_plain_text(selected).unwrap(), "é😀");
    let before = d.state.clone();
    op(
        &mut d,
        Operation::PasteCells {
            at: grid[1][1],
            fragment,
        },
    )
    .unwrap();
    let copied = d
        .copy_cells(StructuredSelection::Cells {
            anchor: grid[1][1],
            head: grid[1][1],
        })
        .unwrap();
    assert_eq!(copied.rows[0][0].blocks[0].text(), "é😀");
    assert!(copied.rows[0][0].blocks[0].runs[0].marks.strong);
    assert_ne!(copied.rows[0][0].blocks[0].id, b);
    assert_eq!(copied.rows[0][0].containers[0].kind, ContainerKind::Quote);
    assert_eq!(d.table_cells(t).unwrap(), grid);
    valid(&d);
    d.undo().unwrap();
    assert_eq!(d.state.blocks, before.blocks);
    d.redo().unwrap();
    valid(&d);
}
#[test]
fn structured_table_text_boundaries_and_last_leaf_deletion() {
    let mut d = StructuredDocument::new("");
    d.insert_table(1, 2, false, origin()).unwrap();
    let grid = d.table_cells(table(&d)).unwrap();
    let a = Position {
        block: d.cell_first_block(grid[0][0]).unwrap(),
        offset: 0,
        affinity: Affinity::After,
    };
    let h = Position {
        block: d.cell_first_block(grid[0][1]).unwrap(),
        ..a
    };
    let before = d.state.clone();
    assert!(op(
        &mut d,
        Operation::Replace {
            range: TextSelection { anchor: a, head: h },
            text: "oops".into()
        }
    )
    .is_err());
    assert_eq!(d.state.blocks, before.blocks);
    op(&mut d, Operation::DeleteNode { block: a.block }).unwrap();
    assert_ne!(d.cell_first_block(grid[0][0]).unwrap(), a.block);
    assert_eq!(d.table_cells(table(&d)).unwrap(), grid);
    valid(&d);
    caret(&mut d, grid[0][0]);
    d.enter(origin()).unwrap();
    valid(&d);
}
#[test]
fn structured_atomic_payload_history_fallback_and_fences() {
    let mut d = StructuredDocument::new("text");
    d.insert_atomic(payload("an image"), origin()).unwrap();
    let StructuredSelection::Node(atom) = d.selection() else {
        panic!()
    };
    let b = d.blocks().iter().find(|b| b.id == atom).unwrap();
    assert_eq!(b.copy_fallback(), "an image");
    assert_eq!(b.atomic_payload().unwrap(), payload("an image"));
    assert!(d
        .display_offset(Position {
            block: atom,
            offset: 0,
            affinity: Affinity::After
        })
        .is_err());
    let before = d.state.clone();
    op(
        &mut d,
        Operation::UpdateAtomic {
            block: atom,
            payload: payload("nested commit"),
        },
    )
    .unwrap();
    assert_eq!(d.selection(), StructuredSelection::Node(atom));
    d.undo().unwrap();
    assert_eq!(d.state.blocks, before.blocks);
    d.redo().unwrap();
    assert_eq!(
        d.blocks()
            .iter()
            .find(|b| b.id == atom)
            .unwrap()
            .copy_fallback(),
        "nested commit"
    );
    d.replace_selection("replacement", origin()).unwrap();
    assert!(!d.blocks().iter().any(|b| b.id == atom));
    valid(&d);
    d.undo().unwrap();
    assert!(d.blocks().iter().any(|b| b.id == atom));
    d.set_readonly(true);
    assert_eq!(
        op(
            &mut d,
            Operation::UpdateAtomic {
                block: atom,
                payload: payload("refused")
            }
        ),
        Err(StructuredError::Readonly)
    );
}
#[test]
fn structured_table_fragment_atom_and_bad_fragment_are_atomic() {
    let mut d = StructuredDocument::new("");
    d.insert_table(1, 2, false, origin()).unwrap();
    let grid = d.table_cells(table(&d)).unwrap();
    d.insert_atomic(payload("img"), origin()).unwrap();
    let fragment = d
        .copy_cells(StructuredSelection::Cells {
            anchor: grid[0][0],
            head: grid[0][0],
        })
        .unwrap();
    op(
        &mut d,
        Operation::PasteCells {
            at: grid[0][1],
            fragment: fragment.clone(),
        },
    )
    .unwrap();
    assert!(d.cells_plain_text(d.selection()).unwrap().contains("img"));
    valid(&d);
    let before = d.state.clone();
    let mut invalid = fragment;
    invalid.rows[0][0].blocks[0].parent = Some(ContainerId(999999));
    assert!(op(
        &mut d,
        Operation::PasteCells {
            at: grid[0][1],
            fragment: invalid
        }
    )
    .is_err());
    assert_eq!(d.state.blocks, before.blocks);
    assert_eq!(d.state.containers, before.containers);
}

#[test]
fn structured_table_rectangle_typing_clear_and_inverse() {
    let mut d = StructuredDocument::new("");
    d.insert_table(2, 2, true, origin()).unwrap();
    let grid = d.table_cells(table(&d)).unwrap();
    let rectangle = StructuredSelection::Cells {
        anchor: grid[1][1],
        head: grid[0][0],
    };
    d.set_selection(rectangle).unwrap();
    let before = d.state.clone();
    d.replace_selection("hello\nworld", origin()).unwrap();
    assert_eq!(d.cells_plain_text(rectangle).unwrap(), "hello\nworld\t\n\t");
    assert!(d.text_selection().is_some());
    assert_eq!(d.table_cells(table(&d)).unwrap(), grid);
    valid(&d);
    d.undo().unwrap();
    assert_eq!(d.state.blocks, before.blocks);
    assert_eq!(d.selection(), rectangle);
    d.redo().unwrap();
    d.set_selection(rectangle).unwrap();
    d.backspace(origin()).unwrap();
    assert_eq!(d.cells_plain_text(rectangle).unwrap(), "\t\n\t");
    valid(&d);
}
#[test]
fn structured_table_invalid_schema_readonly_composition_and_transaction_rollback() {
    let mut d = StructuredDocument::new("");
    for (r, c) in [(0, 1), (1, 0), (257, 1), (1, 65)] {
        assert!(d.insert_table(r, c, false, origin()).is_err());
        assert!(d.containers().is_empty());
        assert_eq!(d.revision(), Revision(0));
    }
    d.insert_table(2, 2, false, origin()).unwrap();
    let t = table(&d);
    let grid = d.table_cells(t).unwrap();
    let before = d.state.clone();
    assert!(d.insert_table(1, 1, false, origin()).is_err());
    let tx = Transaction {
        document: d.id(),
        base: d.revision(),
        origin: origin(),
        group: None,
        operations: vec![
            Operation::InsertRow { table: t, index: 0 },
            Operation::DeleteColumn {
                table: t,
                index: 99,
            },
        ],
    };
    assert!(d.apply(tx).is_err());
    assert_eq!(d.state.blocks, before.blocks);
    assert_eq!(d.state.containers, before.containers);
    d.set_readonly(true);
    assert_eq!(
        op(&mut d, Operation::DeleteRow { table: t, index: 0 }),
        Err(StructuredError::Readonly)
    );
    assert!(d.navigate_cell(false).unwrap());
    d.set_readonly(false);
    caret(&mut d, grid[0][0]);
    d.begin_composition().unwrap();
    assert_eq!(
        op(&mut d, Operation::InsertColumn { table: t, index: 0 }),
        Err(StructuredError::CompositionActive)
    );
    assert_eq!(
        d.navigate_cell(false),
        Err(StructuredError::CompositionActive)
    );
    d.commit_composition("中").unwrap();
    assert_eq!(
        d.blocks()
            .iter()
            .find(|b| b.id == d.cell_first_block(grid[0][0]).unwrap())
            .unwrap()
            .text(),
        "中"
    );
    valid(&d);
}
#[test]
fn structured_atomic_bad_payload_stale_nested_callback_and_adjacent_atoms() {
    let mut d = StructuredDocument::new("");
    let mut bad = payload("bad\ncontrol");
    assert!(d.insert_atomic(bad.clone(), origin()).is_err());
    bad.fallback = "valid".into();
    bad.kind = "bad kind".into();
    assert!(d.insert_atomic(bad, origin()).is_err());
    d.insert_atomic(payload("first"), origin()).unwrap();
    let StructuredSelection::Node(first) = d.selection() else {
        panic!()
    };
    let right = d.blocks().last().unwrap().id;
    let p = Position {
        block: right,
        offset: 0,
        affinity: Affinity::After,
    };
    d.set_selection(StructuredSelection::Text(TextSelection {
        anchor: p,
        head: p,
    }))
    .unwrap();
    d.insert_atomic(payload("second"), origin()).unwrap();
    let StructuredSelection::Node(second) = d.selection() else {
        panic!()
    };
    assert_ne!(first, second);
    let stale = Transaction {
        document: d.id(),
        base: d.revision(),
        origin: EditOrigin::Host(77),
        group: None,
        operations: vec![Operation::UpdateAtomic {
            block: first,
            payload: payload("late"),
        }],
    };
    op(&mut d, Operation::DeleteNode { block: first }).unwrap();
    assert_eq!(d.apply(stale), Err(StructuredError::StaleRevision));
    assert!(op(
        &mut d,
        Operation::UpdateAtomic {
            block: first,
            payload: payload("late")
        }
    )
    .is_err());
    assert_eq!(
        d.blocks()
            .iter()
            .filter(|b| b.kind == BlockKind::Atomic)
            .count(),
        1
    );
    valid(&d);
    d.undo().unwrap();
    assert!(d.blocks().iter().any(|b| b.id == first));
    valid(&d);
}
