use rndr_core::{
    children_of, detail_pane, field_height, set_parent, shell, spawn_node, tree_column, App, Body,
    Caret, Color, Disclosure, EditField, Editing, Field, Focus, Focusable, Input, Key, Label,
    NodeSpec, Pane, Parent, Schedule, Selection, World, CELL_H, TEXT_ORIGIN_X, TEXT_ORIGIN_Y,
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

fn pane_field(world: &World, kind: EditField) -> rndr_core::Entity {
    let pane = world
        .entities_with::<Pane>()
        .into_iter()
        .next()
        .expect("pane");
    for entity in world.get::<Pane>(pane).unwrap().spawned.iter().copied() {
        if world
            .get::<Field>(entity)
            .is_some_and(|field| field.kind == kind)
        {
            return entity;
        }
    }
    panic!("no pane field {kind:?}");
}

fn click_rect(app: &mut App, entity: rndr_core::Entity) {
    let rect = *app.world().get::<rndr_core::Rect>(entity).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.cursor = [rect.x + 8.0, rect.y + 8.0];
        input.pressed = true;
    }
    app.update();
}

#[test]
fn enter_on_alpha_starts_on_the_title() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);

    let alpha = node_named(app.world(), "Alpha");
    let editing = *app.world().resource::<Editing>().unwrap();
    assert_eq!(editing.source, Some(alpha));
    assert_eq!(editing.field, EditField::Title);
    assert_eq!(app.world().get::<Body>(alpha).unwrap().text, "");
    let body = pane_field(app.world(), EditField::Body);
    assert!(app.world().has::<Label>(body));
    assert!(app
        .world()
        .has::<Caret>(pane_field(app.world(), EditField::Title)));
    assert!(!app.world().has::<Caret>(body));
}

#[test]
fn tab_moves_the_caret_to_the_body() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);

    assert_eq!(
        app.world().resource::<Editing>().unwrap().field,
        EditField::Body
    );
    assert!(app
        .world()
        .has::<Caret>(pane_field(app.world(), EditField::Body)));
    assert!(!app
        .world()
        .has::<Caret>(pane_field(app.world(), EditField::Title)));
    assert_eq!(focused_label(app.world()), "Alpha");
}

#[test]
fn typing_in_the_body_writes_back() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');

    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(
        app.world()
            .get::<Label>(pane_field(app.world(), EditField::Body))
            .unwrap()
            .text,
        "x"
    );
    assert_eq!(app.world().get::<Body>(alpha).unwrap().text, "x");
}

#[test]
fn enter_in_the_body_inserts_a_newline() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');
    press(&mut app, Key::Enter);

    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(app.world().get::<Body>(alpha).unwrap().text, "x\n");
    let body = pane_field(app.world(), EditField::Body);
    assert_eq!(
        app.world().get::<rndr_core::Layout>(body).unwrap().height,
        rndr_core::Size::Px(field_height("x\n"))
    );
    assert!(!app.world().get::<Disclosure>(alpha).unwrap().open);
}

#[test]
fn down_then_type_starts_the_next_line() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');
    press(&mut app, Key::Enter);
    press(&mut app, Key::Down);
    type_char(&mut app, 'y');

    let alpha = node_named(app.world(), "Alpha");
    assert_eq!(app.world().get::<Body>(alpha).unwrap().text, "x\ny");

    press(&mut app, Key::Up);
    let caret = *app
        .world()
        .get::<Caret>(pane_field(app.world(), EditField::Body))
        .unwrap();
    assert_eq!(caret.index, 1);
}

#[test]
fn click_second_line_places_the_caret_there() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
    type_char(&mut app, 'x');
    press(&mut app, Key::Enter);
    type_char(&mut app, 'y');

    let body = pane_field(app.world(), EditField::Body);
    let rect = *app.world().get::<rndr_core::Rect>(body).unwrap();
    {
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.cursor = [
            rect.x + TEXT_ORIGIN_X + 1.0,
            rect.y + TEXT_ORIGIN_Y + CELL_H + 1.0,
        ];
        input.pressed = true;
    }
    app.update();

    let caret = *app.world().get::<Caret>(body).unwrap();
    assert_eq!(caret.index, 2);
    assert_eq!(caret.anchor, 2);
    assert_eq!(
        app.world()
            .get::<Body>(node_named(app.world(), "Alpha"))
            .unwrap()
            .text,
        "x\ny"
    );
}

#[test]
fn click_body_starts_edit_click_tree_does_not() {
    let mut app = boot();
    let alpha = node_named(app.world(), "Alpha");
    let header = children_of(app.world(), alpha)
        .into_iter()
        .find(|child| app.world().has::<Label>(*child))
        .unwrap();
    click_rect(&mut app, header);
    assert!(app.world().entities_with::<Caret>().is_empty());
    assert_eq!(
        app.world().resource::<Selection>().unwrap().entity,
        Some(alpha)
    );

    let body = pane_field(app.world(), EditField::Body);
    click_rect(&mut app, body);
    assert_eq!(
        app.world().resource::<Editing>().unwrap().field,
        EditField::Body
    );
    assert!(app.world().has::<Caret>(body));

    click_rect(&mut app, header);
    assert!(app.world().entities_with::<Caret>().is_empty());
    assert!(app.world().resource::<Editing>().unwrap().source.is_none());
}

#[test]
fn escape_leaves_the_body_caret() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    press(&mut app, Key::Tab);
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
