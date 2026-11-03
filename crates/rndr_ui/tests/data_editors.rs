//! Public composition tests; native input scheduling, not only model setters.
use rndr_core::*;
use rndr_ui::{
    collections::{CollectionEvent, CollectionEventKind, Item, ItemId},
    controls,
    data::*,
    *,
};
fn setup() -> (App, Entity) {
    let mut a = App::new();
    a.add_plugin(UiPlugin);
    a.world_mut().insert_resource(Viewport::new(900., 900.));
    let root = a.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    (a, root)
}
fn frame(a: &mut App) {
    rndr_paint::frame(a);
}
fn batch(a: &mut App, keys: Vec<KeyEvent>) {
    a.world_mut().resource_mut::<Input>().unwrap().keys = keys;
    frame(a);
}
fn key(k: NamedKey) -> KeyEvent {
    KeyEvent::named(k, Modifiers::default())
}
fn ch(c: char) -> KeyEvent {
    KeyEvent::character(c, Modifiers::default())
}
fn columns() -> Vec<Column> {
    vec![
        Column {
            id: ColumnId(1),
            label: "Name".into(),
            width: 200.,
            alignment: CellAlignment::Start,
            sortable: true,
        },
        Column {
            id: ColumnId(2),
            label: "Count".into(),
            width: 100.,
            alignment: CellAlignment::End,
            sortable: false,
        },
    ]
}
fn rows() -> Vec<Item> {
    (1..=3)
        .map(|id| Item::new(id, format!("Record {id}")))
        .collect()
}
fn table(
    a: &mut App,
    r: Entity,
    props: TableProps,
    c: &[Column],
    rows: &[Item],
) -> Result<TableHandle, UiError> {
    DataTable::mount(
        a.world_mut(),
        r,
        UiId(40),
        props,
        c,
        rows,
        |w, s, row, col| {
            let text = format!("{}:{}", row.id.0, col.id.0);
            let children = children_of(w, s.content);
            if children.is_empty() {
                rndr_ui::text(w, s.content, &text, Typography::Label).unwrap();
            } else {
                w.get_mut::<Label>(children[0]).unwrap().text = text;
            }
        },
    )
}
fn groups() -> Vec<PropertyGroup> {
    vec![
        PropertyGroup {
            id: GroupId(1),
            label: "General".into(),
        },
        PropertyGroup {
            id: GroupId(2),
            label: "Details".into(),
        },
    ]
}
fn properties() -> Vec<Property> {
    [
        (
            PropertyKind::Text { max_chars: 8 },
            PropertyValue::Text("A".into()),
        ),
        (
            PropertyKind::Number { min: 0., max: 10. },
            PropertyValue::Number(2.),
        ),
        (PropertyKind::Boolean, PropertyValue::Boolean(false)),
        (
            PropertyKind::Choice(vec![
                Choice {
                    id: 1,
                    label: "One".into(),
                },
                Choice {
                    id: 2,
                    label: "Two".into(),
                },
            ]),
            PropertyValue::Choice(1),
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(n, (kind, value))| Property {
        id: PropertyId(n as u64 + 1),
        group: GroupId(1),
        label: format!("Property {}", n + 1),
        description: "Synthetic property".into(),
        kind,
        value,
        enabled: true,
        readonly: false,
        validation: None,
        validate: None,
    })
    .collect()
}
fn inspector(
    a: &mut App,
    r: Entity,
    object: u64,
    fields: &[Property],
) -> Result<InspectorHandle, UiError> {
    PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(50),
        InspectorProps::new("Inspector", ObjectId(object)),
        &groups(),
        fields,
        |_, _, _| {},
    )
}
fn property_events(a: &App) -> Vec<PropertyEventKind> {
    events::<PropertyEvent>(a.world())
        .iter()
        .map(|e| e.kind.clone())
        .collect()
}
#[test]
fn table_preserves_cells_selection_focus_across_row_column_reorder_and_resize() {
    let (mut a, r) = setup();
    let mut c = columns();
    let mut rows = rows();
    let h = table(&mut a, r, TableProps::new("Table"), &c, &rows).unwrap();
    frame(&mut a);
    let cell = h.cell(a.world(), ItemId(2), ColumnId(1)).unwrap();
    let content = children_of(a.world(), cell.content);
    h.rows.select(a.world_mut(), Some(ItemId(2))).unwrap();
    h.rows.focus(a.world_mut(), ItemId(3)).unwrap();
    rows.reverse();
    c.reverse();
    c[1].width = 350.;
    assert_eq!(
        h,
        table(&mut a, r, TableProps::new("Table"), &c, &rows).unwrap()
    );
    frame(&mut a);
    assert_eq!(h.rows.selected(a.world()), Some(ItemId(2)));
    assert_eq!(h.rows.focused(a.world()), Some(ItemId(3)));
    assert_eq!(
        h.cell(a.world(), ItemId(2), ColumnId(1)).unwrap().root,
        cell.root
    );
    assert_eq!(children_of(a.world(), cell.content), content);
    assert_eq!(a.world().get::<Rect>(cell.root).unwrap().w, 350.);
    rows.retain(|x| x.id != ItemId(2));
    table(&mut a, r, TableProps::new("Table"), &c, &rows).unwrap();
    assert!(!a.world().is_alive(cell.root));
    assert_eq!(h.rows.selected(a.world()), None);
}
#[test]
fn table_keyboard_sort_request_never_sorts_host_data_and_semantics_are_table_rows_cells() {
    let (mut a, r) = setup();
    let h = table(&mut a, r, TableProps::new("Table"), &columns(), &rows()).unwrap();
    frame(&mut a);
    let header = h.header(a.world(), ColumnId(1)).unwrap();
    assert!(request_focus(a.world_mut(), header));
    batch(&mut a, vec![key(NamedKey::Enter)]);
    assert_eq!(events::<SortRequested>(a.world()).len(), 1);
    assert_eq!(
        events::<SortRequested>(a.world())[0].sort.direction,
        SortDirection::Ascending
    );
    let mut p = TableProps::new("Table");
    p.sort = Some(SortState {
        column: ColumnId(1),
        direction: SortDirection::Ascending,
    });
    table(&mut a, r, p, &columns(), &rows()).unwrap();
    batch(&mut a, vec![key(NamedKey::Enter)]);
    assert_eq!(
        events::<SortRequested>(a.world())[0].sort.direction,
        SortDirection::Descending
    );
    h.rows.focus(a.world_mut(), ItemId(1)).unwrap();
    batch(
        &mut a,
        vec![key(NamedKey::ArrowDown), key(NamedKey::Space), {
            let mut e = key(NamedKey::Space);
            e.released = true;
            e
        }],
    );
    assert_eq!(h.rows.selected(a.world()), Some(ItemId(2)));
    assert_eq!(h.rows.focused(a.world()), Some(ItemId(2)));
    batch(&mut a, vec![key(NamedKey::Enter)]);
    assert!(events::<CollectionEvent>(a.world())
        .iter()
        .any(|e| e.kind == CollectionEventKind::Activated(ItemId(2))));
    let tree = access_tree(a.world());
    assert!(tree
        .nodes
        .iter()
        .any(|n| n.id == access_id(h.root) && n.role == AccessRole::Table));
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|n| n.role == AccessRole::Cell)
            .count(),
        6
    );
}
#[test]
fn table_invalid_atomic_identity_conflict_disabled_and_unmount() {
    let (mut a, r) = setup();
    let h = table(&mut a, r, TableProps::new("Table"), &columns(), &rows()).unwrap();
    let before = children_of(a.world(), h.root);
    let mut bad = columns();
    bad[1].id = ColumnId(1);
    assert_eq!(
        table(&mut a, r, TableProps::new("Table"), &bad, &rows()),
        Err(UiError::InvalidProps)
    );
    assert_eq!(children_of(a.world(), h.root), before);
    assert!(matches!(
        controls::Button::mount(
            a.world_mut(),
            r,
            UiId(40),
            controls::ButtonProps::new("Conflict", controls::HostIntent::Command(1))
        ),
        Err(UiError::IdentityConflict)
    ));
    let mut p = TableProps::new("Table");
    p.enabled = false;
    table(&mut a, r, p, &columns(), &rows()).unwrap();
    frame(&mut a);
    let header = h.header(a.world(), ColumnId(1)).unwrap();
    assert!(!request_focus(a.world_mut(), header));
    h.unmount(a.world_mut()).unwrap();
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
}
#[test]
fn inspector_precise_multiple_commit_boundaries_and_no_hidden_model_mutation() {
    let (mut a, r) = setup();
    let p = properties();
    let h = inspector(&mut a, r, 1, &p).unwrap();
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    batch(
        &mut a,
        vec![ch('1'), key(NamedKey::Enter), ch('2'), key(NamedKey::Enter)],
    );
    let commits: Vec<_> = property_events(&a)
        .into_iter()
        .filter_map(|e| {
            if let PropertyEventKind::CommitRequested { before, after } = e {
                Some((before, after))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        commits,
        vec![
            (
                PropertyValue::Text("A".into()),
                PropertyValue::Text("A1".into())
            ),
            (
                PropertyValue::Text("A".into()),
                PropertyValue::Text("A12".into())
            )
        ]
    );
    assert_eq!(p[0].value, PropertyValue::Text("A".into()));
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    let mut accepted = p.clone();
    accepted[0].value = PropertyValue::Text("A12".into());
    assert_eq!(h, inspector(&mut a, r, 1, &accepted).unwrap());
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A12");
}
#[test]
fn inspector_invalid_number_range_nonfinite_validation_and_cancel() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    for raw in ["-", "NaN", "inf", "11", "-1"] {
        h.set_draft(a.world_mut(), PropertyId(2), raw).unwrap();
        assert!(h.draft(a.world(), PropertyId(2)).unwrap().error.is_some());
        assert!(!h.commit(a.world_mut(), PropertyId(2)).unwrap());
    }
    h.cancel(a.world_mut(), PropertyId(2), CancelReason::Escape)
        .unwrap();
    assert_eq!(h.draft(a.world(), PropertyId(2)).unwrap().raw, "2");
    h.set_draft(a.world_mut(), PropertyId(2), "3.5").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(2)).unwrap());
    h.set_draft(a.world_mut(), PropertyId(1), "too long text")
        .unwrap();
    assert!(h.draft(a.world(), PropertyId(1)).unwrap().error.is_some());
}
#[test]
fn inspector_object_switch_cancels_old_identity_and_clears_history() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let old = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), old.editor);
    batch(&mut a, vec![ch('x')]);
    let mut p = properties();
    p[0].value = PropertyValue::Text("B".into());
    inspector(&mut a, r, 2, &p).unwrap();
    frame(&mut a);
    assert!(!a.world().is_alive(old.editor));
    assert!(events::<PropertyEvent>(a.world())
        .iter()
        .any(|e| e.object == ObjectId(1)
            && e.kind == PropertyEventKind::Cancelled(CancelReason::Reconciled)));
    let editor = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), editor);
    let mut modifiers = Modifiers::default();
    modifiers.super_key = Platform::host() == Platform::Mac;
    modifiers.ctrl = !modifiers.super_key;
    batch(&mut a, vec![KeyEvent::character('z', modifiers)]);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "B");
}
#[test]
fn inspector_same_identity_draft_retained_then_reconciled_disabled_readonly_removed_unmount() {
    let (mut a, r) = setup();
    let mut p = properties();
    let h = inspector(&mut a, r, 1, &p).unwrap();
    h.set_draft(a.world_mut(), PropertyId(1), "draft").unwrap();
    inspector(&mut a, r, 1, &p).unwrap();
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "draft");
    p[0].readonly = true;
    inspector(&mut a, r, 1, &p).unwrap();
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A");
    assert!(h.set_draft(a.world_mut(), PropertyId(1), "x").is_err());
    p[1].enabled = false;
    inspector(&mut a, r, 1, &p).unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(2)).is_err());
    let old = h.slot(a.world(), PropertyId(3)).unwrap();
    p.remove(2);
    inspector(&mut a, r, 1, &p).unwrap();
    assert!(!a.world().is_alive(old.root));
    h.unmount(a.world_mut()).unwrap();
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
}
#[test]
fn inspector_tab_cancels_at_boundary_before_other_field_changes_and_commit_action_keeps_draft() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let slot = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), slot.editor);
    batch(&mut a, vec![ch('x'), key(NamedKey::Tab)]);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "Ax");
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(slot.commit)
    );
    batch(&mut a, vec![key(NamedKey::Enter)]);
    assert!(property_events(&a)
        .iter()
        .any(|e| matches!(e, PropertyEventKind::CommitRequested { .. })));
    request_focus(a.world_mut(), slot.editor);
    batch(&mut a, vec![ch('y')]);
    let other = h.slot(a.world(), PropertyId(2)).unwrap().editor;
    request_focus(a.world_mut(), other);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A");
}
#[test]
fn inspector_boolean_choice_keyboard_explicit_commit_and_invalid_mount_atomic() {
    let (mut a, r) = setup();
    let p = properties();
    let h = inspector(&mut a, r, 1, &p).unwrap();
    frame(&mut a);
    for id in [PropertyId(3), PropertyId(4)] {
        let slot = h.slot(a.world(), id).unwrap();
        request_focus(a.world_mut(), slot.editor);
        // Checkbox toggles; Choice now opens a real popup, then chooses explicitly.
        let keys = if id == PropertyId(4) {
            vec![
                key(NamedKey::Enter),
                key(NamedKey::ArrowDown),
                key(NamedKey::Enter),
            ]
        } else {
            vec![key(NamedKey::Enter)]
        };
        batch(&mut a, keys);
        assert!(!property_events(&a)
            .iter()
            .any(|e| matches!(e, PropertyEventKind::CommitRequested { .. })));
        assert!(h.commit(a.world_mut(), id).unwrap());
        frame(&mut a);
        frame(&mut a);
    }
    let mut bad = p.clone();
    bad[0].value = PropertyValue::Number(9.);
    assert_eq!(inspector(&mut a, r, 1, &bad), Err(UiError::InvalidProps));
    assert!(h.validate(a.world()).is_ok());
}
#[test]
fn grouping_reparent_custom_draft_validator_and_stale_handles_are_safe() {
    let (mut a, r) = setup();
    let mut p = properties();
    fn deny_bad(v: &PropertyValue) -> Result<(), String> {
        if *v == PropertyValue::Text("bad".into()) {
            Err("Reserved name".into())
        } else {
            Ok(())
        }
    }
    p[0].validate = Some(deny_bad);
    let h = inspector(&mut a, r, 1, &p).unwrap();
    h.set_draft(a.world_mut(), PropertyId(1), "bad").unwrap();
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().error.as_deref(),
        Some("Reserved name")
    );
    assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    p[0].group = GroupId(2);
    p.reverse();
    let g = vec![
        PropertyGroup {
            id: GroupId(2),
            label: "New group".into(),
        },
        PropertyGroup {
            id: GroupId(1),
            label: "Old group".into(),
        },
    ];
    PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(50),
        InspectorProps::new("Inspector", ObjectId(1)),
        &g,
        &p,
        |_, _, _| {},
    )
    .unwrap();
    assert_eq!(h.slot(a.world(), PropertyId(1)).unwrap().editor, e);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "bad");
    a.world_mut().remove::<Label>(e);
    assert_eq!(h.validate(a.world()), Err(UiError::Stale));
    assert_eq!(h.commit(a.world_mut(), PropertyId(1)), Err(UiError::Stale));
}
#[test]
fn window_blur_cancels_draft_and_access_setvalue_remains_honestly_unsupported() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    batch(&mut a, vec![ch('x')]);
    assert_eq!(
        try_apply_access(
            a.world_mut(),
            AccessCommand::SetValue {
                entity: e,
                value: "unsafe".into()
            }
        ),
        AccessCommandResult::UnsupportedSetValue
    );
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "Ax");
    send(a.world_mut(), WindowFocus::Blurred);
    frame(&mut a);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A");
    assert!(property_events(&a).contains(&PropertyEventKind::Cancelled(CancelReason::Blur)));
}
#[test]
fn bounded_components_reject_oversized_or_duplicate_input_before_mutating() {
    let (mut a, r) = setup();
    let h = table(&mut a, r, TableProps::new("Table"), &columns(), &rows()).unwrap();
    let many: Vec<_> = (1..=MAX_TABLE_ROWS as u64 + 1)
        .map(|id| Item::new(id, "Item"))
        .collect();
    assert_eq!(
        table(&mut a, r, TableProps::new("Table"), &columns(), &many),
        Err(UiError::InvalidProps)
    );
    assert!(h.validate(a.world()).is_ok());
    let mut p = properties();
    p[1].id = p[0].id;
    assert_eq!(inspector(&mut a, r, 1, &p), Err(UiError::InvalidProps));
}
#[test]
fn custom_editor_uses_explicit_typed_draft_without_reflection() {
    let (mut a, r) = setup();
    let mut p = properties();
    p.truncate(1);
    p[0].kind = PropertyKind::Custom("tag".into());
    p[0].value = PropertyValue::Custom("old".into());
    let h = PropertyInspector::mount(
        a.world_mut(),
        r,
        UiId(50),
        InspectorProps::new("Custom", ObjectId(1)),
        &groups(),
        &p,
        |w, slot, _| {
            if children_of(w, slot.editor).is_empty() {
                rndr_ui::text(w, slot.editor, "App editor slot", Typography::Label).unwrap();
            }
        },
    )
    .unwrap();
    h.set_draft(a.world_mut(), PropertyId(1), "new").unwrap();
    assert!(h.commit(a.world_mut(), PropertyId(1)).unwrap());
    assert_eq!(
        h.draft(a.world(), PropertyId(1)).unwrap().value,
        Some(PropertyValue::Custom("new".into()))
    );
}

#[test]
fn reviewer_boolean_window_blur_cancels_draft() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(3)).unwrap().editor;
    request_focus(a.world_mut(), e);
    batch(&mut a, vec![key(NamedKey::Enter)]);
    assert_eq!(
        h.draft(a.world(), PropertyId(3)).unwrap().value,
        Some(PropertyValue::Boolean(true))
    );
    send(a.world_mut(), WindowFocus::Blurred);
    frame(&mut a);
    assert_eq!(
        h.draft(a.world(), PropertyId(3)).unwrap().value,
        Some(PropertyValue::Boolean(false))
    );
}
#[test]
fn reviewer_commit_control_window_blur_cancels_draft() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let slot = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), slot.editor);
    batch(&mut a, vec![ch('x'), key(NamedKey::Tab)]);
    assert_eq!(
        a.world().resource::<Focus>().unwrap().entity,
        Some(slot.commit)
    );
    send(a.world_mut(), WindowFocus::Blurred);
    frame(&mut a);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A");
}

#[test]
fn reviewer_leave_and_return_in_one_batch_must_cancel_before_commit() {
    let (mut a, r) = setup();
    let h = inspector(&mut a, r, 1, &properties()).unwrap();
    frame(&mut a);
    let slot = h.slot(a.world(), PropertyId(1)).unwrap();
    request_focus(a.world_mut(), slot.editor);
    let mut back = key(NamedKey::Tab);
    back.modifiers.shift = true;
    batch(
        &mut a,
        vec![
            ch('x'),
            key(NamedKey::Tab),
            key(NamedKey::Tab),
            key(NamedKey::Tab),
            back.clone(),
            back,
            key(NamedKey::Enter),
        ],
    );
    assert!(
        !property_events(&a)
            .iter()
            .any(|e| matches!(e, PropertyEventKind::CommitRequested { .. })),
        "must cancel when focus leaves property even if it returns before frame ends"
    );
}

#[test]
fn acknowledged_submit_retains_later_draft_caret_history_and_external_replacement_resets() {
    let (mut a, r) = setup();
    let mut p = properties();
    let h = inspector(&mut a, r, 1, &p).unwrap();
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    batch(&mut a, vec![ch('1'), key(NamedKey::Enter), ch('2')]);
    let caret = *a.world().get::<Caret>(e).unwrap();
    p[0].value = PropertyValue::Text("A1".into());
    inspector(&mut a, r, 1, &p).unwrap();
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A12");
    let after = a.world().get::<Caret>(e).unwrap();
    assert_eq!((after.index, after.anchor), (caret.index, caret.anchor));
    let mut m = Modifiers::default();
    m.super_key = Platform::host() == Platform::Mac;
    m.ctrl = !m.super_key;
    batch(&mut a, vec![KeyEvent::character('z', m)]);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A1");
    p[0].value = PropertyValue::Text("B".into());
    inspector(&mut a, r, 1, &p).unwrap();
    batch(&mut a, vec![KeyEvent::character('z', m)]);
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "B");
}

#[test]
fn multiple_outstanding_acknowledgements_preserve_invalid_later_draft_and_contract_change_cancels()
{
    let (mut a, r) = setup();
    let mut p = properties();
    let h = inspector(&mut a, r, 1, &p).unwrap();
    frame(&mut a);
    let e = h.slot(a.world(), PropertyId(1)).unwrap().editor;
    request_focus(a.world_mut(), e);
    batch(
        &mut a,
        vec![ch('1'), key(NamedKey::Enter), ch('2'), key(NamedKey::Enter)],
    );
    h.set_draft(a.world_mut(), PropertyId(1), "invalid too long")
        .unwrap();
    for accepted in ["A1", "A12"] {
        p[0].value = PropertyValue::Text(accepted.into());
        inspector(&mut a, r, 1, &p).unwrap();
        let d = h.draft(a.world(), PropertyId(1)).unwrap();
        assert_eq!(d.raw, "invalid too long");
        assert!(d.error.is_some());
        assert!(!h.commit(a.world_mut(), PropertyId(1)).unwrap());
    }
    p[0].readonly = true;
    inspector(&mut a, r, 1, &p).unwrap();
    assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "A12");
}

#[test]
fn choice_window_blur_and_queued_departure_cancel_without_commit() {
    for blur in [true, false] {
        let (mut a, r) = setup();
        let h = inspector(&mut a, r, 1, &properties()).unwrap();
        frame(&mut a);
        let slot = h.slot(a.world(), PropertyId(4)).unwrap();
        request_focus(a.world_mut(), slot.editor);
        batch(
            &mut a,
            vec![
                key(NamedKey::Enter),
                key(NamedKey::ArrowDown),
                key(NamedKey::Enter),
                key(NamedKey::Tab),
            ],
        );
        assert_eq!(
            h.draft(a.world(), PropertyId(4)).unwrap().value,
            Some(PropertyValue::Choice(2))
        );
        if blur {
            send(a.world_mut(), WindowFocus::Blurred);
            frame(&mut a);
        } else {
            batch(&mut a, vec![key(NamedKey::Tab), key(NamedKey::Tab)]);
        }
        assert_eq!(
            h.draft(a.world(), PropertyId(4)).unwrap().value,
            Some(PropertyValue::Choice(1))
        );
        assert!(!property_events(&a)
            .iter()
            .any(|e| matches!(e, PropertyEventKind::CommitRequested { .. })));
    }
}

#[test]
fn custom_descendant_focus_retains_within_property_then_blur_or_departure_cancels() {
    for blur in [true, false] {
        let (mut a, r) = setup();
        let mut p = properties();
        p[0].kind = PropertyKind::Custom("tag".into());
        p[0].value = PropertyValue::Custom("old".into());
        let h = PropertyInspector::mount(
            a.world_mut(),
            r,
            UiId(50),
            InspectorProps::new("Custom", ObjectId(1)),
            &groups(),
            &p,
            |w, slot, _| {
                if children_of(w, slot.editor).is_empty() && slot.editor != slot.commit {
                    controls::Button::mount(
                        w,
                        slot.editor,
                        UiId(900),
                        controls::ButtonProps::new(
                            "Custom child",
                            controls::HostIntent::Command(900),
                        ),
                    )
                    .unwrap();
                }
            },
        )
        .unwrap();
        frame(&mut a);
        let slot = h.slot(a.world(), PropertyId(1)).unwrap();
        let child = children_of(a.world(), slot.editor)[0];
        request_focus(a.world_mut(), child);
        h.set_draft(a.world_mut(), PropertyId(1), "new").unwrap();
        request_focus(a.world_mut(), slot.commit);
        assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "new");
        request_focus(a.world_mut(), child);
        assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "new");
        if blur {
            send(a.world_mut(), WindowFocus::Blurred);
            frame(&mut a);
        } else {
            let other = h.slot(a.world(), PropertyId(2)).unwrap().editor;
            request_focus(a.world_mut(), other);
            // Cancellation must already have happened, without a paint/update.
        }
        assert_eq!(h.draft(a.world(), PropertyId(1)).unwrap().raw, "old");
        assert!(!property_events(&a)
            .iter()
            .any(|e| matches!(e, PropertyEventKind::CommitRequested { .. })));
    }
}
