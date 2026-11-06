//! Virtual renderers observe canonical publication, in presentation order.
use rndr_core::*;
use rndr_ui::{
    collections::{Item, ItemId, ListView, RowSlot, ViewProps, VirtualViewHandle},
    UiId, UiPlugin,
};

struct Expected {
    count: usize,
    selected: Option<ItemId>,
    calls: Vec<ItemId>,
}
fn compose(world: &mut World, slot: RowSlot, item: &Item) {
    let root = world.get::<Parent>(slot.root).unwrap().entity;
    let view = VirtualViewHandle { root };
    let expected = world.resource::<Expected>().unwrap();
    assert_eq!(view.len(world), expected.count);
    assert_eq!(view.selected(world), expected.selected);
    let published = view.row(world, item.id).unwrap();
    assert_eq!(published.root, slot.root);
    assert_eq!(published.content, slot.content);
    assert!(children_of(world, root).contains(&slot.root));
    assert_eq!(
        world.get::<Layout>(slot.root).unwrap().height,
        Size::Px(32.)
    );
    world
        .resource_mut::<Expected>()
        .unwrap()
        .calls
        .push(item.id);
}
#[test]
fn callbacks_observe_published_metadata_and_rows_before_style_resolution() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(400., 160.));
    app.world_mut().insert_resource(Expected {
        count: 100,
        selected: None,
        calls: vec![],
    });
    let parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let items: Vec<_> = (0..100)
        .map(|id| Item::new(id, format!("Row {id}")))
        .collect();
    let view = ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Rows"),
        &items,
        32.,
        compose,
    )
    .unwrap();
    rndr_paint::frame(&mut app);
    view.select(app.world_mut(), Some(ItemId(3))).unwrap();
    app.world_mut().resource_mut::<Expected>().unwrap().selected = Some(ItemId(3));
    let mut reversed = items;
    reversed.reverse();
    app.world_mut()
        .resource_mut::<Expected>()
        .unwrap()
        .calls
        .clear();
    ListView::mount_virtual(
        app.world_mut(),
        parent,
        UiId(1),
        ViewProps::new("Reordered"),
        &reversed,
        32.,
        compose,
    )
    .unwrap();
    let calls = &app.world().resource::<Expected>().unwrap().calls;
    assert!(!calls.is_empty());
    assert!(
        calls.windows(2).all(|pair| pair[0] > pair[1]),
        "callbacks must follow presentation, not BTree identity order"
    );
    assert_eq!(view.selected(app.world()), Some(ItemId(3)));
}
