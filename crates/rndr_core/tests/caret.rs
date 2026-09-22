use rndr_core::{
    children_of, detail_pane, set_parent, shell, spawn_node, tree_column, App, Caret, Color,
    Disclosure, Editing, Focus, Focusable, Input, Key, Label, NodeSpec, Pane, Parent, Schedule,
    Selection, World,
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

fn type_char(app: &mut App, ch: char) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .chars
        .push(ch);
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

fn pane_title(world: &World) -> String {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    let title = world
        .get::<Pane>(pane)
        .unwrap()
        .spawned
        .first()
        .copied()
        .expect("title");
    world.get::<Label>(title).unwrap().text.clone()
}

fn title_caret(world: &World) -> Caret {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    let title = world
        .get::<Pane>(pane)
        .unwrap()
        .spawned
        .first()
        .copied()
        .expect("title");
    *world.get::<Caret>(title).expect("caret")
}

fn tree_header_text(world: &World, name: &str) -> String {
    let node = node_named(world, name);
    for child in children_of(world, node) {
        if let Some(label) = world.get::<Label>(child) {
            return label.text.clone();
        }
    }
    panic!("tree node {name} has no header");
}

#[test]
fn enter_on_alpha_starts_edit() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);

    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(
        app.world().resource::<Editing>().unwrap().source,
        Some(alpha)
    );
    assert_eq!(pane_title(app.world()), "Alpha");
    assert_eq!(title_caret(app.world()).index, 5);
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
    assert_eq!(focused_label(app.world()), "Alpha");
}

#[test]
fn typing_writes_back_to_the_tree() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    type_char(&mut app, 'x');

    assert_eq!(pane_title(app.world()), "Alphax");
    assert_eq!(tree_header_text(app.world(), "Alphax"), "Alphax");
    assert_eq!(title_caret(app.world()).index, 6);

    press(&mut app, Key::Backspace);
    assert_eq!(pane_title(app.world()), "Alpha");
    assert_eq!(tree_header_text(app.world(), "Alpha"), "Alpha");
    assert_eq!(title_caret(app.world()).index, 5);
}

#[test]
fn left_then_type_inserts_in_the_middle() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Left);
    press(&mut app, Key::Left);
    type_char(&mut app, 'x');

    assert_eq!(pane_title(app.world()), "Alpxha");
    assert_eq!(tree_header_text(app.world(), "Alpxha"), "Alpxha");
    assert_eq!(title_caret(app.world()).index, 4);
}

#[test]
fn escape_leaves_the_caret_and_keeps_selection() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Escape);

    assert!(app.world().resource::<Editing>().unwrap().source.is_none());
    assert!(app.world().entities_with::<Caret>().is_empty());
    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(alpha)
    );
    assert_eq!(focused_label(app.world()), "Alpha");
}

#[test]
fn left_while_editing_does_not_open_alpha() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Left);

    let alpha = node_named(app.world(), "Alpha");
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
    assert_eq!(title_caret(app.world()).index, 4);
}

#[test]
fn click_fills_the_pane_without_a_caret() {
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

    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(alpha)
    );
    assert_eq!(pane_title(app.world()), "Alpha");
    assert!(app.world().entities_with::<Caret>().is_empty());
    assert!(app.world().resource::<Editing>().unwrap().source.is_none());
}
