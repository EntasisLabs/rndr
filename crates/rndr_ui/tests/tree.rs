//! Virtual tree: windowing, keyboard, selection, load, drag, and a large model.
//! Headless. Not a pointer, keyboard, or screen-reader certificate.
use rndr_core::*;
use rndr_ui::drag::{announcement, DropPlace};
use rndr_ui::tree::*;
use rndr_ui::{UiError, UiId, UiPlugin};
use std::time::{Duration, Instant};

fn boot(width: f32, height: f32) -> (Sim, Entity) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(width, height));
    let root = sim.world().spawn((Layout {
        width: Size::Px(width),
        height: Size::Px(height),
        ..Layout::column()
    },));
    (sim, root)
}

fn settle(sim: &mut Sim) {
    sim.update();
    sim.update();
}

fn size(sim: &mut Sim, entity: Entity, width: f32, height: f32) {
    let layout = sim.world().get_mut::<Layout>(entity).unwrap();
    layout.width = Size::Px(width);
    layout.height = Size::Px(height);
}

fn mount(
    sim: &mut Sim,
    parent: Entity,
    props: TreeProps,
    nodes: &[TreeNode],
    expanded: &[NodeId],
) -> TreeHandle {
    let tree = VirtualTree::mount(sim.world(), parent, UiId(1), props, nodes, expanded).unwrap();
    size(sim, tree.root, 320.0, 160.0);
    tree
}

fn key(sim: &mut Sim, event: KeyEvent) {
    sim.world().resource_mut::<Input>().unwrap().keys = vec![event];
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().keys.clear();
}

fn named(key: NamedKey) -> KeyEvent {
    KeyEvent::named(key, Modifiers::default())
}

fn click(sim: &mut Sim, entity: Entity) {
    let rect = sim.world().get::<Rect>(entity).copied().unwrap();
    let at = [rect.x + rect.w * 0.5, rect.y + rect.h * 0.5];
    press(sim, at, PointerButton::Left);
    release(sim, at);
}

fn press(sim: &mut Sim, at: [f32; 2], button: PointerButton) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        press_at: Some(at),
        pressed: true,
        down: true,
        button,
        ..Pointer::default()
    };
    sim.update();
}

fn release(sim: &mut Sim, at: [f32; 2]) {
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: at,
        released: true,
        ..Pointer::default()
    };
    sim.update();
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer::default();
}

fn at_y(world: &World, entity: Entity, fraction: f32) -> [f32; 2] {
    let rect = world.get::<Rect>(entity).copied().unwrap();
    [rect.x + rect.w * 0.5, rect.y + rect.h * fraction]
}

fn kinds(sim: &mut Sim, tree: TreeHandle) -> Vec<TreeEventKind> {
    events::<TreeEvent>(sim.world())
        .iter()
        .filter(|event| event.tree == tree.root)
        .map(|event| event.kind)
        .collect()
}

fn sample() -> Vec<TreeNode> {
    vec![
        TreeNode::new(1, "src").icon(Icon::Folder).expandable(true),
        TreeNode::new(2, "main.rs")
            .child(NodeId(1))
            .icon(Icon::File)
            .secondary("edited")
            .badge("2"),
        TreeNode::new(3, "lib.rs").child(NodeId(1)).icon(Icon::File),
        TreeNode::new(4, "pkg").child(NodeId(1)).expandable(true),
        TreeNode::new(5, "a.rs").child(NodeId(4)),
        TreeNode::new(6, "b.rs").child(NodeId(4)),
        TreeNode::new(7, "readme").icon(Icon::File),
    ]
}

#[test]
fn only_the_visible_window_is_realized() {
    let (mut sim, parent) = boot(320.0, 200.0);
    let mut nodes = vec![TreeNode::new(1, "root").expandable(true)];
    for id in 2..=80 {
        nodes.push(TreeNode::new(id, format!("n{id}")).child(NodeId(1)));
    }
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &nodes,
        &[NodeId(1)],
    );
    settle(&mut sim);
    let realized = tree.realized_count(sim.world());
    assert!(realized > 0, "rows in the viewport");
    assert!(realized < 12, "realized {realized}, model {}", nodes.len());
    assert_eq!(tree.visible_count(sim.world()), nodes.len());
    assert!(tree.row(sim.world(), NodeId(2)).is_some());
    assert!(tree.row(sim.world(), NodeId(80)).is_none());
    tree.reveal(sim.world(), NodeId(80)).unwrap();
    assert!(tree.row(sim.world(), NodeId(80)).is_some());
    assert!(tree.row(sim.world(), NodeId(2)).is_none());
}

#[test]
fn keyboard_follows_the_tree_pattern() {
    let (mut sim, parent) = boot(320.0, 400.0);
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &sample(),
        &[NodeId(1)],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    tree.focus(sim.world(), NodeId(1)).unwrap();
    key(&mut sim, named(NamedKey::ArrowRight));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(2)));
    key(&mut sim, named(NamedKey::ArrowDown));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(3)));
    key(&mut sim, named(NamedKey::End));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(7)));
    key(&mut sim, named(NamedKey::Home));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(1)));
    tree.focus(sim.world(), NodeId(4)).unwrap();
    key(&mut sim, named(NamedKey::ArrowRight));
    assert!(tree.expanded(sim.world(), NodeId(4)));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(4)));
    key(&mut sim, named(NamedKey::ArrowRight));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(5)));
    key(&mut sim, named(NamedKey::ArrowLeft));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(4)));
    key(&mut sim, named(NamedKey::ArrowLeft));
    assert!(!tree.expanded(sim.world(), NodeId(4)));
    tree.focus(sim.world(), NodeId(1)).unwrap();
    key(&mut sim, KeyEvent::character('r', Modifiers::default()));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(7)));
    tree.focus(sim.world(), NodeId(2)).unwrap();
    key(&mut sim, named(NamedKey::Enter));
    assert!(kinds(&mut sim, tree).contains(&TreeEventKind::Activated(NodeId(2))));
    key(&mut sim, named(NamedKey::F(2)));
    assert!(kinds(&mut sim, tree).contains(&TreeEventKind::RenameRequested(NodeId(2))));
}

#[test]
fn asterisk_expands_sibling_branches() {
    let (mut sim, parent) = boot(320.0, 400.0);
    let nodes = vec![
        TreeNode::new(1, "a").expandable(true),
        TreeNode::new(2, "a-child").child(NodeId(1)),
        TreeNode::new(3, "b").expandable(true),
        TreeNode::new(4, "b-child").child(NodeId(3)),
    ];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &nodes,
        &[],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    tree.focus(sim.world(), NodeId(1)).unwrap();
    key(&mut sim, KeyEvent::character('*', Modifiers::default()));
    assert!(tree.expanded(sim.world(), NodeId(1)));
    assert!(tree.expanded(sim.world(), NodeId(3)));
    assert_eq!(tree.focused(sim.world()), Some(NodeId(1)));
    assert!(tree.row(sim.world(), NodeId(2)).is_some());
    assert!(tree.row(sim.world(), NodeId(4)).is_some());
}

#[test]
fn single_and_multiple_selection() {
    let (mut sim, parent) = boot(320.0, 400.0);
    let nodes = vec![
        TreeNode::new(1, "one"),
        TreeNode::new(2, "two"),
        TreeNode::new(3, "three"),
    ];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &nodes,
        &[],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    let first = tree.row(sim.world(), NodeId(1)).unwrap();
    click(&mut sim, first);
    assert_eq!(tree.selected(sim.world()), Some(NodeId(1)));
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").multiple().row_height(32.0),
        &nodes,
        &[],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    let first = tree.row(sim.world(), NodeId(1)).unwrap();
    click(&mut sim, first);
    sim.world().resource_mut::<Input>().unwrap().shift = true;
    let third = tree.row(sim.world(), NodeId(3)).unwrap();
    click(&mut sim, third);
    sim.world().resource_mut::<Input>().unwrap().shift = false;
    assert_eq!(
        tree.selection(sim.world()),
        [NodeId(1), NodeId(2), NodeId(3)].into()
    );
    sim.world().resource_mut::<Input>().unwrap().ctrl = true;
    let second = tree.row(sim.world(), NodeId(2)).unwrap();
    click(&mut sim, second);
    sim.world().resource_mut::<Input>().unwrap().ctrl = false;
    assert_eq!(tree.selection(sim.world()), [NodeId(1), NodeId(3)].into());
    let first = tree.row(sim.world(), NodeId(1)).unwrap();
    click(&mut sim, first);
    tree.focus(sim.world(), NodeId(1)).unwrap();
    let modifiers = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    key(&mut sim, KeyEvent::named(NamedKey::ArrowDown, modifiers));
    assert!(tree.selection(sim.world()).contains(&NodeId(1)));
    assert!(tree.selection(sim.world()).contains(&NodeId(2)));
}

#[test]
fn roving_tab_stop_and_row_access() {
    let (mut sim, parent) = boot(320.0, 400.0);
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").multiple().row_height(32.0),
        &sample(),
        &[NodeId(1), NodeId(4)],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    tree.focus(sim.world(), NodeId(5)).unwrap();
    let row = tree.row(sim.world(), NodeId(5)).unwrap();
    let access = sim.world().get::<CollectionAccess>(row).copied().unwrap();
    assert_eq!(access.role, AccessRole::TreeItem);
    assert_eq!(access.level, Some(3));
    assert_eq!(access.position, Some(1));
    assert_eq!(access.set_size, Some(2));
    assert_eq!(access.expanded, None);
    let root = sim
        .world()
        .get::<CollectionAccess>(tree.root)
        .copied()
        .unwrap();
    assert_eq!(root.role, AccessRole::Tree);
    assert!(root.multiselect);
    let stops = [
        NodeId(1),
        NodeId(2),
        NodeId(3),
        NodeId(4),
        NodeId(5),
        NodeId(6),
        NodeId(7),
    ]
    .into_iter()
    .filter(|id| {
        tree.row(sim.world(), *id)
            .is_some_and(|row| sim.world().get::<TabStop>(row).is_some_and(|stop| stop.0))
    })
    .count();
    assert_eq!(stops, 1);
    assert!(!sim.world().get::<TabStop>(tree.root).unwrap().0);
    let branch = tree.row(sim.world(), NodeId(4)).unwrap();
    let branch_access = sim
        .world()
        .get::<CollectionAccess>(branch)
        .copied()
        .unwrap();
    assert_eq!(branch_access.expanded, Some(true));
    assert_eq!(branch_access.level, Some(2));
    let guide_root = children_of(sim.world(), row)[0];
    let guides = children_of(sim.world(), guide_root);
    let shown = guides
        .iter()
        .filter(|column| !sim.world().has::<Hidden>(**column))
        .count();
    assert_eq!(shown, 2, "depth 3 item draws two indent columns");
    let named = tree.row(sim.world(), NodeId(2)).unwrap();
    let name = sim.world().get::<Semantics>(named).unwrap();
    assert_eq!(name.name.as_deref(), Some("main.rs, edited, 2"));
}

#[test]
fn lazy_children_show_a_loading_row_until_the_host_mounts_them() {
    let (mut sim, parent) = boot(320.0, 240.0);
    let nodes = vec![TreeNode::new(1, "notes").expandable(true)];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &nodes,
        &[],
    );
    settle(&mut sim);
    tree.focus(sim.world(), NodeId(1)).unwrap();
    key(&mut sim, named(NamedKey::ArrowRight));
    assert!(kinds(&mut sim, tree).contains(&TreeEventKind::LoadChildren(NodeId(1))));
    assert!(kinds(&mut sim, tree).contains(&TreeEventKind::Expanded(NodeId(1), true)));
    assert!(loading_row(&mut sim));
    let nodes = vec![
        TreeNode::new(1, "notes").expandable(true),
        TreeNode::new(2, "todo.md").child(NodeId(1)),
    ];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").row_height(32.0),
        &nodes,
        &[NodeId(1)],
    );
    settle(&mut sim);
    assert!(tree.row(sim.world(), NodeId(2)).is_some());
    assert!(!loading_row(&mut sim));
}

fn loading_row(sim: &mut Sim) -> bool {
    sim.world()
        .entities_with::<Semantics>()
        .into_iter()
        .any(|entity| {
            sim.world()
                .get::<Semantics>(entity)
                .and_then(|semantics| semantics.name.as_deref())
                == Some("Loading")
                && sim
                    .world()
                    .get::<CollectionAccess>(entity)
                    .is_some_and(|access| access.role == AccessRole::TreeItem)
        })
}

#[test]
fn drag_reorders_and_refuses_a_descendant() {
    let (mut sim, parent) = boot(320.0, 400.0);
    let nodes = sample();
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").with_reorder().row_height(32.0),
        &nodes,
        &[NodeId(1), NodeId(4)],
    );
    size(&mut sim, tree.root, 320.0, 400.0);
    settle(&mut sim);
    let from = tree.row(sim.world(), NodeId(3)).unwrap();
    let onto = tree.row(sim.world(), NodeId(2)).unwrap();
    let from_at = at_y(sim.world(), from, 0.5);
    press(&mut sim, from_at, PointerButton::Left);
    let onto_at = at_y(sim.world(), onto, 0.1);
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: onto_at,
        down: true,
        ..Pointer::default()
    };
    sim.update();
    release(&mut sim, onto_at);
    let moved = kinds(&mut sim, tree)
        .into_iter()
        .find_map(|kind| match kind {
            TreeEventKind::Reordered(move_) => Some(move_),
            _ => None,
        });
    let moved = moved.expect("before");
    assert_eq!(moved.id, NodeId(3));
    assert_eq!(moved.target, NodeId(2));
    assert_eq!(moved.place, DropPlace::Before);
    let next = reorder_nodes(&nodes, moved.id, moved.target, moved.place).unwrap();
    assert_eq!(next[1].id, NodeId(3));
    assert_eq!(next[2].id, NodeId(2));
    assert!(reorder_nodes(&nodes, NodeId(1), NodeId(5), DropPlace::Inside).is_none());
    assert!(announcement(sim.world()).contains("Moved"));

    let parent_row = tree.row(sim.world(), NodeId(1)).unwrap();
    let child = tree.row(sim.world(), NodeId(5)).unwrap();
    let parent_at = at_y(sim.world(), parent_row, 0.5);
    press(&mut sim, parent_at, PointerButton::Left);
    let child_at = at_y(sim.world(), child, 0.5);
    sim.world().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: child_at,
        down: true,
        ..Pointer::default()
    };
    sim.update();
    release(&mut sim, child_at);
    assert!(!kinds(&mut sim, tree)
        .iter()
        .any(|kind| matches!(kind, TreeEventKind::Reordered(_))));
    assert!(announcement(sim.world()).contains("Cannot drop"));
}

#[test]
fn alt_arrow_asks_the_host_to_move() {
    let (mut sim, parent) = boot(320.0, 240.0);
    let nodes = vec![TreeNode::new(1, "one"), TreeNode::new(2, "two")];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").with_reorder().row_height(32.0),
        &nodes,
        &[],
    );
    settle(&mut sim);
    tree.focus(sim.world(), NodeId(1)).unwrap();
    let modifiers = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    key(&mut sim, KeyEvent::named(NamedKey::ArrowDown, modifiers));
    assert!(
        kinds(&mut sim, tree).contains(&TreeEventKind::Reordered(TreeMove {
            id: NodeId(1),
            target: NodeId(2),
            place: DropPlace::After,
        }))
    );
    assert!(announcement(sim.world()).starts_with("Moved one"));
}

#[test]
fn context_menu_is_readable_on_the_next_frame() {
    let (mut sim, parent) = boot(320.0, 240.0);
    let nodes = vec![TreeNode::new(1, "one"), TreeNode::new(2, "two")];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files").multiple().row_height(32.0),
        &nodes,
        &[],
    );
    settle(&mut sim);
    let row = tree.row(sim.world(), NodeId(2)).unwrap();
    let at = at_y(sim.world(), row, 0.5);
    press(&mut sim, at, PointerButton::Right);
    sim.update();
    let menu = kinds(&mut sim, tree)
        .into_iter()
        .find_map(|kind| match kind {
            TreeEventKind::ContextMenu(request) => Some(request),
            _ => None,
        });
    let menu = menu.expect("menu");
    assert_eq!(menu.id, NodeId(2));
    assert_eq!(tree.selection(sim.world()), [NodeId(2)].into());
}

#[test]
fn disabled_ancestor_blocks_focus_and_bad_forests_are_refused() {
    let (mut sim, parent) = boot(320.0, 240.0);
    let nodes = vec![
        TreeNode::new(1, "hidden").enabled(false).expandable(true),
        TreeNode::new(2, "child").child(NodeId(1)),
    ];
    let tree = mount(
        &mut sim,
        parent,
        TreeProps::new("Files"),
        &nodes,
        &[NodeId(1)],
    );
    settle(&mut sim);
    assert_eq!(
        tree.focus(sim.world(), NodeId(2)),
        Err(UiError::InvalidProps)
    );
    let again = VirtualTree::mount(
        sim.world(),
        parent,
        UiId(1),
        TreeProps::new("Files"),
        &[TreeNode::new(1, "a"), TreeNode::new(1, "b")],
        &[],
    );
    assert_eq!(again.err(), Some(UiError::InvalidProps));
    assert_eq!(tree.node_count(sim.world()), 2);
    assert!(VirtualTree::mount(
        sim.world(),
        parent,
        UiId(1),
        TreeProps::new("  "),
        &nodes,
        &[],
    )
    .is_err());
    assert!(VirtualTree::mount(
        sim.world(),
        parent,
        UiId(1),
        TreeProps::new("Files"),
        &[TreeNode::new(2, "orphan").child(NodeId(9))],
        &[],
    )
    .is_err());
    assert!(reorder_nodes(
        &[
            TreeNode::new(1, "a").expandable(true),
            TreeNode::new(2, "b").child(NodeId(1)),
        ],
        NodeId(1),
        NodeId(2),
        DropPlace::Inside,
    )
    .is_none());
}

#[test]
fn one_hundred_thousand_nodes_stay_outside_the_window() {
    let (mut sim, parent) = boot(320.0, 320.0);
    let mut nodes = Vec::with_capacity(100_001);
    nodes.push(TreeNode::new(1, "root").expandable(true));
    for id in 2..=100_001 {
        nodes.push(TreeNode::new(id, format!("n{id}")).child(NodeId(1)));
    }
    let started = Instant::now();
    let tree = VirtualTree::mount(
        sim.world(),
        parent,
        UiId(1),
        TreeProps::new("Large").row_height(32.0),
        &nodes,
        &[NodeId(1)],
    )
    .unwrap();
    size(&mut sim, tree.root, 320.0, 320.0);
    let mounted = started.elapsed();
    settle(&mut sim);
    let framed = started.elapsed();
    let realized = tree.realized_count(sim.world());
    eprintln!(
        "virtual tree 100001 nodes: mount {mounted:?}, mount+2 frames {framed:?}, realized {realized}, visible {}",
        tree.visible_count(sim.world())
    );
    assert_eq!(tree.node_count(sim.world()), 100_001);
    assert_eq!(tree.visible_count(sim.world()), 100_001);
    assert!(realized > 0 && realized < 20, "realized {realized}");
    assert!(mounted < Duration::from_secs(10), "mount {mounted:?}");
    tree.reveal(sim.world(), NodeId(100_001)).unwrap();
    assert!(tree.row(sim.world(), NodeId(100_001)).is_some());
    assert!(tree.row(sim.world(), NodeId(2)).is_none());
}
