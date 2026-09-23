use rndr_core::{
    children_of, detail_pane, set_parent, shell, spawn_node, tree_column, App, Caret, Color,
    Disclosure, Focus, Focusable, Input, Key, Label, NodeSpec, Pane, Parent, Schedule, Selection,
    World,
};

fn tree() -> Vec<NodeSpec> {
    let root = Color::rgb(0.2, 0.3, 0.4);
    let child = Color::rgb(0.3, 0.4, 0.3);
    vec![
        NodeSpec::branch(
            "Alpha",
            root,
            vec![
                NodeSpec::leaf("Alpha / 1", child),
                NodeSpec::leaf("Alpha / 2", child),
            ],
        ),
        NodeSpec::branch("Beta", root, vec![NodeSpec::leaf("Beta / 1", child)]),
    ]
}

fn setup_shell(world: &mut World) {
    let root = world.spawn((shell(), Color::rgb(0.08, 0.09, 0.10)));

    let (tree_layout, tree_scroll) = tree_column();
    let column = world.spawn((tree_layout, tree_scroll, Color::rgb(0.10, 0.11, 0.13)));
    set_parent(world, column, root);
    for spec in tree() {
        let node = spawn_node(world, spec);
        set_parent(world, node, column);
    }

    let (pane_layout, pane_scroll, pane) = detail_pane();
    let detail = world.spawn((pane_layout, pane_scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
    set_parent(world, detail, root);
}

fn boot() -> App {
    let mut app = App::new();
    app.add_system(Schedule::Startup, setup_shell);
    app.update();
    app
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
}

fn focused_label(world: &World) -> String {
    let entity = world.resource::<Focus>().unwrap().entity.expect("focus");
    for child in children_of(world, entity) {
        if let Some(label) = world.get::<Label>(child) {
            return label.text.clone();
        }
    }
    panic!("focused node has no label");
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

fn pane_labels(world: &World) -> Vec<String> {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    world
        .get::<Pane>(pane)
        .unwrap()
        .spawned
        .iter()
        .filter_map(|entity| world.get::<Label>(*entity).map(|label| label.text.clone()))
        .collect()
}

#[test]
fn enter_on_alpha_fills_the_pane() {
    let mut app = boot();
    assert_eq!(pane_labels(app.world()), vec!["Nothing open".to_string()]);

    press(&mut app, Key::Down);
    assert_eq!(focused_label(app.world()), "Alpha");
    press(&mut app, Key::Enter);

    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(alpha)
    );
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
    let labels = pane_labels(app.world());
    assert!(labels.contains(&"Alpha".to_string()));
    assert!(labels.contains(&"Closed".to_string()));
    assert!(labels.contains(&"Alpha / 1".to_string()));
    assert!(!app.world().has::<Focusable>(
        app.world()
            .entities_with::<Pane>()
            .into_iter()
            .next()
            .unwrap()
    ));
}

#[test]
fn enter_on_beta_replaces_the_pane_body() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert!(pane_labels(app.world()).contains(&"Alpha".to_string()));

    press(&mut app, Key::Escape);
    press(&mut app, Key::Down);
    assert_eq!(focused_label(app.world()), "Beta");
    press(&mut app, Key::Enter);

    let beta = node_named(app.world(), "Beta");
    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(beta)
    );
    let labels = pane_labels(app.world());
    assert!(labels.contains(&"Beta".to_string()));
    assert!(!labels.contains(&"Alpha".to_string()));
    assert!(!labels.contains(&"Nothing open".to_string()));
}

#[test]
fn escape_clears_selection_and_keeps_tree_focus() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert_eq!(focused_label(app.world()), "Beta");
    assert!(app
        .world()
        .resource::<Selection>()
        .unwrap()
        .entity
        .is_some());

    press(&mut app, Key::Escape);
    assert!(app
        .world()
        .resource::<Selection>()
        .unwrap()
        .entity
        .is_some());
    press(&mut app, Key::Escape);
    assert!(app
        .world()
        .resource::<Selection>()
        .unwrap()
        .entity
        .is_none());
    assert_eq!(pane_labels(app.world()), vec!["Nothing open".to_string()]);
    assert_eq!(focused_label(app.world()), "Beta");
    assert!(!app.world().resource::<Input>().unwrap().quit);
}

#[test]
fn click_selects_without_opening_and_fills_the_pane() {
    let mut app = boot();
    let alpha = node_named(app.world(), "Alpha");
    let header = children_of(app.world(), alpha)
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
    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(alpha)
    );
    let labels = pane_labels(app.world());
    assert!(labels.contains(&"Alpha".to_string()));
    assert!(labels.contains(&"Closed".to_string()));
    assert!(app.world().entities_with::<Caret>().is_empty());
}
