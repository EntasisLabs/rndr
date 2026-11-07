use rndr_core::{
    hit_test, layout_propagate, paint_order, set_parent, Absolute, Align, AlignSelf, AspectRatio,
    Axis, Entity, Items, Justify, Layout, MaxSize, Padding, Rect, Scroll, Size, Viewport, World,
    Wrap,
};

fn boxed(w: Size, h: Size, axis: Axis) -> Layout {
    Layout {
        axis,
        gap: 0.0,
        padding: 0.0,
        width: w,
        height: h,
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn fixed(w: f32, h: f32) -> Layout {
    boxed(Size::Px(w), Size::Px(h), Axis::Column)
}

fn world() -> World {
    let mut world = World::new();
    world.insert_resource(Viewport::new(400.0, 300.0));
    world
}

fn child(world: &mut World, parent: Entity, layout: Layout) -> Entity {
    let e = world.spawn(layout);
    set_parent(world, e, parent);
    e
}

fn rect(world: &World, e: Entity) -> [f32; 4] {
    let r = world.get::<Rect>(e).copied().unwrap();
    [r.x, r.y, r.w, r.h]
}

fn row_of_three(world: &mut World, align: Align) -> (Entity, [Entity; 3]) {
    let row = world.spawn(boxed(Size::Px(200.0), Size::Px(50.0), Axis::Row));
    world.insert(row, align);
    let a = child(world, row, fixed(20.0, 10.0));
    let b = child(world, row, fixed(20.0, 20.0));
    let c = child(world, row, fixed(20.0, 30.0));
    (row, [a, b, c])
}

#[test]
fn justify_distributes_leftover_space() {
    let cases = [
        (Justify::Start, [0.0, 20.0, 40.0]),
        (Justify::Center, [70.0, 90.0, 110.0]),
        (Justify::End, [140.0, 160.0, 180.0]),
        (Justify::SpaceBetween, [0.0, 90.0, 180.0]),
        (Justify::SpaceAround, [23.0, 90.0, 157.0]),
        (Justify::SpaceEvenly, [35.0, 90.0, 145.0]),
    ];
    for (justify, xs) in cases {
        let mut world = world();
        let (_, kids) = row_of_three(
            &mut world,
            Align {
                justify,
                items: Items::Start,
            },
        );
        layout_propagate(&mut world);
        for (kid, x) in kids.iter().zip(xs) {
            let got = rect(&world, *kid)[0];
            assert!((got - x).abs() <= 1.0, "{justify:?}: {got} vs {x}");
        }
    }
}

#[test]
fn items_align_across_and_align_self_overrides() {
    let mut world = world();
    let (_, [a, b, c]) = row_of_three(
        &mut world,
        Align {
            justify: Justify::Start,
            items: Items::Center,
        },
    );
    world.insert(c, AlignSelf(Items::End));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, a)[1], 20.0);
    assert_eq!(rect(&world, b)[1], 15.0);
    assert_eq!(rect(&world, c)[1], 20.0);
}

#[test]
fn stretch_fills_cross_axis_unless_fixed() {
    let mut world = world();
    let column = world.spawn(boxed(Size::Px(200.0), Size::Hug, Axis::Column));
    world.insert(
        column,
        Align {
            justify: Justify::Start,
            items: Items::Stretch,
        },
    );
    let hug = child(
        &mut world,
        column,
        boxed(Size::Hug, Size::Px(10.0), Axis::Row),
    );
    let px = child(&mut world, column, fixed(30.0, 10.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, hug), [0.0, 0.0, 200.0, 10.0]);
    assert_eq!(rect(&world, px)[2], 30.0);
    assert_eq!(world.get::<Layout>(hug).unwrap().width, Size::Hug);
}

#[test]
fn justify_ignored_when_fill_takes_leftover() {
    let mut world = world();
    let (row, [a, b, _]) = row_of_three(&mut world, Align::center());
    world.insert(b, boxed(Size::Fill, Size::Px(20.0), Axis::Column));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, a)[0], 0.0);
    assert_eq!(rect(&world, b)[2], 160.0);
    assert_eq!(rect(&world, row)[2], 200.0);
}

#[test]
fn per_side_padding_insets_content_and_hug() {
    let mut world = world();
    let column = world.spawn(boxed(Size::Hug, Size::Hug, Axis::Column));
    world.insert(
        column,
        Padding {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        },
    );
    let kid = child(&mut world, column, fixed(10.0, 10.0));
    let fill = child(
        &mut world,
        column,
        boxed(Size::Fill, Size::Px(5.0), Axis::Row),
    );
    layout_propagate(&mut world);
    assert_eq!(rect(&world, column), [0.0, 0.0, 16.0, 19.0]);
    assert_eq!(rect(&world, kid), [4.0, 1.0, 10.0, 10.0]);
    assert_eq!(rect(&world, fill), [4.0, 11.0, 10.0, 5.0]);
}

#[test]
fn max_size_caps_fill_and_min_wins() {
    let mut world = world();
    let root = world.spawn(boxed(Size::Fill, Size::Fill, Axis::Column));
    let capped = child(
        &mut world,
        root,
        boxed(Size::Fill, Size::Px(10.0), Axis::Row),
    );
    world.insert(capped, MaxSize::width(120.0));
    let mut floor = boxed(Size::Fill, Size::Px(10.0), Axis::Row);
    floor.min_width = 150.0;
    let floored = child(&mut world, root, floor);
    world.insert(floored, MaxSize::width(100.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, capped)[2], 120.0);
    assert_eq!(rect(&world, floored)[2], 150.0);
}

#[test]
fn max_size_wraps_row_and_hug_parent_grows() {
    let mut world = world();
    let column = world.spawn(boxed(Size::Px(300.0), Size::Hug, Axis::Column));
    let mut row_layout = boxed(Size::Fill, Size::Hug, Axis::Row);
    row_layout.gap = 10.0;
    let row = child(&mut world, column, row_layout);
    world.insert(row, Wrap { line_gap: 5.0 });
    world.insert(row, MaxSize::width(100.0));
    let tags: Vec<Entity> = (0..5)
        .map(|_| child(&mut world, row, fixed(40.0, 20.0)))
        .collect();
    let after = child(&mut world, column, fixed(10.0, 10.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, row)[2], 100.0);
    assert_eq!(rect(&world, row)[3], 70.0);
    assert_eq!(rect(&world, tags[1])[..2], [50.0, 0.0]);
    assert_eq!(rect(&world, tags[2])[..2], [0.0, 25.0]);
    assert_eq!(rect(&world, tags[4])[..2], [0.0, 50.0]);
    assert_eq!(rect(&world, after)[1], 70.0);
}

#[test]
fn wrap_justifies_and_aligns_per_line() {
    let mut world = world();
    let row = world.spawn(boxed(Size::Px(100.0), Size::Hug, Axis::Row));
    world.insert(row, Wrap { line_gap: 0.0 });
    world.insert(row, Align::center());
    let a = child(&mut world, row, fixed(40.0, 10.0));
    let b = child(&mut world, row, fixed(40.0, 20.0));
    let c = child(&mut world, row, fixed(40.0, 20.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, a), [10.0, 5.0, 40.0, 10.0]);
    assert_eq!(rect(&world, b), [50.0, 0.0, 40.0, 20.0]);
    assert_eq!(rect(&world, c), [30.0, 20.0, 40.0, 20.0]);
    assert_eq!(rect(&world, row)[3], 40.0);
}

#[test]
fn aspect_ratio_derives_the_free_axis() {
    let mut world = world();
    let column = world.spawn(boxed(Size::Px(160.0), Size::Hug, Axis::Column));
    let video = child(
        &mut world,
        column,
        boxed(Size::Fill, Size::Hug, Axis::Column),
    );
    world.insert(video, AspectRatio(16.0 / 9.0));
    let square = child(
        &mut world,
        column,
        boxed(Size::Hug, Size::Px(30.0), Axis::Column),
    );
    world.insert(square, AspectRatio(1.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, video), [0.0, 0.0, 160.0, 90.0]);
    assert_eq!(rect(&world, square), [0.0, 90.0, 30.0, 30.0]);
    assert_eq!(rect(&world, column)[3], 120.0);
}

#[test]
fn absolute_children_leave_flow_and_pin_to_edges() {
    let mut world = world();
    let card = world.spawn(boxed(Size::Px(200.0), Size::Hug, Axis::Column));
    world.insert(card, Padding::all(10.0));
    let badge = child(&mut world, card, fixed(16.0, 16.0));
    world.insert(
        badge,
        Absolute {
            top: Some(-4.0),
            right: Some(-4.0),
            ..Absolute::default()
        },
    );
    let body = child(&mut world, card, fixed(50.0, 40.0));
    let scrim = child(&mut world, card, boxed(Size::Hug, Size::Hug, Axis::Column));
    world.insert(scrim, Absolute::fill(0.0));
    let bottom = child(&mut world, card, fixed(20.0, 10.0));
    world.insert(
        bottom,
        Absolute {
            bottom: Some(5.0),
            ..Absolute::default()
        },
    );
    layout_propagate(&mut world);
    assert_eq!(rect(&world, card), [0.0, 0.0, 200.0, 60.0]);
    assert_eq!(rect(&world, body), [10.0, 10.0, 50.0, 40.0]);
    assert_eq!(rect(&world, badge), [188.0, -4.0, 16.0, 16.0]);
    assert_eq!(rect(&world, scrim), [0.0, 0.0, 200.0, 60.0]);
    assert_eq!(rect(&world, bottom), [10.0, 45.0, 20.0, 10.0]);
    assert_eq!(world.get::<Layout>(scrim).unwrap().width, Size::Hug);
    assert_eq!(paint_order(&world, card), vec![body, badge, scrim, bottom]);
}

#[test]
fn absolute_child_hit_tests_above_later_siblings() {
    let mut world = world();
    let root = world.spawn(fixed(200.0, 100.0));
    let overlay = child(&mut world, root, fixed(50.0, 50.0));
    world.insert(
        overlay,
        Absolute {
            left: Some(0.0),
            top: Some(0.0),
            ..Absolute::default()
        },
    );
    let under = child(&mut world, root, fixed(100.0, 100.0));
    layout_propagate(&mut world);
    assert_eq!(hit_test(&world, 10.0, 10.0), Some(overlay));
    assert_eq!(hit_test(&world, 80.0, 10.0), Some(under));
}

#[test]
fn absolute_child_scrolls_with_content() {
    let mut world = world();
    let port = world.spawn(fixed(100.0, 100.0));
    world.insert(
        port,
        Scroll {
            offset: [0.0, -30.0],
            content: [0.0, 0.0],
        },
    );
    child(&mut world, port, fixed(100.0, 300.0));
    let marker = child(&mut world, port, fixed(10.0, 10.0));
    world.insert(
        marker,
        Absolute {
            left: Some(0.0),
            top: Some(50.0),
            ..Absolute::default()
        },
    );
    layout_propagate(&mut world);
    assert_eq!(rect(&world, marker)[1], 20.0);
    assert_eq!(world.get::<Scroll>(port).unwrap().content[1], 300.0);
}
