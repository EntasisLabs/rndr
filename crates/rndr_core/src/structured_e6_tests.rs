use super::*;

#[test]
fn structured_e6_middle_outdent_preserves_order_and_ordered_continuation() {
    let mut d = StructuredDocument::new("a\nb\nc");
    list(&mut d, ListKind::Ordered { start: 7 });
    select(&mut d, (1, 0), (1, 0));
    d.outdent(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb\nc");
    assert_eq!(d.blocks()[1].parent, None);
    assert_eq!(d.list_number(d.blocks()[0].id).unwrap(), Some(7));
    assert_eq!(d.list_number(d.blocks()[1].id).unwrap(), None);
    assert_eq!(d.list_number(d.blocks()[2].id).unwrap(), Some(9));
    assert_valid(&d);
    let mut d = StructuredDocument::new("a\nb\nc\nd");
    list(&mut d, ListKind::Task);
    select(&mut d, (1, 0), (1, 0));
    d.indent(origin()).unwrap();
    select(&mut d, (2, 0), (2, 0));
    d.indent(origin()).unwrap();
    let before = d.state.clone();
    select(&mut d, (1, 0), (1, 0));
    d.outdent(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb\nc\nd");
    let b_item = d.blocks()[1].parent.unwrap();
    assert!(d
        .ancestors(d.blocks()[2].id)
        .unwrap()
        .iter()
        .any(|c| c.id == b_item));
    assert_valid(&d);
    d.undo().unwrap();
    assert_eq!(d.state.blocks, before.blocks);
    assert_eq!(d.state.containers, before.containers);
}

#[test]
fn structured_e6_quote_middle_exit_splits_without_reordering() {
    let mut d = StructuredDocument::new("a\nb\nc");
    select(&mut d, (0, 0), (2, 1));
    d.wrap_selection(ContainerKind::Quote, origin()).unwrap();
    select(&mut d, (1, 0), (1, 0));
    d.outdent(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb\nc");
    assert_eq!(d.blocks()[1].parent, None);
    assert_ne!(d.blocks()[0].parent, d.blocks()[2].parent);
    assert_valid(&d);
}

#[test]
fn structured_e6_all_unicode_cross_container_replacements_are_atomic_inverses() {
    let mut base = StructuredDocument::new("é😀\n中x\nαz");
    list(&mut base, ListKind::Task);
    select(&mut base, (1, 0), (1, 0));
    base.indent(origin()).unwrap();
    let positions: Vec<_> = base
        .blocks()
        .iter()
        .flat_map(|b| {
            let t = b.text();
            (0..=t.len())
                .filter(|&n| t.is_char_boundary(n))
                .map(|offset| Position {
                    block: b.id,
                    offset,
                    affinity: Affinity::After,
                })
                .collect::<Vec<_>>()
        })
        .collect();
    for &anchor in &positions {
        for &head in &positions {
            for text in ["", "Q", "é\n😀", "\n\n"] {
                let mut d = StructuredDocument::new("");
                d.state = base.state.clone();
                d.next_block = base.next_block;
                d.set_selection(StructuredSelection::Text(TextSelection { anchor, head }))
                    .unwrap();
                let before = d.state.clone();
                let changed = d.replace_selection(text, origin()).unwrap();
                assert_valid(&d);
                if changed {
                    let after = d.state.clone();
                    d.undo().unwrap();
                    assert_eq!(d.state, before);
                    d.redo().unwrap();
                    assert_eq!(d.state, after);
                }
            }
        }
    }
}

fn origin() -> EditOrigin {
    EditOrigin::Keyboard
}
fn pos(d: &StructuredDocument, i: usize, offset: usize) -> Position {
    Position {
        block: d.blocks()[i].id,
        offset,
        affinity: Affinity::After,
    }
}
fn select(d: &mut StructuredDocument, a: (usize, usize), z: (usize, usize)) {
    let range = TextSelection {
        anchor: pos(d, a.0, a.1),
        head: pos(d, z.0, z.1),
    };
    d.set_selection(StructuredSelection::Text(range)).unwrap();
}
fn apply(d: &mut StructuredDocument, operations: Vec<Operation>) -> Result<bool, StructuredError> {
    d.apply(Transaction {
        document: d.id(),
        base: d.revision(),
        origin: origin(),
        group: None,
        operations,
    })
}
fn list(d: &mut StructuredDocument, kind: ListKind) {
    let n = d.blocks().len() - 1;
    let end = d.blocks()[n].len();
    select(d, (0, 0), (n, end));
    d.wrap_selection(ContainerKind::List(kind), origin())
        .unwrap();
}
fn assert_valid(d: &StructuredDocument) {
    let mut s = d.state.clone();
    normalize(&mut s).unwrap();
    assert_eq!(s, d.state);
    validate_selection(&s, s.selection).unwrap();
    assert!(s.blocks.iter().any(|b| b.kind != BlockKind::HorizontalRule));
}

#[test]
fn structured_e6_nested_task_identity_toggle_and_inverse() {
    let mut d = StructuredDocument::new("one\ntwo\nthree");
    let ids: Vec<_> = d.blocks().iter().map(|b| b.id).collect();
    list(&mut d, ListKind::Task);
    select(&mut d, (1, 2), (1, 0));
    let directed = d.selection();
    let before = d.state.clone();
    d.indent(origin()).unwrap();
    let ancestors = d.ancestors(ids[1]).unwrap();
    assert_eq!(ancestors.len(), 4);
    assert!(matches!(
        ancestors[0].kind,
        ContainerKind::Item {
            checked: Some(false)
        }
    ));
    let nested_id = ancestors[1].id;
    let after_indent = d.state.clone();
    d.toggle_task(origin()).unwrap();
    assert_eq!(d.selection(), directed);
    assert!(matches!(
        d.ancestors(ids[1]).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(true)
        }
    ));
    d.undo().unwrap();
    assert_eq!(d.state, after_indent);
    d.undo().unwrap();
    assert_eq!(d.state, before);
    d.redo().unwrap();
    assert_eq!(d.state, after_indent);
    d.outdent(origin()).unwrap();
    assert_eq!(d.blocks().iter().map(|b| b.id).collect::<Vec<_>>(), ids);
    assert_eq!(d.ancestors(ids[1]).unwrap().len(), 2);
    assert!(!d.containers().iter().any(|c| c.id == nested_id));
    assert_valid(&d);
}

#[test]
fn structured_e6_indent_moves_subtree_and_promotes_after_parent_subtree() {
    let mut d = StructuredDocument::new("a\nb\nc\nd");
    list(&mut d, ListKind::Bullet);
    select(&mut d, (2, 0), (2, 0));
    d.indent(origin()).unwrap(); // c below b
    select(&mut d, (1, 0), (1, 0));
    d.indent(origin()).unwrap(); // b and its c below a
    assert_eq!(d.ancestors(d.blocks()[2].id).unwrap().len(), 6);
    select(&mut d, (1, 0), (1, 0));
    d.outdent(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb\nc\nd");
    assert_eq!(d.ancestors(d.blocks()[2].id).unwrap().len(), 4);
    assert_valid(&d);
}

#[test]
fn structured_e6_enter_split_task_paste_and_empty_exit() {
    let mut d = StructuredDocument::new("é😀\nnext");
    list(&mut d, ListKind::Task);
    select(&mut d, (0, 2), (0, 2));
    d.toggle_task(origin()).unwrap();
    let left = d.blocks()[0].id;
    d.enter(origin()).unwrap();
    assert_eq!(d.plain_text(), "é\n😀\nnext");
    assert_eq!(d.blocks()[0].id, left);
    assert!(matches!(
        d.ancestors(d.blocks()[0].id).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(true)
        }
    ));
    assert!(matches!(
        d.ancestors(d.blocks()[1].id).unwrap()[0].kind,
        ContainerKind::Item {
            checked: Some(false)
        }
    ));
    d.replace_selection("x\r\ny\r", origin()).unwrap();
    assert_eq!(d.plain_text(), "é\nx\ny\n😀\nnext");
    assert_valid(&d);
    let mut d = StructuredDocument::new("item");
    list(&mut d, ListKind::Bullet);
    select(&mut d, (0, 4), (0, 4));
    d.enter(origin()).unwrap();
    d.enter(origin()).unwrap();
    assert_eq!(d.plain_text(), "item\n");
    assert_eq!(d.blocks()[1].parent, None);
    assert_valid(&d);
}

#[test]
fn structured_e6_split_item_keeps_nested_children_on_right_sibling() {
    let mut d = StructuredDocument::new("ab\nchild\nend");
    list(&mut d, ListKind::Bullet);
    select(&mut d, (1, 0), (1, 0));
    d.indent(origin()).unwrap();
    select(&mut d, (0, 1), (0, 1));
    d.enter(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb\nchild\nend");
    let right_item = d.blocks()[1].parent.unwrap();
    assert!(d
        .ancestors(d.blocks()[2].id)
        .unwrap()
        .iter()
        .any(|c| c.id == right_item));
    assert_valid(&d);
}

#[test]
fn structured_e6_quote_nesting_and_exit_preserve_stable_positions() {
    let mut d = StructuredDocument::new("a\nb\nc");
    select(&mut d, (0, 0), (2, 1));
    d.wrap_selection(ContainerKind::Quote, origin()).unwrap();
    select(&mut d, (1, 1), (1, 0));
    let directed = d.selection();
    d.wrap_selection(ContainerKind::Quote, origin()).unwrap();
    assert_eq!(d.selection(), directed);
    assert_eq!(d.ancestors(d.blocks()[1].id).unwrap().len(), 2);
    d.outdent(origin()).unwrap();
    assert_eq!(d.selection(), directed);
    assert_eq!(d.ancestors(d.blocks()[1].id).unwrap().len(), 1);
    assert_valid(&d);
}

#[test]
fn structured_e6_cross_container_unicode_replacement_and_undo() {
    let mut d = StructuredDocument::new("é😀\n中x\nz");
    list(&mut d, ListKind::Task);
    select(&mut d, (1, 0), (1, 0));
    d.indent(origin()).unwrap();
    select(&mut d, (1, 3), (0, 2));
    let before = d.state.clone();
    d.replace_selection("α\nβ", origin()).unwrap();
    assert_eq!(d.plain_text(), "éα\nβx\nz");
    assert_valid(&d);
    d.undo().unwrap();
    assert_eq!(d.state, before);
    d.redo().unwrap();
    assert_valid(&d);
}

#[test]
fn structured_e6_horizontal_rule_is_nontext_and_repairs_final_leaf() {
    let mut d = StructuredDocument::new("ab");
    let at = pos(&d, 0, 1);
    let before = d.state.clone();
    apply(&mut d, vec![Operation::InsertHorizontalRule { at }]).unwrap();
    assert_eq!(d.blocks().len(), 3);
    let hr = d.blocks()[1].id;
    assert_eq!(d.selection(), StructuredSelection::Node(hr));
    assert_eq!(
        d.display_offset(pos(&d, 1, 0)),
        Err(StructuredError::InvalidPosition)
    );
    assert!(d
        .set_selection(StructuredSelection::Text(TextSelection {
            anchor: pos(&d, 1, 0),
            head: pos(&d, 1, 0)
        }))
        .is_err());
    assert!(d.blocks()[1].runs.is_empty());
    let left = d.blocks()[0].id;
    let right = d.blocks()[2].id;
    apply(
        &mut d,
        vec![
            Operation::DeleteNode { block: left },
            Operation::DeleteNode { block: right },
        ],
    )
    .unwrap();
    assert!(d.blocks().iter().any(|b| b.kind == BlockKind::Paragraph));
    assert_valid(&d);
    d.set_selection(StructuredSelection::Node(hr)).unwrap();
    d.replace_selection("hello", origin()).unwrap();
    assert_eq!(d.plain_text(), "hello");
    assert!(matches!(d.selection(), StructuredSelection::Text(_)));
    d.undo().unwrap();
    assert_eq!(d.selection(), StructuredSelection::Node(hr));
    d.undo().unwrap();
    d.undo().unwrap();
    assert_eq!(d.state, before);
    assert_valid(&d);
}

#[test]
fn structured_e6_horizontal_rule_adjacent_delete_selects_then_removes() {
    let mut d = StructuredDocument::new("ab");
    let at = pos(&d, 0, 1);
    apply(&mut d, vec![Operation::InsertHorizontalRule { at }]).unwrap();
    select(&mut d, (0, 1), (0, 1));
    let rev = d.revision();
    assert!(!d.delete_forward(origin()).unwrap());
    assert_eq!(d.revision(), rev);
    assert!(matches!(d.selection(), StructuredSelection::Node(_)));
    d.delete_forward(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb");
    d.undo().unwrap();
    select(&mut d, (2, 0), (2, 0));
    assert!(!d.backspace(origin()).unwrap());
    assert!(matches!(d.selection(), StructuredSelection::Node(_)));
    d.backspace(origin()).unwrap();
    assert_eq!(d.plain_text(), "a\nb");
    assert_valid(&d);
}

#[test]
fn structured_e6_attributes_style_schema_and_noop_revision() {
    let mut d = StructuredDocument::new("abc");
    let block = d.blocks()[0].id;
    let attrs = Attributes::from([
        ("app-id".into(), "note-1".into()),
        ("footnote".into(), "inert".into()),
    ]);
    apply(
        &mut d,
        vec![Operation::SetAttributes {
            block,
            attributes: attrs.clone(),
        }],
    )
    .unwrap();
    let rev = d.revision();
    assert!(!apply(
        &mut d,
        vec![Operation::SetAttributes {
            block,
            attributes: attrs
        }]
    )
    .unwrap());
    assert_eq!(d.revision(), rev);
    select(&mut d, (0, 3), (0, 0));
    let selection = d.selection();
    d.set_marks(
        Marks {
            text_style: TextStyle {
                color: Some([1, 2, 3]),
                family: Some("Serif".into()),
            },
            ..Marks::default()
        },
        origin(),
    )
    .unwrap();
    assert_eq!(d.selection(), selection);
    let before = d.state.clone();
    let range = d.text_selection().unwrap();
    for style in [
        TextStyle {
            family: Some(String::new()),
            ..TextStyle::default()
        },
        TextStyle {
            family: Some("bad;url(x)".into()),
            ..TextStyle::default()
        },
    ] {
        assert_eq!(
            apply(
                &mut d,
                vec![Operation::SetMarks {
                    range,
                    marks: Marks {
                        text_style: style,
                        ..Marks::default()
                    }
                }]
            ),
            Err(StructuredError::InvalidSchema)
        );
        assert_eq!(d.state, before);
    }
    for attrs in [
        Attributes::from([("bad key".into(), "x".into())]),
        Attributes::from([("good".into(), "bad\nvalue".into())]),
    ] {
        assert_eq!(
            apply(
                &mut d,
                vec![Operation::SetAttributes {
                    block,
                    attributes: attrs
                }]
            ),
            Err(StructuredError::InvalidSchema)
        );
        assert_eq!(d.state, before);
    }
    assert_valid(&d);
}

#[test]
fn structured_e6_atomic_invalid_structure_and_monotonic_identity() {
    let mut d = StructuredDocument::new("a\nb");
    let range = TextSelection {
        anchor: pos(&d, 0, 0),
        head: pos(&d, 1, 1),
    };
    let before = d.state.clone();
    let next = d.next_block;
    assert_eq!(
        apply(
            &mut d,
            vec![
                Operation::Wrap {
                    range,
                    kind: ContainerKind::Quote
                },
                Operation::Wrap {
                    range,
                    kind: ContainerKind::List(ListKind::Ordered { start: 0 })
                }
            ]
        ),
        Err(StructuredError::InvalidSchema)
    );
    assert_eq!(d.state, before);
    assert_eq!(d.next_block, next);
    assert_eq!(d.revision(), Revision(0));
    assert!(d.undo.is_empty());
    apply(
        &mut d,
        vec![Operation::Wrap {
            range,
            kind: ContainerKind::List(ListKind::Ordered { start: 7 }),
        }],
    )
    .unwrap();
    let max_id = d.next_block;
    let snapshot = d.state.clone();
    d.undo().unwrap();
    d.redo().unwrap();
    assert_eq!(d.state, snapshot);
    d.undo().unwrap();
    apply(
        &mut d,
        vec![Operation::Wrap {
            range,
            kind: ContainerKind::Quote,
        }],
    )
    .unwrap();
    assert!(d.containers()[0].id.0 >= max_id);
    assert_valid(&d);
}

#[test]
fn structured_e6_readonly_and_composition_refuse_structural_commands() {
    let mut d = StructuredDocument::new("a\nb");
    list(&mut d, ListKind::Task);
    select(&mut d, (1, 0), (1, 0));
    let before = d.state.clone();
    d.set_readonly(true);
    assert_eq!(d.indent(origin()), Err(StructuredError::Readonly));
    assert_eq!(d.toggle_task(origin()), Err(StructuredError::Readonly));
    assert_eq!(d.enter(origin()), Err(StructuredError::Readonly));
    assert_eq!(d.backspace(origin()), Err(StructuredError::Readonly));
    assert_eq!(d.state, before);
    d.set_readonly(false);
    d.begin_composition().unwrap();
    assert_eq!(d.indent(origin()), Err(StructuredError::CompositionActive));
    assert_eq!(d.outdent(origin()), Err(StructuredError::CompositionActive));
    assert_eq!(
        d.toggle_task(origin()),
        Err(StructuredError::CompositionActive)
    );
    assert_eq!(d.enter(origin()), Err(StructuredError::CompositionActive));
    assert_eq!(d.state, before);
    d.cancel_composition();
    d.commit_composition("x").unwrap_err();
    assert_valid(&d);
}
