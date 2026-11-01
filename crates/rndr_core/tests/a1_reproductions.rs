use rndr_core::*;

#[test]
fn folded_button_child_label_is_name() {
    let mut world = World::new();
    let button = world.spawn((Button::new("save"), Layout::column()));
    let label = world.spawn((Label::new("Save document"),));
    set_parent(&mut world, label, button);
    let tree = access_tree(&world);
    assert_eq!(
        tree.nodes
            .iter()
            .find(|n| n.id == access_id(button))
            .unwrap()
            .name,
        "Save document"
    );
}

#[test]
fn at_set_value_must_not_bypass_editor_history() {
    let mut world = World::new();
    let field = world.spawn((TextField, Label::new("original")));
    apply_access(
        &mut world,
        AccessCommand::SetValue {
            entity: field,
            value: "unvalidated".into(),
        },
    );
    // A1 deliberately rejects until a validated editor/history route exists.
    assert_eq!(world.get::<Label>(field).unwrap().text, "original");
}

#[test]
fn advertised_tab_activation_selects_panel() {
    let mut world = World::new();
    let tabs = world.spawn((Tabs { selected: 1 }, Layout::row()));
    world.spawn((Parent { entity: tabs }, Tab { id: 1 }));
    let target = world.spawn((Parent { entity: tabs }, Tab { id: 2 }));
    apply_access(&mut world, AccessCommand::Activate(target));
    assert_eq!(world.get::<Tabs>(tabs).unwrap().selected, 2);
}
