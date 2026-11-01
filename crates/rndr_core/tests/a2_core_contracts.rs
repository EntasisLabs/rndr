use rndr_core::*;
fn key(app: &mut App, n: NamedKey) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(KeyEvent::named(n, Modifiers::new()), Platform::Mac);
    app.update();
    app.world_mut().insert_resource(Input::default());
}
fn fixture(orientation: TabOrientation) -> (App, Entity, [Entity; 3], [Entity; 3], Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let root = app.world_mut().spawn((Layout {
        width: Size::Px(300.0),
        ..Layout::column()
    },));
    let group =
        app.world_mut()
            .spawn((Layout::row(), Tabs { selected: 0 }, ManualTabs(orientation)));
    set_parent(app.world_mut(), group, root);
    let mut tabs = vec![];
    let mut panels = vec![];
    for id in 0..3 {
        let tab = app.world_mut().spawn((
            Layout {
                width: Size::Px(80.0),
                height: Size::Px(45.0),
                ..Layout::column()
            },
            Tab { id },
            Focusable,
            ReleaseActivation,
        ));
        set_parent(app.world_mut(), tab, group);
        let label = app.world_mut().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Fill,
                ..Layout::column()
            },
            Label::new(format!("tab{id}")),
        ));
        set_parent(app.world_mut(), label, tab);
        let panel = app.world_mut().spawn((Layout::column(), TabPanel));
        set_parent(app.world_mut(), panel, root);
        link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
        tabs.push(tab);
        panels.push(panel);
    }
    let after = app.world_mut().spawn((
        Layout::column(),
        Button::new("after"),
        Focusable,
        ReleaseActivation,
    ));
    set_parent(app.world_mut(), after, root);
    app.update();
    (
        app,
        group,
        tabs.try_into().unwrap(),
        panels.try_into().unwrap(),
        after,
    )
}
#[test]
fn manual_orientation_roving_entry_exclusion_and_activation() {
    for (orientation, next, irrelevant) in [
        (
            TabOrientation::Horizontal,
            NamedKey::ArrowRight,
            NamedKey::ArrowDown,
        ),
        (
            TabOrientation::Vertical,
            NamedKey::ArrowDown,
            NamedKey::ArrowRight,
        ),
    ] {
        let (mut app, group, tabs, panels, after) = fixture(orientation);
        key(&mut app, NamedKey::Tab);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[0])
        );
        key(&mut app, irrelevant);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[0])
        );
        key(&mut app, next);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[1])
        );
        assert_eq!(app.world().get::<Tabs>(group).unwrap().selected, 0);
        assert!(!interaction_visible(app.world(), panels[1]));
        key(&mut app, NamedKey::Enter);
        assert_eq!(app.world().get::<Tabs>(group).unwrap().selected, 1);
        assert!(interaction_visible(app.world(), panels[1]));
        assert!(!interaction_visible(app.world(), panels[0]));
        key(&mut app, NamedKey::End);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[2])
        );
        key(&mut app, NamedKey::Tab);
        assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after));
        key(&mut app, NamedKey::Tab);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[1])
        );
        app.world_mut().insert(tabs[1], ControlDisabled);
        request_focus(app.world_mut(), after);
        key(&mut app, NamedKey::Tab);
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(tabs[0])
        );
        assert_eq!(
            access_details(app.world(), group).orientation,
            Some(orientation)
        );
        let tree = access_tree(app.world());
        assert!(!tree.nodes.iter().any(|n| n.id == access_id(panels[0])));
    }
}
#[test]
fn modern_tab_descendant_pointer_release_is_canonical() {
    let (mut app, group, tabs, _, _) = fixture(TabOrientation::Horizontal);
    let r = *app.world().get::<Rect>(tabs[1]).unwrap();
    for (press, release) in [(true, false), (false, true)] {
        let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
        p.position = [r.x + 12.0, r.y + 12.0];
        p.press_at = Some(p.position);
        p.pressed = press;
        p.released = release;
        p.down = press;
        app.update();
        assert_eq!(interaction_pressed(app.world(), tabs[1]), press);
    }
    assert_eq!(app.world().get::<Tabs>(group).unwrap().selected, 1);
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}
#[test]
fn selected_state_is_independent_and_not_implicitly_mutated() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let row = app.world_mut().spawn((
        Layout::column(),
        Button::new("nav"),
        Focusable,
        ReleaseActivation,
        SelectedState(true),
    ));
    assert!(
        access_tree(app.world())
            .nodes
            .iter()
            .find(|n| n.id == access_id(row))
            .unwrap()
            .selected
    );
    assert_ne!(app.world().resource::<Focus>().unwrap().entity, Some(row));
    assert_eq!(
        try_activate(app.world_mut(), row),
        ActivationResult::Accepted
    );
    assert!(app.world().get::<SelectedState>(row).unwrap().0);
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
}
