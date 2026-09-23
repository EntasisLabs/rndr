use rndr_core::{
    detail_pane, set_parent, shell, spawn_node, splitter, tree_column, App, Axis, Color,
    Disclosure, Drag, Focus, Focusable, Input, Key, Label, Layout, NodeSpec, Parent, Rect, Shell,
    Size, Splitter, Viewport, World,
};

fn px_fill(w: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Fill,
    }
}

fn fill_fill() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Fill,
        height: Size::Fill,
    }
}

fn boot_row() -> (App, rndr_core::Entity, rndr_core::Entity, rndr_core::Entity) {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.world_mut().insert_resource(Viewport {
        width: 800.0,
        height: 400.0,
    });
    let (leading, seam, trailing) = {
        let world = app.world_mut();
        let parent = world.spawn((shell(), Color::rgb(0.1, 0.1, 0.1)));
        let leading = world.spawn((px_fill(280.0), Color::rgb(0.2, 0.2, 0.2)));
        set_parent(world, leading, parent);
        let (split_layout, split) = splitter();
        let seam = world.spawn((split_layout, split, Color::rgb(0.3, 0.3, 0.3)));
        set_parent(world, seam, parent);
        let trailing = world.spawn((fill_fill(), Color::rgb(0.4, 0.4, 0.4)));
        set_parent(world, trailing, parent);
        (leading, seam, trailing)
    };
    app.update();
    (app, leading, seam, trailing)
}

fn press_at(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.cursor = [x, y];
    input.pressed = true;
    input.down = true;
    app.update();
}

fn drag_to(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.cursor = [x, y];
    input.pressed = false;
    input.down = true;
    app.update();
}

fn release_at(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.cursor = [x, y];
    input.pressed = false;
    input.down = false;
    app.update();
}

fn seam_center(world: &World, seam: rndr_core::Entity) -> [f32; 2] {
    let rect = *world.get::<Rect>(seam).unwrap();
    [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0]
}

#[test]
fn drag_grows_leading_and_shrinks_trailing() {
    let (mut app, leading, seam, trailing) = boot_row();
    let start = *app.world().get::<Rect>(leading).unwrap();
    let trail = *app.world().get::<Rect>(trailing).unwrap();
    let [x, y] = seam_center(app.world(), seam);

    press_at(&mut app, x, y);
    assert_eq!(app.world().resource::<Drag>().unwrap().entity, Some(seam));
    drag_to(&mut app, x + 40.0, y);

    let after = *app.world().get::<Rect>(leading).unwrap();
    let trail_after = *app.world().get::<Rect>(trailing).unwrap();
    assert_eq!(after.w, start.w + 40.0);
    assert_eq!(trail_after.w, trail.w - 40.0);
}

#[test]
fn drag_clamps_to_min_leading_and_trailing() {
    let (mut app, leading, seam, _trailing) = boot_row();
    let [x, y] = seam_center(app.world(), seam);

    press_at(&mut app, x, y);
    drag_to(&mut app, 2000.0, y);
    assert_eq!(app.world().get::<Rect>(leading).unwrap().w, 632.0);

    drag_to(&mut app, -1000.0, y);
    assert_eq!(app.world().get::<Rect>(leading).unwrap().w, 120.0);
}

#[test]
fn release_stops_the_drag() {
    let (mut app, leading, seam, _trailing) = boot_row();
    let [x, y] = seam_center(app.world(), seam);

    press_at(&mut app, x, y);
    drag_to(&mut app, x + 40.0, y);
    let held = app.world().get::<Rect>(leading).unwrap().w;
    release_at(&mut app, x + 40.0, y);
    assert!(app.world().resource::<Drag>().unwrap().entity.is_none());
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.cursor = [x + 80.0, y];
    input.down = false;
    app.update();
    assert_eq!(app.world().get::<Rect>(leading).unwrap().w, held);
}

#[test]
fn press_on_seam_does_not_focus_or_toggle() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_system(rndr_core::Schedule::Startup, |world: &mut World| {
        let root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));
        let (tree_layout, tree_scroll) = tree_column();
        let column = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
        set_parent(world, column, root);
        let node = spawn_node(
            world,
            NodeSpec::branch(
                "Alpha",
                Color::rgb(0.2, 0.3, 0.4),
                vec![NodeSpec::leaf("Alpha / 1", Color::rgb(0.3, 0.4, 0.3))],
            ),
        );
        set_parent(world, node, column);
        let (split_layout, split) = splitter();
        let seam = world.spawn((split_layout, split, Color::rgb(0.06, 0.07, 0.08)));
        set_parent(world, seam, root);
        let (pane_layout, pane_scroll, pane) = detail_pane();
        let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
        set_parent(world, detail, root);
    });
    app.update();

    app.world_mut().resource_mut::<Input>().unwrap().key = Some(Key::Down);
    app.update();
    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(alpha));
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);

    let seam = app
        .world()
        .entities_with::<Splitter>()
        .into_iter()
        .next()
        .expect("seam");
    let [x, y] = seam_center(app.world(), seam);
    press_at(&mut app, x, y);

    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(alpha));
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
}

#[test]
fn column_splitter_changes_leading_height() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.world_mut().insert_resource(Viewport {
        width: 400.0,
        height: 600.0,
    });
    let (leading, seam) = {
        let world = app.world_mut();
        let parent = world.spawn((
            Layout {
                axis: Axis::Column,
                gap: 0.0,
                padding: 0.0,
                width: Size::Fill,
                height: Size::Fill,
            },
            Color::rgb(0.1, 0.1, 0.1),
        ));
        let leading = world.spawn((
            Layout {
                axis: Axis::Column,
                gap: 0.0,
                padding: 0.0,
                width: Size::Fill,
                height: Size::Px(200.0),
            },
            Color::rgb(0.2, 0.2, 0.2),
        ));
        set_parent(world, leading, parent);
        let seam = world.spawn((
            Layout {
                axis: Axis::Row,
                gap: 0.0,
                padding: 0.0,
                width: Size::Fill,
                height: Size::Px(8.0),
            },
            Splitter::default(),
            Color::rgb(0.3, 0.3, 0.3),
        ));
        set_parent(world, seam, parent);
        let trailing = world.spawn((fill_fill(), Color::rgb(0.4, 0.4, 0.4)));
        set_parent(world, trailing, parent);
        (leading, seam)
    };
    app.update();

    let start = app.world().get::<Rect>(leading).unwrap().h;
    let [x, y] = seam_center(app.world(), seam);
    press_at(&mut app, x, y);
    drag_to(&mut app, x, y + 30.0);
    assert_eq!(app.world().get::<Rect>(leading).unwrap().h, start + 30.0);
}

fn node_named(world: &World, name: &str) -> rndr_core::Entity {
    for (header, label) in world.query::<Label>() {
        if label.text != name {
            continue;
        }
        let Some(parent) = world.get::<Parent>(header) else {
            continue;
        };
        if world.has::<Focusable>(parent.entity) {
            return parent.entity;
        }
    }
    panic!("no tree node labeled {name}");
}
