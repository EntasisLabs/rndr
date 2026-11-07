//! Multiple and Range selection. Headless gestures are not a native certificate.
use rndr_core::*;
use rndr_ui::{
    collections::{
        CollectionEvent, CollectionEventKind, Item, ItemId, ListView, SelectionMode, ViewProps,
    },
    data::{Column, ColumnId, DataTable, TableProps, MAX_TABLE_COLUMNS, MAX_TABLE_ROWS},
    diff::{DiffMarker, DiffProps, DiffRow, DiffView, HunkId},
    UiId, UiPlugin,
};
use std::collections::BTreeSet;

fn item(id: u64, enabled: bool) -> Item {
    Item {
        id: ItemId(id),
        name: format!("Row {id}"),
        enabled,
    }
}

fn props(mode: SelectionMode) -> ViewProps {
    ViewProps {
        selection: mode,
        ..ViewProps::new("Rows")
    }
}

fn list_parent(app: &mut App) -> Entity {
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 400.));
    app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },))
}

fn set_shift(app: &mut App, shift: bool) {
    app.world_mut().resource_mut::<Input>().unwrap().shift = shift;
}

fn click(app: &mut App, at: [f32; 2], shift: bool) {
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = shift;
        input.pointer.position = at;
        input.pointer.button = PointerButton::Left;
        input.pointer.down = true;
        input.pointer.pressed = true;
        input.pointer.released = false;
    }
    rndr_paint::frame(app);
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.shift = shift;
        input.pointer.down = false;
        input.pointer.pressed = false;
        input.pointer.released = true;
    }
    rndr_paint::frame(app);
}

fn space(app: &mut App, shift: bool) {
    set_shift(app, shift);
    let mut down = KeyEvent::named(
        NamedKey::Space,
        Modifiers {
            shift,
            ..Modifiers::default()
        },
    );
    down.released = false;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![down];
    rndr_paint::frame(app);
    set_shift(app, shift);
    let mut up = KeyEvent::named(
        NamedKey::Space,
        Modifiers {
            shift,
            ..Modifiers::default()
        },
    );
    up.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![up];
    rndr_paint::frame(app);
}

fn arrow(app: &mut App) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::ArrowDown, Modifiers::default())];
    rndr_paint::frame(app);
}

fn row_point(app: &App, root: Entity, id: u64) -> [f32; 2] {
    let slot = rndr_ui::collections::VirtualViewHandle { root }
        .row(app.world(), ItemId(id))
        .unwrap();
    let rect = app.world().get::<Rect>(slot.root).unwrap();
    [rect.x + 8., rect.y + rect.h * 0.5]
}

fn ids(values: &[u64]) -> BTreeSet<ItemId> {
    values.iter().copied().map(ItemId).collect()
}

fn saw_set(app: &App) -> bool {
    events::<CollectionEvent>(app.world())
        .iter()
        .any(|event| event.kind == CollectionEventKind::SelectionSetChanged)
}

#[test]
fn uniform_and_variable_lists_toggle_and_keep_identity_through_recycling() {
    for variable in [false, true] {
        let mut app = App::new();
        let parent = list_parent(&mut app);
        let rows: Vec<_> = (1..=6).map(|id| item(id, id != 2)).collect();
        let handle = if variable {
            let heights = [8., 24., 16., 40., 12., 96.];
            ListView::mount_variable(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &rows,
                &heights,
                |_, _, _| {},
            )
            .unwrap()
        } else {
            ListView::mount_virtual(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &rows,
                32.,
                |_, _, _| {},
            )
            .unwrap()
        };
        app.world_mut()
            .get_mut::<Layout>(handle.root)
            .unwrap()
            .height = Size::Px(120.);
        rndr_paint::frame(&mut app);
        let row = handle.row(app.world(), ItemId(1)).unwrap().root;
        assert_eq!(
            try_activate(app.world_mut(), row),
            ActivationResult::Accepted
        );
        assert_eq!(handle.selection(app.world()), ids(&[1]));
        assert_eq!(handle.selected(app.world()), None);
        rndr_paint::frame(&mut app);
        assert!(saw_set(&app));
        assert!(request_focus(app.world_mut(), row));
        space(&mut app, false);
        assert!(handle.selection(app.world()).is_empty());
        let first = row_point(&app, handle.root, 1);
        let third = row_point(&app, handle.root, 3);
        click(&mut app, first, false);
        click(&mut app, third, false);
        assert_eq!(handle.selection(app.world()), ids(&[1, 3]));
        let before = handle.selection(app.world());
        arrow(&mut app);
        assert_eq!(handle.selection(app.world()), before);
        let disabled = row_point(&app, handle.root, 2);
        click(&mut app, disabled, false);
        assert!(!handle.selection(app.world()).contains(&ItemId(2)));
        handle.reveal(app.world_mut(), ItemId(6)).unwrap();
        rndr_paint::frame(&mut app);
        assert_eq!(handle.selection(app.world()), ids(&[1, 3]));
        assert!(handle.row(app.world(), ItemId(1)).is_none());
        let mut reordered = rows.clone();
        reordered.reverse();
        if variable {
            let heights = [96., 12., 40., 16., 24., 8.];
            ListView::mount_variable(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &reordered,
                &heights,
                |_, _, _| {},
            )
            .unwrap();
        } else {
            ListView::mount_virtual(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &reordered,
                32.,
                |_, _, _| {},
            )
            .unwrap();
        }
        assert_eq!(handle.selection(app.world()), ids(&[1, 3]));
        reordered.retain(|row| row.id != ItemId(1));
        if variable {
            let heights = vec![32.; reordered.len()];
            ListView::mount_variable(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &reordered,
                &heights,
                |_, _, _| {},
            )
            .unwrap();
        } else {
            ListView::mount_virtual(
                app.world_mut(),
                parent,
                UiId(1),
                props(SelectionMode::Multiple),
                &reordered,
                32.,
                |_, _, _| {},
            )
            .unwrap();
        }
        assert!(!handle.selection(app.world()).contains(&ItemId(1)));
        assert!(handle.selection(app.world()).contains(&ItemId(3)));
        rndr_paint::frame(&mut app);
        assert!(saw_set(&app));
    }
}

#[test]
fn range_extends_without_moving_the_anchor_and_repairs_when_it_disappears() {
    let mut app = App::new();
    let parent = list_parent(&mut app);
    let mut rows: Vec<_> = (1..=5).map(|id| item(id, id != 3)).collect();
    let handle = ListView::mount(
        app.world_mut(),
        parent,
        UiId(3),
        props(SelectionMode::Range),
        &rows,
        |_, _, _| {},
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(240.);
    rndr_paint::frame(&mut app);
    let point = |app: &App, id: u64| {
        let rect = app
            .world()
            .get::<Rect>(handle.row(app.world(), ItemId(id)).unwrap().root)
            .unwrap();
        [rect.x + 8., rect.y + 4.]
    };
    let origin = point(&app, 1);
    click(&mut app, origin, false);
    assert_eq!(handle.selection(app.world()), ids(&[1]));
    assert_eq!(handle.range_anchor(app.world()), Some(ItemId(1)));
    let end = point(&app, 5);
    click(&mut app, end, true);
    assert_eq!(handle.selection(app.world()), ids(&[1, 2, 4, 5]));
    assert_eq!(handle.range_anchor(app.world()), Some(ItemId(1)));
    assert!(!handle.selection(app.world()).contains(&ItemId(3)));
    let before = handle.selection(app.world());
    handle.focus(app.world_mut(), ItemId(4)).unwrap();
    arrow(&mut app);
    assert_eq!(handle.selection(app.world()), before);
    assert_eq!(handle.range_anchor(app.world()), Some(ItemId(1)));
    rows.remove(0);
    ListView::mount(
        app.world_mut(),
        parent,
        UiId(3),
        props(SelectionMode::Range),
        &rows,
        |_, _, _| {},
    )
    .unwrap();
    assert!(!handle.selection(app.world()).contains(&ItemId(1)));
    assert_eq!(handle.range_anchor(app.world()), Some(ItemId(2)));
    rows.retain(|row| row.id != ItemId(2) && row.id != ItemId(4) && row.id != ItemId(5));
    ListView::mount(
        app.world_mut(),
        parent,
        UiId(3),
        props(SelectionMode::Range),
        &rows,
        |_, _, _| {},
    )
    .unwrap();
    assert_eq!(handle.range_anchor(app.world()), None);
    let disabled = point(&app, 3);
    click(&mut app, disabled, true);
    assert!(handle.selection(app.world()).is_empty());
    assert_eq!(handle.range_anchor(app.world()), None);
}

#[test]
fn single_and_none_do_not_toggle_or_emit_a_set_change() {
    let mut app = App::new();
    let parent = list_parent(&mut app);
    let rows = vec![item(1, true), item(2, true)];
    let handle = ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(4),
        props(SelectionMode::Single),
        &rows,
        28.,
        |_, _, _| {},
    )
    .unwrap();
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(80.);
    rndr_paint::frame(&mut app);
    handle.select(app.world_mut(), Some(ItemId(1))).unwrap();
    rndr_paint::frame(&mut app);
    assert!(events::<CollectionEvent>(app.world())
        .iter()
        .any(|event| event.kind == CollectionEventKind::Selected(Some(ItemId(1)))));
    assert!(!saw_set(&app));
    let second = row_point(&app, handle.root, 2);
    click(&mut app, second, false);
    assert_eq!(handle.selected(app.world()), Some(ItemId(2)));
    assert!(handle.selection(app.world()).is_empty());
    click(&mut app, second, false);
    assert_eq!(handle.selected(app.world()), Some(ItemId(2)));
    ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(4),
        props(SelectionMode::None),
        &rows,
        28.,
        |_, _, _| {},
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    let first = row_point(&app, handle.root, 1);
    click(&mut app, first, false);
    assert_eq!(handle.selected(app.world()), None);
    assert!(handle.selection(app.world()).is_empty());
    assert!(handle.select(app.world_mut(), Some(ItemId(1))).is_err());
}

#[test]
fn table_uses_the_same_modes_inside_its_budget_and_diff_stays_unselected() {
    assert_eq!(MAX_TABLE_ROWS, 256);
    assert_eq!(MAX_TABLE_COLUMNS, 32);
    let mut app = App::new();
    let parent = list_parent(&mut app);
    let mut table_props = TableProps::new("Records");
    table_props.selection = SelectionMode::Multiple;
    let columns = [Column {
        id: ColumnId(1),
        label: "Name".into(),
        width: 120.,
        alignment: rndr_ui::data::CellAlignment::Start,
        sortable: false,
    }];
    let rows = vec![item(1, true), item(2, true), item(3, false)];
    let table = DataTable::mount(
        app.world_mut(),
        parent,
        UiId(8),
        table_props,
        &columns,
        &rows,
        |_, _, _, _| {},
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    let row = table.rows.row(app.world(), ItemId(1)).unwrap().root;
    assert_eq!(
        try_activate(app.world_mut(), row),
        ActivationResult::Accepted
    );
    let second = table.rows.row(app.world(), ItemId(2)).unwrap().root;
    let third = table.rows.row(app.world(), ItemId(3)).unwrap().root;
    try_activate(app.world_mut(), second);
    try_activate(app.world_mut(), third);
    assert_eq!(table.rows.selection(app.world()), ids(&[1, 2]));
    assert_eq!(table.rows.selected(app.world()), None);

    let diff_rows = vec![DiffRow {
        id: ItemId(1),
        hunk: HunkId(1),
        text: "context".into(),
        marker: DiffMarker::Unchanged,
        height: 20.,
    }];
    let diff = DiffView::mount(
        app.world_mut(),
        parent,
        UiId(9),
        DiffProps::new("Changes"),
        &diff_rows,
    )
    .unwrap();
    app.world_mut().get_mut::<Layout>(diff.root).unwrap().height = Size::Px(40.);
    rndr_paint::frame(&mut app);
    let diff_row = rndr_ui::collections::VirtualViewHandle { root: diff.root }
        .row(app.world(), ItemId(1))
        .unwrap()
        .root;
    let _ = try_activate(app.world_mut(), diff_row);
    let list = rndr_ui::collections::VirtualViewHandle { root: diff.root };
    assert_eq!(list.selected(app.world()), None);
    assert!(list.selection(app.world()).is_empty());
    diff.next_hunk(app.world_mut()).unwrap();
    assert_eq!(diff.focused_hunk(app.world()), Some(HunkId(1)));
}
