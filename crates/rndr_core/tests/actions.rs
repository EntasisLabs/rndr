use rndr_core::{
    detail_pane, set_parent, shell, spawn_node, tree_column, Actions, App, Binding, Caret, Chord,
    Chrome, Color, Fired, Focus, Focusable, Input, Key, NodeSpec, Pane, Schedule, Shell, World,
};

fn tree() -> Vec<NodeSpec> {
    let root = Color::rgb(0.2, 0.3, 0.4);
    let child = Color::rgb(0.3, 0.4, 0.3);
    vec![NodeSpec::branch(
        "Alpha",
        root,
        vec![NodeSpec::leaf("Alpha / 1", child)],
    )]
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
    app.add_plugin(Chrome);
    app.add_system(Schedule::Startup, setup_shell);
    app.update();
    app
}

fn press(app: &mut App, key: Key) {
    app.world_mut().resource_mut::<Input>().unwrap().key = Some(key);
    app.update();
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

fn bind(app: &mut App, binding: Binding) {
    app.world_mut()
        .resource_mut::<Actions>()
        .unwrap()
        .bindings
        .push(binding);
}

#[test]
fn focused_binding_beats_global() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let entity = app.world_mut().spawn(Focusable);
    app.world_mut().insert_resource(Focus {
        entity: Some(entity),
    });
    bind(&mut app, Binding::new("global", Chord::new(Key::Left)));
    bind(
        &mut app,
        Binding::new("scoped", Chord::new(Key::Left)).scope(entity),
    );

    press(&mut app, Key::Left);

    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("scoped")
    );
}

#[test]
fn consumed_chord_does_not_move_the_caret() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    assert_eq!(title_caret(app.world()).index, 5);

    bind(
        &mut app,
        Binding::new("nudge", Chord::new(Key::Left)).consume(),
    );
    press(&mut app, Key::Left);

    assert_eq!(title_caret(app.world()).index, 5);
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("nudge")
    );
    assert!(app.world().resource::<Input>().unwrap().key.is_none());
}

#[test]
fn unbound_key_still_reaches_the_caret() {
    let mut app = boot();
    press(&mut app, Key::Down);
    press(&mut app, Key::Enter);
    bind(
        &mut app,
        Binding::new("other", Chord::new(Key::Up)).consume(),
    );

    press(&mut app, Key::Left);

    assert_eq!(title_caret(app.world()).index, 4);
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

#[test]
fn non_consuming_binding_leaves_the_key() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    bind(&mut app, Binding::new("save", Chord::new(Key::Save)));
    press(&mut app, Key::Save);

    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("save")
    );
    assert_eq!(
        app.world().resource::<Input>().unwrap().key,
        Some(Key::Save)
    );
}
