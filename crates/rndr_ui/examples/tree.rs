//! A file tree the host owns, plus a timed mount of a large model.
//!
//! Run: `cargo run -p rndr_ui --example tree`
//!
//! The window is not opened. The host applies `reorder_nodes` when the tree
//! asks, and it supplies children after `LoadChildren`.
use rndr_core::*;
use rndr_ui::drag::DropPlace;
use rndr_ui::tree::*;
use rndr_ui::{UiId, UiPlugin};
use std::time::Instant;

fn main() {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(420.0, 360.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(420.0),
        height: Size::Px(360.0),
        ..Layout::column()
    },));
    let mut nodes = vec![
        TreeNode::new(1, "src").icon(Icon::Folder).expandable(true),
        TreeNode::new(2, "main.rs")
            .child(NodeId(1))
            .icon(Icon::File)
            .secondary("edited"),
        TreeNode::new(3, "lib.rs").child(NodeId(1)).icon(Icon::File),
        TreeNode::new(4, "notes")
            .icon(Icon::Folder)
            .expandable(true),
    ];
    let mut expanded = vec![NodeId(1)];
    let mut tree = mount(&mut app, root, &nodes, &expanded);
    app.update();
    app.update();
    assert!(tree.row(app.world(), NodeId(2)).is_some());
    assert!(tree.row(app.world(), NodeId(4)).is_some());

    tree.focus(app.world_mut(), NodeId(4)).unwrap();
    key(&mut app, NamedKey::ArrowRight);
    let loaded = events::<TreeEvent>(app.world())
        .iter()
        .any(|event| event.kind == TreeEventKind::LoadChildren(NodeId(4)));
    assert!(loaded);
    nodes.push(
        TreeNode::new(5, "todo.md")
            .child(NodeId(4))
            .icon(Icon::File)
            .badge("1"),
    );
    expanded.push(NodeId(4));
    tree = mount(&mut app, root, &nodes, &expanded);
    app.update();
    assert!(tree.expanded(app.world(), NodeId(4)));
    assert!(tree.row(app.world(), NodeId(5)).is_some());

    let from = tree.row(app.world(), NodeId(3)).unwrap();
    let onto = tree.row(app.world(), NodeId(2)).unwrap();
    let start = center(app.world(), from);
    let before = at_y(app.world(), onto, 0.15);
    pointer(&mut app, start, true);
    pointer(&mut app, before, false);
    pointer(&mut app, before, false);
    release(&mut app, before);
    let moved = events::<TreeEvent>(app.world())
        .iter()
        .find_map(|event| match event.kind {
            TreeEventKind::Reordered(move_) => Some(move_),
            _ => None,
        })
        .expect("reorder");
    assert_eq!(moved.place, DropPlace::Before);
    nodes = reorder_nodes(&nodes, moved.id, moved.target, moved.place).expect("host applies");
    mount(&mut app, root, &nodes, &expanded);
    app.update();
    let order: Vec<_> = nodes
        .iter()
        .filter(|node| node.parent == Some(NodeId(1)))
        .map(|node| node.id)
        .collect();
    assert_eq!(order, vec![NodeId(3), NodeId(2)]);

    let started = Instant::now();
    let mut wide = Vec::with_capacity(100_001);
    wide.push(TreeNode::new(1, "root").expandable(true));
    for id in 2..=100_001 {
        wide.push(TreeNode::new(id, format!("n{id}")).child(NodeId(1)));
    }
    let built = started.elapsed();
    let mounted_at = Instant::now();
    let wide_tree = VirtualTree::mount(
        app.world_mut(),
        root,
        UiId(2),
        TreeProps::new("Large").row_height(32.0),
        &wide,
        &[NodeId(1)],
    )
    .unwrap();
    size(app.world_mut(), wide_tree.root, 420.0, 320.0);
    let mounted = mounted_at.elapsed();
    app.update();
    app.update();
    let framed = mounted_at.elapsed();
    println!(
        "moved lib.rs before main.rs; 100001 nodes build {built:?}, mount {mounted:?}, mount+2 frames {framed:?}, realized {}, visible {}",
        wide_tree.realized_count(app.world()),
        wide_tree.visible_count(app.world())
    );
    assert!(wide_tree.realized_count(app.world()) < 40);
    assert!(wide_tree.visible_count(app.world()) > 100_000);
}

fn mount(app: &mut App, parent: Entity, nodes: &[TreeNode], expanded: &[NodeId]) -> TreeHandle {
    let tree = VirtualTree::mount(
        app.world_mut(),
        parent,
        UiId(1),
        TreeProps::new("Files").with_reorder().row_height(32.0),
        nodes,
        expanded,
    )
    .unwrap();
    size(app.world_mut(), tree.root, 420.0, 360.0);
    tree
}

fn size(world: &mut World, entity: Entity, width: f32, height: f32) {
    let layout = world.get_mut::<Layout>(entity).unwrap();
    layout.width = Size::Px(width);
    layout.height = Size::Px(height);
}

fn center(world: &World, entity: Entity) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5]
}

fn at_y(world: &World, entity: Entity, fraction: f32) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * 0.5, rect.y + rect.h * fraction]
}

fn pointer(app: &mut App, at: [f32; 2], press: bool) {
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        press_at: press.then_some(at),
        pressed: press,
        down: true,
        ..Pointer::default()
    };
    app.update();
}

fn release(app: &mut App, at: [f32; 2]) {
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        released: true,
        ..Pointer::default()
    };
    app.update();
}

fn key(app: &mut App, named: NamedKey) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .clear();
}
