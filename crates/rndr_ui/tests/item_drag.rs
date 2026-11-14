//! Headless item drag. Not a native pointer or screen-reader certificate.
use rndr_core::*;
use rndr_ui::collections::{
    CollectionEvent, CollectionEventKind, Item, ItemId, ListView, RowSlot, TreeNode, TreeView,
    ViewProps,
};
use rndr_ui::drag::{
    announcement, classify, phase, preview, DragDrop, DragKeyboard, DragKind, DragNotice,
    DragNoticeKind, DragPayload, DragPhase, DragSource, DropCue, DropEdge, DropEnd, DropGeometry,
    DropPlace, DropTarget, AUTOSCROLL_MARGIN,
};
use rndr_ui::{text, Typography, UiId, UiPlugin};

fn boot() -> (Sim, Entity) {
    let mut sim = Sim::new();
    sim.app.add_plugin(UiPlugin);
    sim.world().insert_resource(Viewport::new(480.0, 640.0));
    sim.world().insert_resource(WindowConfig {
        dismiss: Dismiss::Close,
        ..WindowConfig::default()
    });
    let root = sim.world().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    (sim, root)
}

fn compose(world: &mut World, slot: RowSlot, item: &Item) {
    if children_of(world, slot.content).is_empty() {
        text(world, slot.content, &item.name, Typography::Label).unwrap();
    }
}

fn named(names: &[&str]) -> Vec<Item> {
    names
        .iter()
        .enumerate()
        .map(|(index, name)| Item::new(index as u64 + 1, *name))
        .collect()
}

enum Stroke {
    Press,
    Move,
    Release,
}

fn stroke(sim: &mut Sim, at: [f32; 2], stroke: Stroke) {
    let pointer = match stroke {
        Stroke::Press => Pointer {
            position: at,
            press_at: Some(at),
            pressed: true,
            down: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
        Stroke::Move => Pointer {
            position: at,
            down: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
        Stroke::Release => Pointer {
            position: at,
            released: true,
            button: PointerButton::Left,
            ..Pointer::default()
        },
    };
    let input = sim.world().resource_mut::<Input>().unwrap();
    input.pointer = pointer;
    input.keys.clear();
    input.key = None;
    sim.update();
}

fn rect_of(sim: &mut Sim, entity: Entity) -> Rect {
    *sim.world().get::<Rect>(entity).unwrap()
}

fn point_at(rect: Rect, y_fraction: f32) -> [f32; 2] {
    [rect.x + rect.w * 0.5, rect.y + rect.h * y_fraction]
}

fn notices(sim: &mut Sim) -> Vec<DragNoticeKind> {
    events::<DragNotice>(sim.world())
        .iter()
        .map(|notice| notice.kind)
        .collect()
}

fn reorders(sim: &mut Sim, view: Entity) -> Vec<rndr_ui::collections::ItemMove> {
    events::<CollectionEvent>(sim.world())
        .iter()
        .filter(|event| event.view == view)
        .filter_map(|event| match event.kind {
            CollectionEventKind::Reordered(movement) => Some(movement),
            _ => None,
        })
        .collect()
}

fn box_row(
    sim: &mut Sim,
    parent: Entity,
    name: &str,
    value: u64,
    source: bool,
    kind: DragKind,
    geometry: DropGeometry,
) -> Entity {
    let entity = sim.world().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(48.0),
            ..Layout::column()
        },
        Label::new(name),
        Focusable,
        DropTarget {
            enabled: true,
            kinds: vec![kind],
            owner: None,
            geometry,
            gate: None,
        },
    ));
    if source {
        sim.world().insert(
            entity,
            DragSource {
                payload: DragPayload {
                    kind,
                    value,
                    owner: None,
                },
                enabled: true,
                name: name.to_string(),
                keyboard: DragKeyboard::Pickup,
            },
        );
    }
    set_parent(sim.world(), entity, parent);
    entity
}

#[test]
fn classify_reports_before_after_inside_and_edge() {
    let rect = Rect {
        x: 10.0,
        y: 20.0,
        w: 100.0,
        h: 80.0,
    };
    assert_eq!(
        classify(rect, [40.0, 30.0], DropGeometry::reorder(Axis::Column)),
        Some(DropPlace::Before)
    );
    assert_eq!(
        classify(rect, [40.0, 80.0], DropGeometry::reorder(Axis::Column)),
        Some(DropPlace::After)
    );
    assert_eq!(
        classify(rect, [40.0, 60.0], DropGeometry::nest(Axis::Column)),
        Some(DropPlace::Inside)
    );
    assert_eq!(
        classify(rect, [40.0, 24.0], DropGeometry::split(Axis::Row)),
        Some(DropPlace::Edge(DropEdge {
            axis: Axis::Column,
            end: DropEnd::Leading,
        }))
    );
    assert_eq!(
        classify(rect, [40.0, 200.0], DropGeometry::reorder(Axis::Column)),
        None
    );
}

#[test]
fn reorder_helpers_move_lists_and_trees_and_reject_cycles() {
    let items = named(&["One", "Two", "Three"]);
    let moved =
        rndr_ui::collections::reorder_list(&items, ItemId(1), ItemId(3), DropPlace::After).unwrap();
    assert_eq!(
        moved.iter().map(|item| item.id).collect::<Vec<_>>(),
        vec![ItemId(2), ItemId(3), ItemId(1)]
    );
    assert!(
        rndr_ui::collections::reorder_list(&items, ItemId(2), ItemId(3), DropPlace::Before)
            .is_none()
    );
    assert!(
        rndr_ui::collections::reorder_list(&items, ItemId(1), ItemId(2), DropPlace::Inside)
            .is_none()
    );

    let nodes = vec![
        TreeNode {
            item: Item::new(1, "A"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "B"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(3, "C"),
            parent: Some(ItemId(2)),
        },
        TreeNode {
            item: Item::new(4, "D"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(5, "E"),
            parent: None,
        },
    ];
    let inside =
        rndr_ui::collections::reorder_tree(&nodes, ItemId(5), ItemId(1), DropPlace::Inside)
            .unwrap();
    assert_eq!(inside.last().unwrap().item.id, ItemId(5));
    assert_eq!(inside.last().unwrap().parent, Some(ItemId(1)));
    assert!(
        rndr_ui::collections::reorder_tree(&nodes, ItemId(1), ItemId(3), DropPlace::Inside)
            .is_none()
    );
    let promoted =
        rndr_ui::collections::reorder_tree(&nodes, ItemId(2), ItemId(5), DropPlace::After).unwrap();
    let branch = promoted
        .iter()
        .find(|node| node.item.id == ItemId(2))
        .unwrap();
    assert_eq!(branch.parent, None);
    assert!(promoted
        .iter()
        .any(|node| node.item.id == ItemId(3) && node.parent == Some(ItemId(2))));
}

#[test]
fn drag_drop_shows_a_preview_and_commits_on_release() {
    let (mut sim, root) = boot();
    let kind = DragKind(9);
    let geometry = DropGeometry::reorder(Axis::Column);
    let source = box_row(&mut sim, root, "Alpha", 1, true, kind, geometry);
    let _middle = box_row(&mut sim, root, "Beta", 2, false, kind, geometry);
    let target = box_row(&mut sim, root, "Gamma", 3, false, kind, geometry);
    sim.update();
    let from = point_at(rect_of(&mut sim, source), 0.5);
    stroke(&mut sim, from, Stroke::Press);
    assert_eq!(phase(sim.world()), DragPhase::Armed);
    let to = point_at(rect_of(&mut sim, target), 0.8);
    stroke(&mut sim, to, Stroke::Move);
    assert_eq!(phase(sim.world()), DragPhase::Dragging);
    let ghost = preview(sim.world()).unwrap();
    assert_eq!(sim.world().get::<Label>(ghost).unwrap().text, "Alpha");
    assert!(sim.world().has::<IgnorePointer>(ghost));
    let cue = *sim.world().get::<DropCue>(target).unwrap();
    assert_eq!(cue.place, DropPlace::After);
    assert!(cue.accepted);
    assert!(
        notices(&mut sim).contains(&DragNoticeKind::Started(DragPayload {
            kind,
            value: 1,
            owner: None,
        }))
    );
    stroke(&mut sim, to, Stroke::Release);
    assert_eq!(phase(sim.world()), DragPhase::Idle);
    assert!(
        notices(&mut sim).contains(&DragNoticeKind::Dropped(DragDrop {
            payload: DragPayload {
                kind,
                value: 1,
                owner: None
            },
            target,
            place: DropPlace::After,
        }))
    );
    assert!(announcement(sim.world()).contains("Dropped"));
    assert!(announcement(sim.world()).contains("Gamma"));
}

#[test]
fn escape_and_outside_release_cancel() {
    let (mut sim, root) = boot();
    let kind = DragKind(9);
    let geometry = DropGeometry::reorder(Axis::Column);
    let source = box_row(&mut sim, root, "Alpha", 1, true, kind, geometry);
    sim.update();
    let from = point_at(rect_of(&mut sim, source), 0.5);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, [from[0] + 24.0, from[1]], Stroke::Move);
    assert_eq!(phase(sim.world()), DragPhase::Dragging);
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys = vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
        input.key = Some(Key::Escape);
        input.pointer.down = true;
    }
    sim.update();
    assert_eq!(phase(sim.world()), DragPhase::Idle);
    assert!(!sim.world().resource::<Input>().unwrap().quit);
    assert!(notices(&mut sim).contains(&DragNoticeKind::Cancelled {
        payload: DragPayload {
            kind,
            value: 1,
            owner: None
        },
    }));
    assert!(announcement(sim.world()).contains("Cancelled"));

    stroke(&mut sim, from, Stroke::Press);
    let outside = [20.0, 500.0];
    stroke(&mut sim, outside, Stroke::Move);
    stroke(&mut sim, outside, Stroke::Release);
    assert!(notices(&mut sim)
        .iter()
        .any(|kind| matches!(kind, DragNoticeKind::Cancelled { .. })));
    assert!(!notices(&mut sim)
        .iter()
        .any(|kind| matches!(kind, DragNoticeKind::Dropped(_))));
}

#[test]
fn rejected_target_does_not_drop() {
    let (mut sim, root) = boot();
    let source = box_row(
        &mut sim,
        root,
        "Alpha",
        1,
        true,
        DragKind(9),
        DropGeometry::reorder(Axis::Column),
    );
    let foreign = box_row(
        &mut sim,
        root,
        "Other",
        2,
        false,
        DragKind(4),
        DropGeometry::reorder(Axis::Column),
    );
    sim.update();
    let from = point_at(rect_of(&mut sim, source), 0.5);
    let to = point_at(rect_of(&mut sim, foreign), 0.5);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, to, Stroke::Move);
    let cue = *sim.world().get::<DropCue>(foreign).unwrap();
    assert!(!cue.accepted);
    assert_eq!(
        sim.world().resource::<CursorStyle>().copied(),
        Some(CursorStyle::NoDrop)
    );
    stroke(&mut sim, to, Stroke::Release);
    assert!(notices(&mut sim)
        .iter()
        .any(|kind| matches!(kind, DragNoticeKind::Cancelled { .. })));
    assert!(!notices(&mut sim)
        .iter()
        .any(|kind| matches!(kind, DragNoticeKind::Dropped(_))));
    assert!(announcement(sim.world()).contains("Cannot drop"));
}

#[test]
fn autoscroll_moves_a_scrollport_near_the_edge() {
    let (mut sim, root) = boot();
    let port = sim.world().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(120.0),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    set_parent(sim.world(), port, root);
    let kind = DragKind(9);
    let geometry = DropGeometry::reorder(Axis::Column);
    let mut rows = Vec::new();
    for index in 0..8 {
        rows.push(box_row(
            &mut sim,
            port,
            &format!("Row {index}"),
            index,
            index == 0,
            kind,
            geometry,
        ));
    }
    sim.update();
    let port_rect = rect_of(&mut sim, port);
    let from = point_at(rect_of(&mut sim, rows[0]), 0.5);
    let edge = [port_rect.x + 12.0, port_rect.y + port_rect.h - 4.0];
    assert!(edge[1] > port_rect.y + port_rect.h - AUTOSCROLL_MARGIN);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, edge, Stroke::Move);
    for _ in 0..4 {
        stroke(&mut sim, edge, Stroke::Move);
    }
    assert!(
        sim.world().get::<Scroll>(port).unwrap().offset[1] < -8.0,
        "offset {}",
        sim.world().get::<Scroll>(port).unwrap().offset[1]
    );
}

#[test]
fn autoscroll_moves_a_virtual_list() {
    let (mut sim, root) = boot();
    let items: Vec<Item> = (1..=40)
        .map(|id| Item::new(id, format!("Row {id}")))
        .collect();
    let list = ListView::mount_virtual(
        sim.world(),
        root,
        UiId(3),
        ViewProps::new("Records").with_reorder(),
        &items,
        32.0,
        compose,
    )
    .unwrap();
    sim.world().get_mut::<Layout>(list.root).unwrap().height = Size::Px(96.0);
    sim.update();
    let row = list.row(sim.world(), ItemId(1)).unwrap().root;
    let port = rect_of(&mut sim, list.root);
    let edge = [port.x + 8.0, port.y + port.h - 4.0];
    let from_row = point_at(rect_of(&mut sim, row), 0.5);
    stroke(&mut sim, from_row, Stroke::Press);
    stroke(&mut sim, edge, Stroke::Move);
    for _ in 0..5 {
        stroke(&mut sim, edge, Stroke::Move);
    }
    assert!(
        sim.world().get::<Scroll>(list.root).unwrap().offset[1] < -8.0,
        "offset {}",
        sim.world().get::<Scroll>(list.root).unwrap().offset[1]
    );
}

#[test]
fn list_reorder_drop_keyboard_and_click_keep_their_contracts() {
    let (mut sim, root) = boot();
    let items = named(&["One", "Two", "Three", "Four"]);
    let list = ListView::mount(
        sim.world(),
        root,
        UiId(1),
        ViewProps::new("Tasks").with_reorder(),
        &items,
        compose,
    )
    .unwrap();
    sim.world().get_mut::<Layout>(list.root).unwrap().height = Size::Px(220.0);
    sim.update();
    let first = list.row(sim.world(), ItemId(1)).unwrap().root;
    let third = list.row(sim.world(), ItemId(3)).unwrap().root;
    let from = point_at(rect_of(&mut sim, first), 0.5);
    let to = point_at(rect_of(&mut sim, third), 0.85);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, to, Stroke::Move);
    assert_eq!(
        sim.world().get::<DropCue>(third).unwrap().place,
        DropPlace::After
    );
    stroke(&mut sim, to, Stroke::Release);
    assert_eq!(
        reorders(&mut sim, list.root),
        vec![rndr_ui::collections::ItemMove {
            id: ItemId(1),
            target: ItemId(3),
            place: DropPlace::After,
        }]
    );
    assert!(announcement(sim.world()).contains("Moved One after Three"));
    let status = access_tree(sim.world())
        .nodes
        .into_iter()
        .find(|node| node.role == AccessRole::Status)
        .unwrap();
    assert!(status.name.contains("Moved One after Three"));
    let announcer = entity_from_access_id(status.id).unwrap();
    assert_eq!(
        sim.world().get::<LiveRegion>(announcer).unwrap().0,
        LivePoliteness::Polite
    );
    assert!(rect_of(&mut sim, first).y < rect_of(&mut sim, third).y);
    let next =
        rndr_ui::collections::reorder_list(&items, ItemId(1), ItemId(3), DropPlace::After).unwrap();
    ListView::mount(
        sim.world(),
        root,
        UiId(1),
        ViewProps::new("Tasks").with_reorder(),
        &next,
        compose,
    )
    .unwrap();
    sim.update();
    let moved = list.row(sim.world(), ItemId(1)).unwrap().root;
    let stayed = list.row(sim.world(), ItemId(3)).unwrap().root;
    assert!(rect_of(&mut sim, moved).y > rect_of(&mut sim, stayed).y);

    let second = list.row(sim.world(), ItemId(2)).unwrap().root;
    let at = point_at(rect_of(&mut sim, second), 0.5);
    stroke(&mut sim, at, Stroke::Press);
    stroke(&mut sim, at, Stroke::Release);
    assert_eq!(list.selected(sim.world()), Some(ItemId(2)));
    assert_eq!(phase(sim.world()), DragPhase::Idle);

    list.focus(sim.world(), ItemId(2)).unwrap();
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer = Pointer::default();
        input.keys = vec![KeyEvent::named(
            NamedKey::ArrowDown,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        )];
        input.key = None;
    }
    sim.update();
    assert_eq!(
        reorders(&mut sim, list.root),
        vec![rndr_ui::collections::ItemMove {
            id: ItemId(2),
            target: ItemId(3),
            place: DropPlace::After,
        }]
    );
    assert!(announcement(sim.world()).contains("Moved Two after Three"));
}

#[test]
fn drag_in_the_safe_band_does_not_scroll_the_list() {
    let (mut sim, root) = boot();
    let items: Vec<Item> = (1..=24)
        .map(|id| Item::new(id, format!("Row {id}")))
        .collect();
    let list = ListView::mount(
        sim.world(),
        root,
        UiId(4),
        ViewProps::new("Long").with_reorder(),
        &items,
        compose,
    )
    .unwrap();
    sim.world().get_mut::<Layout>(list.root).unwrap().height = Size::Px(200.0);
    sim.update();
    let port = rect_of(&mut sim, list.root);
    let rows: Vec<Entity> = (1..=24)
        .map(|id| list.row(sim.world(), ItemId(id)).unwrap().root)
        .collect();
    let row = rows
        .into_iter()
        .find(|entity| {
            let y = point_at(rect_of(&mut sim, *entity), 0.5)[1];
            y > port.y + AUTOSCROLL_MARGIN + 8.0 && y + 36.0 < port.y + port.h - AUTOSCROLL_MARGIN
        })
        .unwrap();
    let from = point_at(rect_of(&mut sim, row), 0.5);
    let to = [from[0] + 20.0, from[1] + 24.0];
    assert!(to[1] < port.y + port.h - AUTOSCROLL_MARGIN);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, to, Stroke::Move);
    assert_eq!(phase(sim.world()), DragPhase::Dragging);
    assert_eq!(sim.world().get::<Scroll>(list.root).unwrap().offset[1], 0.0);
}

#[test]
fn tree_drop_inside_and_descendant_reject() {
    let (mut sim, root) = boot();
    let nodes = vec![
        TreeNode {
            item: Item::new(1, "Root"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "Branch"),
            parent: Some(ItemId(1)),
        },
        TreeNode {
            item: Item::new(3, "Leaf"),
            parent: Some(ItemId(2)),
        },
        TreeNode {
            item: Item::new(4, "Other"),
            parent: None,
        },
    ];
    let tree = TreeView::mount(
        sim.world(),
        root,
        UiId(2),
        ViewProps::new("Tree").with_reorder(),
        &nodes,
        compose,
    )
    .unwrap();
    tree.set_expanded(sim.world(), ItemId(1), true).unwrap();
    tree.set_expanded(sim.world(), ItemId(2), true).unwrap();
    sim.update();
    let other = tree.row(sim.world(), ItemId(4)).unwrap().root;
    let root_row = tree.row(sim.world(), ItemId(1)).unwrap().root;
    let leaf = tree.row(sim.world(), ItemId(3)).unwrap().root;
    let onto = point_at(rect_of(&mut sim, root_row), 0.5);
    let other_at = point_at(rect_of(&mut sim, other), 0.5);
    stroke(&mut sim, other_at, Stroke::Press);
    stroke(&mut sim, onto, Stroke::Move);
    assert_eq!(
        sim.world().get::<DropCue>(root_row).unwrap().place,
        DropPlace::Inside
    );
    assert!(sim.world().get::<DropCue>(root_row).unwrap().accepted);
    stroke(&mut sim, onto, Stroke::Release);
    assert_eq!(
        reorders(&mut sim, tree.root),
        vec![rndr_ui::collections::ItemMove {
            id: ItemId(4),
            target: ItemId(1),
            place: DropPlace::Inside,
        }]
    );

    let from = point_at(rect_of(&mut sim, root_row), 0.5);
    let onto_leaf = point_at(rect_of(&mut sim, leaf), 0.5);
    stroke(&mut sim, from, Stroke::Press);
    stroke(&mut sim, onto_leaf, Stroke::Move);
    assert!(!sim.world().get::<DropCue>(leaf).unwrap().accepted);
    stroke(&mut sim, onto_leaf, Stroke::Release);
    assert!(reorders(&mut sim, tree.root).is_empty());
}

#[test]
fn disabled_row_is_not_a_drag_source() {
    let (mut sim, root) = boot();
    let mut items = named(&["One", "Two"]);
    items[0].enabled = false;
    let list = ListView::mount(
        sim.world(),
        root,
        UiId(5),
        ViewProps::new("Tasks").with_reorder(),
        &items,
        compose,
    )
    .unwrap();
    sim.update();
    let row = list.row(sim.world(), ItemId(1)).unwrap().root;
    let enabled = list.row(sim.world(), ItemId(2)).unwrap().root;
    assert!(!sim.world().has::<DragSource>(row));
    assert!(sim.world().has::<DragSource>(enabled));
}

#[test]
fn keyboard_pickup_moves_announces_and_escape_cancels() {
    let (mut sim, root) = boot();
    let kind = DragKind(9);
    let geometry = DropGeometry::reorder(Axis::Column);
    let source = box_row(&mut sim, root, "Alpha", 1, true, kind, geometry);
    let target = box_row(&mut sim, root, "Beta", 2, false, kind, geometry);
    sim.update();
    request_focus(sim.world(), source);
    sim.update();
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.pointer = Pointer::default();
        input.keys = vec![KeyEvent::named(
            NamedKey::Enter,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        )];
    }
    sim.update();
    assert_eq!(phase(sim.world()), DragPhase::Keyboard);
    assert!(announcement(sim.world()).contains("Dragging Alpha"));
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys = vec![KeyEvent::named(NamedKey::ArrowDown, Modifiers::default())];
        input.key = None;
    }
    sim.update();
    assert!(announcement(sim.world()).contains("Beta"));
    assert_eq!(
        sim.world().get::<DropCue>(target).unwrap().place,
        DropPlace::After
    );
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys = vec![KeyEvent::named(NamedKey::Enter, Modifiers::default())];
    }
    sim.update();
    assert!(
        notices(&mut sim).contains(&DragNoticeKind::Dropped(DragDrop {
            payload: DragPayload {
                kind,
                value: 1,
                owner: None
            },
            target,
            place: DropPlace::After,
        }))
    );

    request_focus(sim.world(), source);
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys = vec![KeyEvent::named(
            NamedKey::Enter,
            Modifiers {
                alt: true,
                ..Modifiers::default()
            },
        )];
    }
    sim.update();
    {
        let input = sim.world().resource_mut::<Input>().unwrap();
        input.keys = vec![KeyEvent::named(NamedKey::Escape, Modifiers::default())];
        input.key = Some(Key::Escape);
    }
    sim.update();
    assert_eq!(phase(sim.world()), DragPhase::Idle);
    assert!(announcement(sim.world()).contains("Cancelled"));
    assert!(!sim.world().resource::<Input>().unwrap().quit);
}
