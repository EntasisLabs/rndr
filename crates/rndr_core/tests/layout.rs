use rndr_core::{
    apply_scroll, despawn, hit_test, layout_propagate, set_parent, spawn_node, toggle_disclosure,
    Axis, Clip, Color, Disclosure, Input, Layout, NodeSpec, Rect, Scroll, Size, Viewport, World,
};

fn px(w: f32, h: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Px(h),
    }
}

#[test]
fn column_with_padding_and_nested_row() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 400.0,
        height: 300.0,
    });

    let column = world.spawn(Layout {
        axis: Axis::Column,
        gap: 8.0,
        padding: 8.0,
        width: Size::Hug,
        height: Size::Hug,
    });
    let row = world.spawn(Layout {
        axis: Axis::Row,
        gap: 4.0,
        padding: 0.0,
        width: Size::Hug,
        height: Size::Hug,
    });
    let a = world.spawn(px(10.0, 10.0));
    let b = world.spawn(px(10.0, 10.0));
    set_parent(&mut world, a, row);
    set_parent(&mut world, b, row);
    set_parent(&mut world, row, column);

    layout_propagate(&mut world);

    let column_rect = *world.get::<Rect>(column).unwrap();
    let row_rect = *world.get::<Rect>(row).unwrap();
    let a_rect = *world.get::<Rect>(a).unwrap();
    let b_rect = *world.get::<Rect>(b).unwrap();

    assert_eq!(
        column_rect,
        Rect {
            x: 0.0,
            y: 0.0,
            w: 40.0,
            h: 26.0,
        }
    );
    assert_eq!(
        row_rect,
        Rect {
            x: 8.0,
            y: 8.0,
            w: 24.0,
            h: 10.0,
        }
    );
    assert_eq!(
        a_rect,
        Rect {
            x: 8.0,
            y: 8.0,
            w: 10.0,
            h: 10.0,
        }
    );
    assert_eq!(
        b_rect,
        Rect {
            x: 22.0,
            y: 8.0,
            w: 10.0,
            h: 10.0,
        }
    );
}

#[test]
fn cascade_despawn_closes_the_gap() {
    let mut world = World::new();
    world.insert_resource(Viewport::default());

    let column = world.spawn(Layout {
        axis: Axis::Column,
        gap: 4.0,
        padding: 0.0,
        width: Size::Hug,
        height: Size::Hug,
    });
    let first = world.spawn(px(20.0, 20.0));
    let middle = world.spawn(px(20.0, 20.0));
    let last = world.spawn(px(20.0, 20.0));
    set_parent(&mut world, first, column);
    set_parent(&mut world, middle, column);
    set_parent(&mut world, last, column);

    let grandchild = world.spawn(px(8.0, 8.0));
    set_parent(&mut world, grandchild, middle);

    layout_propagate(&mut world);
    assert_eq!(world.get::<Rect>(column).unwrap().h, 68.0);

    despawn(&mut world, middle);
    assert!(!world.is_alive(middle));
    assert!(!world.is_alive(grandchild));
    assert!(world.is_alive(first));
    assert!(world.is_alive(last));

    layout_propagate(&mut world);
    assert_eq!(world.get::<Rect>(column).unwrap().h, 44.0);
    assert_eq!(world.get::<Rect>(last).unwrap().y, 24.0);
}

#[test]
fn hit_test_prefers_the_deepest_rect() {
    let mut world = World::new();
    world.insert_resource(Viewport::default());

    let parent = world.spawn(Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(100.0),
        height: Size::Px(100.0),
    });
    let child = world.spawn(px(40.0, 40.0));
    set_parent(&mut world, child, parent);
    layout_propagate(&mut world);

    assert_eq!(hit_test(&world, 10.0, 10.0), Some(child));
    assert_eq!(hit_test(&world, 80.0, 80.0), Some(parent));
    assert_eq!(hit_test(&world, 200.0, 200.0), None);
}

#[test]
fn later_sibling_wins_hit_test() {
    let mut world = World::new();
    world.insert_resource(Viewport::default());
    let row = world.spawn(Layout {
        axis: Axis::Row,
        gap: -10.0,
        padding: 0.0,
        width: Size::Hug,
        height: Size::Hug,
    });
    let first = world.spawn(px(20.0, 20.0));
    let second = world.spawn(px(20.0, 20.0));
    set_parent(&mut world, first, row);
    set_parent(&mut world, second, row);
    layout_propagate(&mut world);

    // Overlap at x=15 because of negative gap.
    assert_eq!(hit_test(&world, 15.0, 5.0), Some(second));
}

#[test]
fn expand_then_collapse_rewrites_layout() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 400.0,
        height: 400.0,
    });

    let spec = NodeSpec::branch(
        "Root",
        Color::rgb(0.2, 0.3, 0.4),
        vec![
            NodeSpec::leaf("A", Color::rgb(0.3, 0.4, 0.3)),
            NodeSpec::leaf("B", Color::rgb(0.3, 0.4, 0.3)),
        ],
    );
    let root = spawn_node(&mut world, spec);
    layout_propagate(&mut world);
    let closed = world.get::<Rect>(root).unwrap().h;
    assert_eq!(closed, 36.0);

    toggle_disclosure(&mut world, root);
    assert!(world.get::<Disclosure>(root).unwrap().open);
    layout_propagate(&mut world);
    let open = world.get::<Rect>(root).unwrap().h;
    assert!(open > closed);
    assert_eq!(world.get::<Disclosure>(root).unwrap().spawned.len(), 2);

    toggle_disclosure(&mut world, root);
    assert!(!world.get::<Disclosure>(root).unwrap().open);
    layout_propagate(&mut world);
    assert_eq!(world.get::<Rect>(root).unwrap().h, closed);
}

fn fill_column() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Fill,
        height: Size::Fill,
    }
}

#[test]
fn scrollport_offsets_hug_children_and_records_content() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 100.0,
        height: 50.0,
    });

    let port = world.spawn((fill_column(), Scroll::default()));
    let first = world.spawn(px(100.0, 40.0));
    let second = world.spawn(px(100.0, 40.0));
    set_parent(&mut world, first, port);
    set_parent(&mut world, second, port);

    world.get_mut::<Scroll>(port).unwrap().offset = [0.0, -20.0];
    layout_propagate(&mut world);

    let scroll = *world.get::<Scroll>(port).unwrap();
    assert_eq!(scroll.content, [100.0, 80.0]);
    assert_eq!(scroll.offset, [0.0, -20.0]);
    assert_eq!(world.get::<Rect>(port).unwrap().h, 50.0);
    assert_eq!(world.get::<Rect>(first).unwrap().y, -20.0);
    assert_eq!(world.get::<Rect>(second).unwrap().y, 20.0);
}

#[test]
fn overflowed_child_clip_is_intersection_with_port() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 80.0,
        height: 50.0,
    });

    let port = world.spawn((fill_column(), Scroll::default()));
    let child = world.spawn(px(80.0, 80.0));
    set_parent(&mut world, child, port);
    layout_propagate(&mut world);

    let port_rect = *world.get::<Rect>(port).unwrap();
    let child_rect = *world.get::<Rect>(child).unwrap();
    let expected = Clip::from_rect(port_rect).intersect(Clip::from_rect(child_rect));
    assert_eq!(*world.get::<Clip>(child).unwrap(), expected);
    assert_eq!(expected.h, 50.0);
}

#[test]
fn hit_test_ignores_rect_outside_clip() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 80.0,
        height: 40.0,
    });

    let port = world.spawn((fill_column(), Scroll::default()));
    let child = world.spawn(px(80.0, 80.0));
    set_parent(&mut world, child, port);
    world.get_mut::<Scroll>(port).unwrap().offset = [0.0, -50.0];
    layout_propagate(&mut world);

    let child_rect = *world.get::<Rect>(child).unwrap();
    assert!(child_rect.contains(10.0, -10.0));
    assert_eq!(hit_test(&world, 10.0, -10.0), None);
    assert_eq!(hit_test(&world, 10.0, 10.0), Some(child));
}

#[test]
fn wheel_clamps_and_collapse_resets_offset() {
    let mut world = World::new();
    world.insert_resource(Viewport {
        width: 200.0,
        height: 50.0,
    });
    world.insert_resource(Input::default());

    let port = world.spawn((fill_column(), Scroll::default()));
    let spec = NodeSpec::branch(
        "Root",
        Color::rgb(0.2, 0.3, 0.4),
        vec![
            NodeSpec::leaf("A", Color::rgb(0.3, 0.4, 0.3)),
            NodeSpec::leaf("B", Color::rgb(0.3, 0.4, 0.3)),
            NodeSpec::leaf("C", Color::rgb(0.3, 0.4, 0.3)),
        ],
    );
    let node = spawn_node(&mut world, spec);
    set_parent(&mut world, node, port);
    layout_propagate(&mut world);
    assert_eq!(world.get::<Scroll>(port).unwrap().offset[1], 0.0);

    toggle_disclosure(&mut world, node);
    layout_propagate(&mut world);
    let content = world.get::<Scroll>(port).unwrap().content[1];
    assert!(content > 50.0);

    world.resource_mut::<Input>().unwrap().scroll = [0.0, -400.0];
    world.resource_mut::<Input>().unwrap().cursor = [20.0, 20.0];
    apply_scroll(&mut world);
    layout_propagate(&mut world);
    let scrolled = world.get::<Scroll>(port).unwrap().offset[1];
    assert_eq!(scrolled, 50.0 - content);

    toggle_disclosure(&mut world, node);
    layout_propagate(&mut world);
    assert_eq!(world.get::<Scroll>(port).unwrap().offset[1], 0.0);
}
