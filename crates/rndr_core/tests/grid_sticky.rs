use rndr_core::{
    apply_scroll, hit_test, layout_propagate, paint_order, set_parent, Align, Axis, Entity, Grid,
    GridSpan, Input, Items, Layout, Rect, Scroll, Size, Sticky, Track, Viewport, World,
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
    world.insert_resource(Input::default());
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

fn wheel(world: &mut World, at: [f32; 2], delta: [f32; 2], shift: bool) {
    let input = world.resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.scroll = delta;
    input.shift = shift;
    apply_scroll(world);
}

#[test]
fn grid_places_cells_row_by_row_with_spans() {
    let mut world = world();
    let grid = world.spawn(boxed(Size::Px(320.0), Size::Hug, Axis::Column));
    world.insert(
        grid,
        Grid {
            columns: vec![Track::Px(60.0), Track::Fr(1.0), Track::Fr(1.0)],
            column_gap: 10.0,
            row_gap: 5.0,
        },
    );
    let a = child(
        &mut world,
        grid,
        boxed(Size::Fill, Size::Px(20.0), Axis::Column),
    );
    let b = child(
        &mut world,
        grid,
        boxed(Size::Fill, Size::Px(30.0), Axis::Column),
    );
    let c = child(&mut world, grid, fixed(40.0, 10.0));
    let wide = child(
        &mut world,
        grid,
        boxed(Size::Fill, Size::Px(10.0), Axis::Column),
    );
    world.insert(wide, GridSpan(2));
    let full = child(
        &mut world,
        grid,
        boxed(Size::Fill, Size::Px(10.0), Axis::Column),
    );
    world.insert(full, GridSpan(3));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, a), [0.0, 0.0, 60.0, 20.0]);
    assert_eq!(rect(&world, b), [70.0, 0.0, 120.0, 30.0]);
    assert_eq!(rect(&world, c), [200.0, 0.0, 40.0, 10.0]);
    assert_eq!(rect(&world, wide), [0.0, 35.0, 190.0, 10.0]);
    assert_eq!(rect(&world, full), [0.0, 50.0, 320.0, 10.0]);
    assert_eq!(rect(&world, grid)[3], 60.0);
}

#[test]
fn grid_auto_columns_hug_and_items_align_in_rows() {
    let mut world = world();
    let grid = world.spawn(boxed(Size::Hug, Size::Hug, Axis::Column));
    world.insert(
        grid,
        Grid {
            columns: vec![Track::Auto, Track::Auto],
            column_gap: 4.0,
            row_gap: 0.0,
        },
    );
    world.insert(
        grid,
        Align {
            items: Items::Center,
            ..Align::default()
        },
    );
    let a = child(&mut world, grid, fixed(30.0, 10.0));
    let b = child(&mut world, grid, fixed(50.0, 30.0));
    let c = child(&mut world, grid, fixed(40.0, 10.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, grid), [0.0, 0.0, 94.0, 40.0]);
    assert_eq!(rect(&world, a), [0.0, 10.0, 30.0, 10.0]);
    assert_eq!(rect(&world, b), [44.0, 0.0, 50.0, 30.0]);
    assert_eq!(rect(&world, c), [0.0, 30.0, 40.0, 10.0]);
}

#[test]
fn equal_grid_scrolls_and_stretches_rows() {
    let mut world = world();
    let port = world.spawn((fixed(200.0, 100.0), Scroll::default()));
    world.insert(port, Grid::equal(2, 0.0));
    world.insert(
        port,
        Align {
            items: Items::Stretch,
            ..Align::default()
        },
    );
    let tiles: Vec<Entity> = (0..6)
        .map(|i| {
            child(
                &mut world,
                port,
                boxed(
                    Size::Fill,
                    Size::Px(if i == 0 { 50.0 } else { 40.0 }),
                    Axis::Column,
                ),
            )
        })
        .collect();
    let hug = child(&mut world, port, boxed(Size::Fill, Size::Hug, Axis::Column));
    layout_propagate(&mut world);
    assert_eq!(world.get::<Scroll>(port).unwrap().content, [200.0, 130.0]);
    assert_eq!(rect(&world, tiles[1])[3], 40.0);
    assert_eq!(rect(&world, hug)[..2], [0.0, 130.0]);
    wheel(&mut world, [50.0, 50.0], [0.0, -20.0], false);
    assert_eq!(rect(&world, tiles[2])[1], 30.0);
}

fn sectioned_list(world: &mut World) -> (Entity, [Entity; 2], [Entity; 2]) {
    let port = world.spawn((fixed(200.0, 100.0), Scroll::default()));
    let mut headers = [port; 2];
    let mut sections = [port; 2];
    for i in 0..2 {
        let section = child(world, port, boxed(Size::Fill, Size::Hug, Axis::Column));
        let header = child(
            world,
            section,
            boxed(Size::Fill, Size::Px(20.0), Axis::Column),
        );
        world.insert(header, Sticky::top(0.0));
        for _ in 0..4 {
            child(
                world,
                section,
                boxed(Size::Fill, Size::Px(30.0), Axis::Column),
            );
        }
        headers[i] = header;
        sections[i] = section;
    }
    (port, headers, sections)
}

#[test]
fn sticky_header_pins_then_is_pushed_by_its_section() {
    let mut world = world();
    let (port, [first, second], [section, _]) = sectioned_list(&mut world);
    layout_propagate(&mut world);
    assert_eq!(rect(&world, first)[1], 0.0);
    assert_eq!(rect(&world, second)[1], 140.0);
    assert_eq!(world.get::<Scroll>(port).unwrap().content[1], 280.0);

    wheel(&mut world, [50.0, 50.0], [0.0, -50.0], false);
    assert_eq!(rect(&world, first)[1], 0.0);
    assert_eq!(rect(&world, section)[1], -50.0);
    assert_eq!(hit_test(&world, 50.0, 10.0), Some(first));
    assert_eq!(*paint_order(&world, section).last().unwrap(), first);

    wheel(&mut world, [50.0, 50.0], [0.0, -80.0], false);
    assert_eq!(rect(&world, first)[1], -10.0);
    assert_eq!(rect(&world, second)[1], 10.0);

    wheel(&mut world, [50.0, 50.0], [0.0, -60.0], false);
    assert_eq!(rect(&world, second)[1], 0.0);
}

#[test]
fn sticky_bottom_and_no_vertical_port_are_respected() {
    let mut world = world();
    let port = world.spawn((fixed(200.0, 100.0), Scroll::default()));
    child(
        &mut world,
        port,
        boxed(Size::Fill, Size::Px(150.0), Axis::Column),
    );
    let footer = child(
        &mut world,
        port,
        boxed(Size::Fill, Size::Px(20.0), Axis::Column),
    );
    world.insert(footer, Sticky::bottom(0.0));
    let still = world.spawn(fixed(100.0, 50.0));
    let free = child(&mut world, still, fixed(10.0, 10.0));
    world.insert(free, Sticky::top(30.0));
    layout_propagate(&mut world);
    assert_eq!(rect(&world, footer)[1], 80.0);
    assert_eq!(rect(&world, free)[1], 0.0);
    wheel(&mut world, [50.0, 50.0], [0.0, -70.0], false);
    assert_eq!(rect(&world, footer)[1], 80.0);
}

#[test]
fn horizontal_strip_scrolls_on_x_and_hands_vertical_to_page() {
    let mut world = world();
    let page = world.spawn((fixed(200.0, 100.0), Scroll::default()));
    let strip = child(
        &mut world,
        page,
        boxed(Size::Fill, Size::Px(40.0), Axis::Row),
    );
    world.insert(strip, Scroll::default());
    for _ in 0..5 {
        child(&mut world, strip, fixed(100.0, 40.0));
    }
    child(
        &mut world,
        page,
        boxed(Size::Fill, Size::Px(200.0), Axis::Column),
    );
    layout_propagate(&mut world);
    assert_eq!(world.get::<Scroll>(strip).unwrap().content[0], 500.0);

    wheel(&mut world, [50.0, 20.0], [-30.0, 0.0], false);
    assert_eq!(world.get::<Scroll>(strip).unwrap().offset, [-30.0, 0.0]);
    assert_eq!(world.get::<Scroll>(page).unwrap().offset, [0.0, 0.0]);

    wheel(&mut world, [50.0, 20.0], [0.0, -25.0], true);
    assert_eq!(world.get::<Scroll>(strip).unwrap().offset[0], -55.0);

    wheel(&mut world, [50.0, 20.0], [-1000.0, 0.0], false);
    assert_eq!(world.get::<Scroll>(strip).unwrap().offset[0], -300.0);

    wheel(&mut world, [50.0, 20.0], [0.0, -40.0], false);
    assert_eq!(world.get::<Scroll>(strip).unwrap().offset[0], -300.0);
    assert_eq!(world.get::<Scroll>(page).unwrap().offset[1], -40.0);
}
