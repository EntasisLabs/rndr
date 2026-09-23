use rndr_core::{
    scroll_page, set_parent, spawn_node, App, Color, Disclosure, Focus, Input, Key, Label,
    NodeSpec, Parent, Schedule, Scroll, Shell, Viewport, World,
};

fn tree() -> Vec<NodeSpec> {
    let root = Color::rgb(0.2, 0.3, 0.4);
    let child = Color::rgb(0.3, 0.4, 0.3);
    vec![
        NodeSpec::branch(
            "Alpha",
            root,
            vec![
                NodeSpec::branch(
                    "Alpha / 1",
                    child,
                    vec![NodeSpec::leaf("Alpha / 1 / a", child)],
                ),
                NodeSpec::leaf("Alpha / 2", child),
            ],
        ),
        NodeSpec::branch("Beta", root, vec![NodeSpec::leaf("Beta / 1", child)]),
        NodeSpec::leaf("Gamma", root),
    ]
}

fn setup_page(world: &mut World) {
    let (layout, scroll) = scroll_page();
    let page = world.spawn((layout, scroll, Color::rgb(0.1, 0.1, 0.1)));
    for spec in tree() {
        let node = spawn_node(world, spec);
        set_parent(world, node, page);
    }
}

fn boot() -> App {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_system(Schedule::Startup, setup_page);
    app.update();
    app
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
}

fn focused_label(world: &World) -> String {
    let entity = world.resource::<Focus>().unwrap().entity.expect("focus");
    for child in rndr_core::children_of(world, entity) {
        if let Some(label) = world.get::<Label>(child) {
            return label.text.clone();
        }
    }
    panic!("focused node has no label");
}

fn node_named(world: &World, name: &str) -> rndr_core::Entity {
    for (header, label) in world.query::<Label>() {
        if label.text == name {
            return world.get::<Parent>(header).unwrap().entity;
        }
    }
    panic!("no node labeled {name}");
}

#[test]
fn down_then_right_opens_alpha() {
    let mut app = boot();
    press(&mut app, Key::Down);
    assert_eq!(focused_label(app.world()), "Alpha");
    press(&mut app, Key::Right);
    let alpha = node_named(app.world(), "Alpha");
    assert!(app.world().get::<Disclosure>(alpha).unwrap().open);
    assert_eq!(focused_label(app.world()), "Alpha");
}

#[test]
fn enter_does_not_toggle_the_tree() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    let alpha = node_named(app.world(), "Alpha");
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
    assert_eq!(focused_label(app.world()), "Alpha");
}

#[test]
fn tree_walk_and_escape() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Right);
    press(&mut app, Key::Down);
    assert_eq!(focused_label(app.world()), "Alpha / 1");

    press(&mut app, Key::Right);
    assert!(
        app.world()
            .get::<Disclosure>(node_named(app.world(), "Alpha / 1"))
            .unwrap()
            .open
    );
    press(&mut app, Key::Escape);
    assert!(
        !app.world()
            .get::<Disclosure>(node_named(app.world(), "Alpha / 1"))
            .unwrap()
            .open
    );
    assert_eq!(focused_label(app.world()), "Alpha / 1");

    press(&mut app, Key::Escape);
    assert_eq!(focused_label(app.world()), "Alpha");
    assert!(!app.world().resource::<Input>().unwrap().quit);

    press(&mut app, Key::Escape);
    assert!(
        !app.world()
            .get::<Disclosure>(node_named(app.world(), "Alpha"))
            .unwrap()
            .open
    );
    press(&mut app, Key::Escape);
    assert!(app.world().resource::<Input>().unwrap().quit);
}

#[test]
fn click_sets_focus_without_opening() {
    let mut app = boot();
    let alpha = node_named(app.world(), "Alpha");
    let header = rndr_core::children_of(app.world(), alpha)
        .into_iter()
        .find(|child| app.world().has::<Label>(*child))
        .unwrap();
    let rect = *app.world().get::<rndr_core::Rect>(header).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.cursor = [rect.x + 8.0, rect.y + 8.0];
        input.pressed = true;
    }
    app.update();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(alpha));
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
}

#[test]
fn tab_walks_without_opening() {
    let mut app = boot();
    press(&mut app, Key::Tab);
    assert_eq!(focused_label(app.world()), "Alpha");
    press(&mut app, Key::Tab);
    assert_eq!(focused_label(app.world()), "Beta");
    assert!(
        !app.world()
            .get::<Disclosure>(node_named(app.world(), "Alpha"))
            .unwrap()
            .open
    );
    assert!(
        !app.world()
            .get::<Disclosure>(node_named(app.world(), "Beta"))
            .unwrap()
            .open
    );
}

#[test]
fn focus_below_port_adjusts_scroll() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.world_mut().insert_resource(Viewport {
        width: 200.0,
        height: 80.0,
    });
    app.add_system(Schedule::Startup, setup_page);
    app.update();

    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    let page = app
        .world()
        .entities_with::<Scroll>()
        .into_iter()
        .next()
        .unwrap();
    assert_ne!(app.world().get::<Scroll>(page).unwrap().offset[1], 0.0);
}
