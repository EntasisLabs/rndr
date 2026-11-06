//! Variable-height ListView. Headless anchor math only; not a native certificate.
use rndr_core::*;
use rndr_ui::{
    collections::{
        Item as RowItem, ItemId, ListView, RowSlot, VariableAnchor, ViewProps, VirtualViewHandle,
    },
    UiId, UiPlugin,
};
use std::collections::HashMap;

#[derive(Default)]
struct Drafts(HashMap<ItemId, &'static str>);

fn bare(_: &mut World, _: RowSlot, _: &RowItem) {}

fn marked(w: &mut World, slot: RowSlot, item: &RowItem) {
    if item.id == ItemId(0) {
        w.insert(slot.content, Color::rgb(0.2, 0.3, 0.4));
    }
    if let Some(text) = w
        .resource::<Drafts>()
        .and_then(|drafts| drafts.0.get(&item.id).copied())
    {
        w.insert(slot.content, Label::new(text));
    }
}

fn nested(w: &mut World, slot: RowSlot, _item: &RowItem) {
    use rndr_ui::selection::{Checkbox, SelectionProps};
    Checkbox::mount(
        w,
        slot.content,
        UiId(2),
        SelectionProps::new("Enabled"),
        false,
    )
    .unwrap();
}

fn rows(n: u64, height: impl Fn(u64) -> f32) -> (Vec<RowItem>, Vec<f32>) {
    (
        (0..n).map(|id| RowItem::new(id, "row")).collect(),
        (0..n).map(height).collect(),
    )
}

fn mixed(n: u64) -> (Vec<RowItem>, Vec<f32>) {
    rows(n, |id| match id % 3 {
        0 => 8.,
        1 => 24.,
        _ => 96.,
    })
}

fn mount(
    app: &mut App,
    parent: Entity,
    items: &[RowItem],
    heights: &[f32],
    compose: fn(&mut World, RowSlot, &RowItem),
) -> VirtualViewHandle {
    ListView::mount_variable(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Records"),
        items,
        heights,
        compose,
    )
    .unwrap()
}

fn setup(
    n: u64,
    list_height: f32,
    compose: fn(&mut World, RowSlot, &RowItem),
) -> (App, Entity, VirtualViewHandle, Vec<RowItem>, Vec<f32>) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 400.));
    app.world_mut().insert_resource(Drafts::default());
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let (items, heights) = if n == 10_000 {
        mixed(n)
    } else {
        rows(n, |_| 32.)
    };
    let handle = mount(&mut app, parent, &items, &heights, compose);
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(list_height);
    rndr_paint::frame(&mut app);
    (app, parent, handle, items, heights)
}

fn anchor(app: &App, handle: VirtualViewHandle) -> VariableAnchor {
    handle.anchor(app.world()).expect("variable anchor")
}

#[test]
fn ten_thousand_mixed_heights_keep_identity_and_record_paint_error() {
    let (mut app, _, handle, _, _) = setup(10_000, 320., bare);
    assert_eq!(handle.len(app.world()), 10_000);
    let layouts = app.world().entities_with::<Layout>().len();
    assert!(
        layouts < 160,
        "realized layouts {layouts} are not viewport-bounded"
    );
    handle.jump(app.world_mut(), ItemId(0), 0).unwrap();
    assert_eq!(
        app.world()
            .get::<Rect>(handle.row(app.world(), ItemId(0)).unwrap().root)
            .unwrap()
            .h,
        8.
    );
    handle.jump(app.world_mut(), ItemId(2), 0).unwrap();
    assert_eq!(
        app.world()
            .get::<Rect>(handle.row(app.world(), ItemId(2)).unwrap().root)
            .unwrap()
            .h,
        96.
    );
    let away = handle.jump(app.world_mut(), ItemId(9_000), 333);
    assert!(away.is_ok());
    rndr_paint::frame(&mut app);
    let saved = anchor(&app, handle);
    assert_eq!(saved.id, ItemId(9_000));
    assert_eq!(saved.remainder_mp, 333);
    let error = handle
        .paint_conversion_error_mp(app.world())
        .expect("paint error");
    eprintln!("10000-row f32 paint conversion error: {error} milli-points");
    assert_eq!(
        error, 10.75,
        "recorded paint conversion error for row 9000 remainder 333"
    );
    handle.jump(app.world_mut(), ItemId(100), 0).unwrap();
    rndr_paint::frame(&mut app);
    handle
        .jump(app.world_mut(), saved.id, saved.remainder_mp)
        .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(anchor(&app, handle), saved);
    assert_eq!(
        handle
            .paint_conversion_error_mp(app.world())
            .unwrap()
            .to_bits(),
        error.to_bits()
    );
    let identities = app.world().entities_with::<Layout>();
    for _ in 0..5 {
        rndr_paint::frame(&mut app);
    }
    assert_eq!(identities, app.world().entities_with::<Layout>());
    assert_eq!(anchor(&app, handle), saved);
    assert!(handle.row(app.world(), ItemId(1)).is_none());
}

#[test]
fn anchor_survives_insert_resize_reorder_and_repairs_removal() {
    let (mut app, parent, handle, mut items, mut heights) = setup(80, 320., bare);
    handle.jump(app.world_mut(), ItemId(40), 1_500).unwrap();
    let before = anchor(&app, handle);
    let row = *app
        .world()
        .get::<Rect>(handle.row(app.world(), ItemId(40)).unwrap().root)
        .unwrap();
    let list_y = app.world().get::<Rect>(handle.root).unwrap().y;
    items.insert(0, RowItem::new(1_000, "row"));
    heights.insert(0, 40.);
    mount(&mut app, parent, &items, &heights, bare);
    rndr_paint::frame(&mut app);
    assert_eq!(anchor(&app, handle), before);
    let after = *app
        .world()
        .get::<Rect>(handle.row(app.world(), ItemId(40)).unwrap().root)
        .unwrap();
    assert!(
        (after.y - row.y).abs() < 1.0,
        "row moved from {} to {}",
        row.y,
        after.y
    );
    assert!(
        (after.y - (list_y - 1.5)).abs() < 1.0,
        "anchored row y {}",
        after.y
    );

    handle
        .measure_height(app.world_mut(), ItemId(1_000), 80.)
        .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(
        anchor(&app, handle),
        before,
        "resize above moved the anchor"
    );

    let mut reversed_items = items.clone();
    let mut reversed_heights = heights.clone();
    reversed_items.reverse();
    reversed_heights.reverse();
    assert_ne!(reversed_items[40].id, ItemId(40));
    mount(&mut app, parent, &reversed_items, &reversed_heights, bare);
    rndr_paint::frame(&mut app);
    assert_eq!(anchor(&app, handle).id, ItemId(40));
    assert_eq!(anchor(&app, handle).remainder_mp, 1_500);

    let mut removed = reversed_items.clone();
    let mut removed_heights = reversed_heights.clone();
    let position = removed
        .iter()
        .position(|item| item.id == ItemId(40))
        .unwrap();
    let next = removed[position + 1].id;
    removed.remove(position);
    removed_heights.remove(position);
    mount(&mut app, parent, &removed, &removed_heights, bare);
    assert_eq!(
        anchor(&app, handle),
        VariableAnchor {
            id: next,
            remainder_mp: 0
        }
    );

    let tail = removed.len() - 1;
    let previous = removed[tail - 1].id;
    let tail_id = removed[tail].id;
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(32.);
    handle.jump(app.world_mut(), tail_id, 0).unwrap();
    assert_eq!(anchor(&app, handle).id, tail_id);
    removed.pop();
    removed_heights.pop();
    mount(&mut app, parent, &removed, &removed_heights, bare);
    assert_eq!(
        anchor(&app, handle),
        VariableAnchor {
            id: previous,
            remainder_mp: 0
        }
    );

    mount(&mut app, parent, &[], &[], bare);
    assert_eq!(handle.anchor(app.world()), None);
    assert!(handle.is_empty(app.world()));
}

#[test]
fn shrink_clamps_remainder_and_invalid_heights_do_not_mutate() {
    let (mut app, parent, handle, items, heights) = setup(12, 64., bare);
    handle.jump(app.world_mut(), ItemId(4), 20_000).unwrap();
    handle
        .measure_height(app.world_mut(), ItemId(4), 10.)
        .unwrap();
    assert_eq!(
        anchor(&app, handle),
        VariableAnchor {
            id: ItemId(4),
            remainder_mp: 10_000
        }
    );
    let saved = anchor(&app, handle);
    let layouts = app.world().entities_with::<Layout>().len();
    for bad in [0., -4., f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0004] {
        let mut bad_heights = heights.clone();
        bad_heights[1] = bad;
        assert!(
            ListView::mount_variable(
                app.world_mut(),
                parent,
                UiId(1),
                ViewProps::new("Records"),
                &items,
                &bad_heights,
                bare,
            )
            .is_err(),
            "{bad}"
        );
        assert!(handle
            .measure_height(app.world_mut(), ItemId(4), bad)
            .is_err());
    }
    assert!(ListView::mount_variable(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Records"),
        &items,
        &heights[..heights.len() - 1],
        bare,
    )
    .is_err());
    let duplicate = [RowItem::new(1, "row"), RowItem::new(1, "row")];
    assert!(ListView::mount_variable(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Records"),
        &duplicate,
        &[12., 12.],
        bare,
    )
    .is_err());
    assert_eq!(anchor(&app, handle), saved);
    assert_eq!(handle.len(app.world()), 12);
    assert_eq!(layouts, app.world().entities_with::<Layout>().len());
    assert!(handle.jump(app.world_mut(), ItemId(4), 10_001).is_err());
    assert!(handle.jump(app.world_mut(), ItemId(4), -1).is_err());
    assert_eq!(anchor(&app, handle), saved);
}

#[test]
fn recycling_keeps_item_state_and_parks_focused_nested_control() {
    let (mut app, parent, handle, _, _) = setup(60, 30., marked);
    app.world_mut()
        .resource_mut::<Drafts>()
        .unwrap()
        .0
        .insert(ItemId(0), "draft-zero");
    let (items, heights) = rows(60, |_| 48.);
    mount(&mut app, parent, &items, &heights, marked);
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(30.);
    rndr_paint::frame(&mut app);
    let slot = handle.row(app.world(), ItemId(0)).unwrap();
    let content = slot.content;
    let root = slot.root;
    assert!(app.world().has::<Color>(content));
    assert_eq!(
        app.world().get::<Label>(content).unwrap().text,
        "draft-zero"
    );
    handle.jump(app.world_mut(), ItemId(1), 0).unwrap();
    rndr_paint::frame(&mut app);
    assert!(!app.world().is_alive(content));
    let rebound = handle.row(app.world(), ItemId(1)).unwrap();
    assert_eq!(rebound.root, root);
    assert!(!app.world().has::<Color>(rebound.content));
    assert!(!app.world().has::<Label>(rebound.content));
    assert_eq!(
        app.world().resource::<Drafts>().unwrap().0.get(&ItemId(0)),
        Some(&"draft-zero")
    );
    assert_eq!(handle.item_for_entity(app.world(), root), Some(ItemId(1)));

    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(200.);
    mount(&mut app, parent, &items, &heights, nested);
    handle.jump(app.world_mut(), ItemId(0), 0).unwrap();
    rndr_paint::frame(&mut app);
    let control = children_of(
        app.world(),
        handle.row(app.world(), ItemId(0)).unwrap().content,
    )[0];
    assert!(request_focus(app.world_mut(), control));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(control)
    );
    handle.jump(app.world_mut(), ItemId(40), 0).unwrap();
    rndr_paint::frame(&mut app);
    assert!(!app.world().is_alive(control));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(handle.root)
    );
    assert_eq!(handle.focused(app.world()), Some(ItemId(0)));
    handle.jump(app.world_mut(), ItemId(0), 0).unwrap();
    rndr_paint::frame(&mut app);
    let restored = children_of(
        app.world(),
        handle.row(app.world(), ItemId(0)).unwrap().content,
    )[0];
    assert_ne!(restored, control);
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(handle.root)
    );
    assert_ne!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(restored)
    );
    assert_eq!(handle.focused(app.world()), Some(ItemId(0)));
}

#[test]
fn uniform_mount_is_unchanged_and_conflicts_with_variable() {
    let (mut app, parent, handle, items, heights) = setup(20, 320., bare);
    assert!(handle.anchor(app.world()).is_some());
    assert!(ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Records"),
        &items,
        32.,
        bare,
    )
    .is_err());
    assert_eq!(handle.len(app.world()), 20);
    let uniform = ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(2),
        ViewProps::new("Uniform"),
        &items,
        32.,
        bare,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(uniform.anchor(app.world()), None);
    assert!(uniform.jump(app.world_mut(), ItemId(1), 0).is_err());
    assert!(uniform
        .measure_height(app.world_mut(), ItemId(1), 10.)
        .is_err());
    assert_eq!(uniform.len(app.world()), 20);
    assert!(ListView::mount_variable(
        app.world_mut(),
        parent,
        UiId(2),
        ViewProps::new("Uniform"),
        &items,
        &heights,
        bare,
    )
    .is_err());
    uniform.reveal(app.world_mut(), ItemId(15)).unwrap();
    assert!(uniform.row(app.world(), ItemId(15)).is_some());
}

#[test]
fn page_down_uses_supplied_heights() {
    let (mut app, parent, handle, _, _) = setup(80, 320., bare);
    let (items, heights) = rows(80, |id| if id == 0 { 8. } else { 100. });
    mount(&mut app, parent, &items, &heights, bare);
    app.world_mut()
        .get_mut::<Layout>(handle.root)
        .unwrap()
        .height = Size::Px(320.);
    rndr_paint::frame(&mut app);
    handle.focus(app.world_mut(), ItemId(0)).unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::PageDown, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert_eq!(handle.focused(app.world()), Some(ItemId(4)));
}

#[test]
fn wheel_rebinds_the_anchor_and_a_later_height_edit_keeps_it() {
    let (mut app, _, handle, _, _) = setup(40, 120., bare);
    assert_eq!(
        anchor(&app, handle),
        VariableAnchor {
            id: ItemId(0),
            remainder_mp: 0
        }
    );
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [40., 20.];
        input.scroll = [0., -80.];
    }
    rndr_paint::frame(&mut app);
    let moved = anchor(&app, handle);
    assert_ne!(
        moved,
        VariableAnchor {
            id: ItemId(0),
            remainder_mp: 0
        }
    );
    handle
        .measure_height(app.world_mut(), ItemId(0), 48.)
        .unwrap();
    rndr_paint::frame(&mut app);
    assert_eq!(anchor(&app, handle), moved);
}
