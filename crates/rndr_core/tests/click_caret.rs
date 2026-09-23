use rndr_core::{
    children_of, cols_for, detail_pane, set_editing_field, set_parent, set_selection, shell,
    spawn_node, tree_column, App, Caret, Color, EditField, Field, Focusable, Input, Key, Label,
    NodeSpec, Pane, Parent, Rect, Schedule, Shell, Viewport, World, GLYPH_PX, TEXT_INSET_X,
    TEXT_INSET_Y,
};

fn tree() -> Vec<NodeSpec> {
    let root = Color::rgb(0.2, 0.3, 0.4);
    let child = Color::rgb(0.3, 0.4, 0.3);
    vec![
        NodeSpec::branch("Alpha", root, vec![NodeSpec::leaf("Alpha / 1", child)]),
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
    app.add_plugin(Shell);
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

fn click_at(app: &mut App, entity: rndr_core::Entity, dx: f32, dy: f32) {
    let rect = *app.world().get::<Rect>(entity).unwrap();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.cursor = [rect.x + dx, rect.y + dy];
    input.pressed = true;
    app.update();
}

fn boot_narrow() -> App {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.world_mut().insert_resource(Viewport {
        width: 200.0,
        height: 400.0,
    });
    {
        let world = app.world_mut();
        let (layout, scroll, pane) = detail_pane();
        world.spawn((layout, scroll, pane, Color::rgb(0.12, 0.13, 0.15)));
        let node = spawn_node(world, NodeSpec::leaf("Alpha", Color::rgb(0.2, 0.3, 0.4)));
        set_selection(world, Some(node));
        set_editing_field(world, node, EditField::Body);
    }
    app.update();
    app.update();
    app
}

#[test]
fn click_title_near_first_glyph_places_caret_at_zero() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert_eq!(
        app.world()
            .get::<Caret>(pane_field(app.world(), EditField::Title))
            .unwrap()
            .index,
        5
    );

    let title = pane_field(app.world(), EditField::Title);
    click_at(&mut app, title, TEXT_INSET_X + 4.0, TEXT_INSET_Y + 4.0);
    assert_eq!(app.world().get::<Caret>(title).unwrap().index, 0);
}

#[test]
fn click_between_title_glyphs_places_caret() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    for _ in 0.."Alpha".len() {
        press(&mut app, Key::Backspace);
    }
    for ch in "abcd".chars() {
        type_char(&mut app, ch);
    }
    assert_eq!(
        app.world()
            .get::<Caret>(pane_field(app.world(), EditField::Title))
            .unwrap()
            .index,
        4
    );

    let title = pane_field(app.world(), EditField::Title);
    click_at(
        &mut app,
        title,
        TEXT_INSET_X + 2.0 * GLYPH_PX,
        TEXT_INSET_Y + 4.0,
    );
    assert_eq!(app.world().get::<Caret>(title).unwrap().index, 2);
}

#[test]
fn click_second_wrapped_body_line() {
    let mut app = boot_narrow();
    for _ in 0..20 {
        type_char(&mut app, 'a');
    }
    let body = pane_field(app.world(), EditField::Body);
    assert_eq!(app.world().get::<Caret>(body).unwrap().index, 20);

    let cols = cols_for(app.world().get::<Rect>(body).unwrap().w);
    click_at(&mut app, body, TEXT_INSET_X, TEXT_INSET_Y + GLYPH_PX + 4.0);
    assert_eq!(app.world().get::<Caret>(body).unwrap().index, cols);
    assert_eq!(cols, 9);
}

#[test]
fn click_tree_header_clears_the_caret() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert!(!app.world().entities_with::<Caret>().is_empty());

    let alpha = node_named(app.world(), "Alpha");
    let header = children_of(app.world(), alpha)
        .into_iter()
        .find(|child| app.world().has::<Label>(*child))
        .unwrap();
    click_at(&mut app, header, 8.0, 8.0);
    assert!(app.world().entities_with::<Caret>().is_empty());
}
