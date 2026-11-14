//! Reorder a list and a tree by dragging a row, or from the keyboard.
//!
//! Drag a row onto another. The top half inserts before, the bottom half
//! after, and the middle of a tree row inserts inside. Escape or a release
//! on a rejected row cancels. Alt+Up and Alt+Down move the focused row.
//! On the tree, Alt+Right moves it inside the previous row and Alt+Left
//! places it after its parent.
//!
//! `cargo run -p rndr --example item_drag`
use rndr_core::*;
use rndr_ui::collections::{
    reorder_list, reorder_tree, CollectionEvent, CollectionEventKind, Item, ItemId, ListView,
    RowSlot, TreeNode, TreeView, ViewProps,
};
use rndr_ui::{text, Typography, UiId, UiPlugin, UiTheme};

const W: f32 = 720.0;
const H: f32 = 560.0;

struct Model {
    items: Vec<Item>,
    nodes: Vec<TreeNode>,
    list: rndr_ui::collections::ViewHandle,
    tree: rndr_ui::collections::ViewHandle,
    list_parent: Entity,
    tree_parent: Entity,
    status: Entity,
}

fn row(world: &mut World, slot: RowSlot, item: &Item) {
    let children = children_of(world, slot.content);
    if children.is_empty() {
        text(world, slot.content, &item.name, Typography::Label).unwrap();
    } else if let Some(label) = world.get_mut::<Label>(children[0]) {
        label.text = item.name.clone();
    }
}

fn sample_items() -> Vec<Item> {
    ["Inbox", "Today", "Reading", "Later"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| Item::new(index as u64 + 1, name))
        .collect()
}

fn sample_tree() -> Vec<TreeNode> {
    vec![
        TreeNode {
            item: Item::new(1, "Work"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "Notes"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(3, "Archive"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(4, "Personal"),
            parent: None,
        },
    ]
}

fn build(app: &mut App) {
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(UiTheme::default());
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            padding: 20.0,
            gap: 12.0,
            ..Layout::column()
        },
        Color::rgb(0.96, 0.96, 0.97),
    ));
    let heading = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new("Drag a row. Alt+Up and Alt+Down move the focused one."),
    ));
    set_parent(app.world_mut(), heading, root);
    let columns = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        gap: 16.0,
        ..Layout::row()
    },));
    set_parent(app.world_mut(), columns, root);
    let list_parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let tree_parent = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    set_parent(app.world_mut(), list_parent, columns);
    set_parent(app.world_mut(), tree_parent, columns);
    let items = sample_items();
    let nodes = sample_tree();
    let list = ListView::mount(
        app.world_mut(),
        list_parent,
        UiId(1),
        ViewProps::new("Tasks").with_reorder(),
        &items,
        row,
    )
    .unwrap();
    let tree = TreeView::mount(
        app.world_mut(),
        tree_parent,
        UiId(2),
        ViewProps::new("Projects").with_reorder(),
        &nodes,
        row,
    )
    .unwrap();
    tree.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    let status = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            ..Layout::column()
        },
        Label::new("Nothing moved yet."),
    ));
    set_parent(app.world_mut(), status, root);
    app.world_mut().insert_resource(Model {
        items,
        nodes,
        list,
        tree,
        list_parent,
        tree_parent,
        status,
    });
    app.add_system(Schedule::Update, apply_moves);
}

fn apply_moves(world: &mut World) {
    let events = events::<CollectionEvent>(world).to_vec();
    let Some(model) = world.resource::<Model>() else {
        return;
    };
    let (list_root, tree_root, status) = (model.list.root, model.tree.root, model.status);
    let mut items = model.items.clone();
    let mut nodes = model.nodes.clone();
    let mut changed = false;
    let mut message = None;
    for event in events {
        let CollectionEventKind::Reordered(movement) = event.kind else {
            continue;
        };
        if event.view == list_root {
            if let Some(next) = reorder_list(&items, movement.id, movement.target, movement.place) {
                items = next;
                changed = true;
            }
        } else if event.view == tree_root {
            if let Some(next) = reorder_tree(&nodes, movement.id, movement.target, movement.place) {
                nodes = next;
                changed = true;
            }
        }
        message = Some(rndr_ui::drag::announcement(world));
    }
    if message.is_none() {
        let text = rndr_ui::drag::announcement(world);
        if text.contains("Cancelled") || text.contains("Cannot") {
            message = Some(text);
        }
    }
    if !changed && message.is_none() {
        return;
    }
    if changed {
        let list_parent = world.resource::<Model>().unwrap().list_parent;
        let tree_parent = world.resource::<Model>().unwrap().tree_parent;
        let list = ListView::mount(
            world,
            list_parent,
            UiId(1),
            ViewProps::new("Tasks").with_reorder(),
            &items,
            row,
        )
        .unwrap();
        let tree = TreeView::mount(
            world,
            tree_parent,
            UiId(2),
            ViewProps::new("Projects").with_reorder(),
            &nodes,
            row,
        )
        .unwrap();
        let expanded: Vec<ItemId> = nodes
            .iter()
            .filter(|node| nodes.iter().any(|other| other.parent == Some(node.item.id)))
            .map(|node| node.item.id)
            .collect();
        for id in expanded {
            let _ = tree.set_expanded(world, id, true);
        }
        let model = world.resource_mut::<Model>().unwrap();
        model.items = items;
        model.nodes = nodes;
        model.list = list;
        model.tree = tree;
    }
    if let Some(message) = message.filter(|text| !text.is_empty()) {
        if let Some(label) = world.get_mut::<Label>(status) {
            label.text = message;
        }
    }
}

fn main() {
    let mut app = App::new();
    build(&mut app);
    app.world_mut().insert_resource(WindowConfig {
        title: "rndr item drag".into(),
        size: LogicalSize::new(W, H),
        ..WindowConfig::default()
    });
    rndr::run(app);
}
