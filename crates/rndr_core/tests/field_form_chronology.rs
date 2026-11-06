use rndr_core::*;

#[derive(Default)]
struct Signals(Vec<(Entity, FieldTransaction)>);
fn record(world: &mut World, entity: Entity, signal: FieldTransaction) {
    world
        .resource_mut::<Signals>()
        .unwrap()
        .0
        .push((entity, signal));
}
fn form() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Signals::default());
    let world = app.world_mut();
    let root = world.spawn(Layout::column());
    let mut fields = Vec::new();
    for text in ["A", "B"] {
        let entity = world.spawn((
            Layout::column(),
            Label::new(text),
            TextField,
            FieldTransactions(record),
            Focusable,
            CapturesKeys,
        ));
        set_parent(world, entity, root);
        fields.push(entity);
    }
    request_focus(world, fields[0]);
    app.update();
    app.world_mut().resource_mut::<Signals>().unwrap().0.clear();
    (app, fields[0], fields[1])
}

#[test]
fn legacy_text_is_applied_and_published_before_tab_blur() {
    let (mut app, a, b) = form();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.chars = "x".into();
    input.key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "Ax");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(b));
    assert_eq!(
        app.world().resource::<Signals>().unwrap().0,
        vec![
            (a, FieldTransaction::Changed("Ax".into())),
            (a, FieldTransaction::FocusLost(Some(b))),
        ]
    );
    assert_eq!(
        events::<TextChanged>(app.world()),
        &[TextChanged {
            entity: a,
            text: "Ax".into()
        }]
    );
    app.update();
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
    assert!(events::<TextChanged>(app.world()).is_empty());
}

#[test]
fn legacy_text_before_tab_participates_in_existing_undo_redo_history() {
    let (mut app, a, b) = form();
    app.world_mut().resource_mut::<Input>().unwrap().chars = "p".into();
    app.update();
    request_focus(app.world_mut(), b);
    request_focus(app.world_mut(), a);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.chars = "x".into();
    input.key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "Apx");
    request_focus(app.world_mut(), a);
    let platform = platform_of(app.world());
    let mut modifiers = Modifiers::new();
    if platform.primary_is_super() {
        modifiers.super_key = true;
    } else {
        modifiers.ctrl = true;
    }
    for (shift, expected) in [(false, "Ap"), (true, "Apx")] {
        modifiers.shift = shift;
        app.world_mut()
            .resource_mut::<Input>()
            .unwrap()
            .push_key(KeyEvent::character('z', modifiers), platform);
        app.update();
        assert_eq!(app.world().get::<Label>(a).unwrap().text, expected);
        assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
    }
}

#[test]
fn readonly_legacy_text_is_discarded_not_redirected_by_tab() {
    let (mut app, a, b) = form();
    app.world_mut().insert(
        a,
        Semantics {
            readonly: true,
            ..Semantics::default()
        },
    );
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.chars = "unwanted".into();
    input.key = Some(Key::Tab);
    app.update();
    assert_eq!(app.world().get::<Label>(a).unwrap().text, "A");
    assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(b));
    assert_eq!(
        app.world().resource::<Signals>().unwrap().0,
        vec![(a, FieldTransaction::FocusLost(Some(b)))]
    );
    assert!(events::<TextChanged>(app.world()).is_empty());
}

#[test]
fn composition_tab_stays_on_its_field_for_legacy_and_rich_input() {
    for rich in [false, true] {
        let (mut app, a, b) = form();
        let platform = platform_of(app.world());
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit = "に".into();
        input.key = Some(Key::Tab);
        if rich {
            input.push_key(KeyEvent::named(NamedKey::Tab, Modifiers::new()), platform);
        }
        app.update();
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(a));
        assert_eq!(app.world().get::<Label>(a).unwrap().text, "A");
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.ime_preedit.clear();
        input.ime_commit = "日本語".into();
        input.key = Some(Key::Tab);
        if rich {
            input.push_key(KeyEvent::named(NamedKey::Tab, Modifiers::new()), platform);
        }
        app.update();
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(a));
        assert_eq!(app.world().get::<Label>(a).unwrap().text, "A日本語");
        assert_eq!(app.world().get::<Label>(b).unwrap().text, "B");
        app.update();
        assert_eq!(app.world().get::<Label>(a).unwrap().text, "A日本語");
    }
}
