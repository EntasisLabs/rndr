//! Observable reconciliation ordering, including state before publication.
use rndr_core::*;
use rndr_ui::{collections::Item, controls, data::*, *};

fn setup() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    let parent = app.world_mut().spawn((Layout::column(),));
    (app, parent)
}
fn columns() -> Vec<Column> {
    [2, 1]
        .into_iter()
        .map(|id| Column {
            id: ColumnId(id),
            label: format!("Column {id}"),
            width: 100.,
            alignment: CellAlignment::Start,
            sortable: true,
        })
        .collect()
}
fn groups() -> Vec<PropertyGroup> {
    vec![PropertyGroup {
        id: GroupId(1),
        label: "Group".into(),
    }]
}
fn fields() -> Vec<Property> {
    [2, 1]
        .into_iter()
        .map(|id| Property {
            id: PropertyId(id),
            group: GroupId(1),
            label: format!("Field {id}"),
            description: String::new(),
            kind: PropertyKind::Custom("test".into()),
            value: PropertyValue::Custom("base".into()),
            enabled: true,
            readonly: false,
            validation: None,
            validate: None,
        })
        .collect()
}

#[test]
fn table_callbacks_follow_supplied_order_and_see_retained_previous_publication() {
    let (mut app, parent) = setup();
    let columns = columns();
    let rows = vec![Item::new(2, "Second"), Item::new(1, "First")];
    let mut calls = Vec::new();
    let handle = DataTable::mount(
        app.world_mut(),
        parent,
        UiId(1),
        TableProps::new("Old"),
        &columns,
        &rows,
        |_, slot, row, column| calls.push((row.id, column.id, slot.root)),
    )
    .unwrap();
    assert_eq!(
        calls
            .iter()
            .map(|(row, column, _)| (row.0, column.0))
            .collect::<Vec<_>>(),
        vec![(2, 2), (2, 1), (1, 2), (1, 1)]
    );
    let retained = calls.clone();
    calls.clear();
    let mut reordered = rows.clone();
    reordered.reverse();
    let new_handle = DataTable::mount(
        app.world_mut(),
        parent,
        UiId(1),
        TableProps::new("New"),
        &columns,
        &reordered,
        |world, slot, row, column| {
            // Managed state is published only after all callbacks. On remount,
            // the previous handle still resolves the same retained content.
            assert_eq!(
                handle.cell(world, row.id, column.id).unwrap().root,
                slot.root
            );
            assert!(world.has::<Layout>(slot.content));
            calls.push((row.id, column.id, slot.root));
        },
    )
    .unwrap();
    assert_eq!(new_handle, handle);
    assert_eq!(
        calls
            .iter()
            .map(|(row, column, _)| (row.0, column.0))
            .collect::<Vec<_>>(),
        vec![(1, 2), (1, 1), (2, 2), (2, 1)]
    );
    for call in calls {
        assert!(retained.contains(&call));
    }
}

#[test]
fn refusal_order_is_props_then_parent_then_identity_then_retained_validation() {
    let (mut app, parent) = setup();
    let columns = columns();
    let stale = app.world_mut().spawn((Label::new("Not a layout parent"),));
    let invalid = TableProps::new("");
    assert_eq!(
        DataTable::mount(
            app.world_mut(),
            stale,
            UiId(1),
            invalid,
            &columns,
            &[],
            |_, _, _, _| panic!("refused callback")
        ),
        Err(UiError::InvalidProps)
    );
    assert_eq!(
        DataTable::mount(
            app.world_mut(),
            stale,
            UiId(1),
            TableProps::new("Table"),
            &columns,
            &[],
            |_, _, _, _| panic!("refused callback")
        ),
        Err(UiError::Stale)
    );
    controls::Button::mount(
        app.world_mut(),
        parent,
        UiId(1),
        controls::ButtonProps::new("Conflict", controls::HostIntent::Command(1)),
    )
    .unwrap();
    let before = children_of(app.world(), parent);
    assert_eq!(
        DataTable::mount(
            app.world_mut(),
            parent,
            UiId(1),
            TableProps::new("Table"),
            &columns,
            &[],
            |_, _, _, _| panic!("refused callback")
        ),
        Err(UiError::IdentityConflict)
    );
    assert_eq!(children_of(app.world(), parent), before);
    let groups = groups();
    let fields = fields();
    let invalid = InspectorProps::new("", ObjectId(1));
    assert_eq!(
        PropertyInspector::mount(
            app.world_mut(),
            stale,
            UiId(2),
            invalid,
            &groups,
            &fields,
            |_, _, _| panic!("refused callback")
        ),
        Err(UiError::InvalidProps)
    );
    assert_eq!(
        PropertyInspector::mount(
            app.world_mut(),
            stale,
            UiId(2),
            InspectorProps::new("Inspector", ObjectId(1)),
            &groups,
            &fields,
            |_, _, _| panic!("refused callback")
        ),
        Err(UiError::Stale)
    );
    assert_eq!(
        PropertyInspector::mount(
            app.world_mut(),
            parent,
            UiId(1),
            InspectorProps::new("Inspector", ObjectId(1)),
            &groups,
            &fields,
            |_, _, _| panic!("refused callback")
        ),
        Err(UiError::IdentityConflict)
    );
    let handle = PropertyInspector::mount(
        app.world_mut(),
        parent,
        UiId(2),
        InspectorProps::new("Inspector", ObjectId(1)),
        &groups,
        &fields,
        |_, _, _| {},
    )
    .unwrap();
    let slot = handle.slot(app.world(), PropertyId(1)).unwrap();
    app.world_mut().remove::<Layout>(slot.root);
    let before = children_of(app.world(), handle.root);
    assert_eq!(
        PropertyInspector::mount(
            app.world_mut(),
            parent,
            UiId(2),
            InspectorProps::new("Inspector", ObjectId(1)),
            &groups,
            &fields,
            |_, _, _| panic!("refused callback")
        ),
        Err(UiError::Stale)
    );
    assert_eq!(children_of(app.world(), handle.root), before);
}

#[test]
fn inspector_custom_callbacks_use_identity_order_after_cancellation_before_publication() {
    let (mut app, parent) = setup();
    let groups = groups();
    let fields = fields();
    let mut calls = Vec::new();
    let handle = PropertyInspector::mount(
        app.world_mut(),
        parent,
        UiId(1),
        InspectorProps::new("Inspector", ObjectId(1)),
        &groups,
        &fields,
        |_, _, property| calls.push(property.id),
    )
    .unwrap();
    // Custom projection follows the BTree identity order, not supplied order.
    assert_eq!(calls, vec![PropertyId(1), PropertyId(2)]);
    handle
        .set_draft(app.world_mut(), PropertyId(1), "draft")
        .unwrap();
    let old_slot = handle.slot(app.world(), PropertyId(1)).unwrap();
    calls.clear();
    PropertyInspector::mount(
        app.world_mut(),
        parent,
        UiId(1),
        InspectorProps::new("Inspector", ObjectId(2)),
        &groups,
        &fields,
        |world, slot, property| {
            assert!(!world.is_alive(old_slot.root));
            assert!(world.has::<Layout>(slot.editor));
            calls.push(property.id);
        },
    )
    .unwrap();
    assert_eq!(calls, vec![PropertyId(1), PropertyId(2)]);
    // Requests are queued during mount; canonical event publication is frame-bound.
    rndr_paint::frame(&mut app);
    assert!(events::<PropertyEvent>(app.world())
        .iter()
        .any(|event| event.object == ObjectId(1)
            && event.property == PropertyId(1)
            && event.kind == PropertyEventKind::Cancelled(CancelReason::Reconciled)));
    assert_eq!(
        handle.draft(app.world(), PropertyId(1)).unwrap().raw,
        "base"
    );
}
