use rndr_core::*;

fn boot() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(Chrome)
        .add_plugin(GenericInteraction)
        .add_plugin(Chrome)
        .add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Viewport::new(400.0, 240.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        ..Layout::column()
    },));
    let mut make = |modern| {
        let e = app.world_mut().spawn((
            Layout {
                width: Size::Px(100.0),
                height: Size::Px(40.0),
                ..Layout::column()
            },
            Button::new(if modern { "modern" } else { "legacy" }),
            Focusable,
        ));
        set_parent(app.world_mut(), e, root);
        if modern {
            app.world_mut().insert(e, ReleaseActivation);
        }
        e
    };
    let modern = make(true);
    let legacy = make(false);
    app.update();
    (app, modern, legacy)
}
fn point(app: &mut App, e: Entity, press: bool, release: bool) {
    let r = *app.world().get::<Rect>(e).unwrap();
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position = [r.x + 3.0, r.y + 3.0];
    p.press_at = Some(p.position);
    p.pressed = press;
    p.down = press && !release;
    p.released = release;
    app.update();
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.pressed = false;
    p.released = false;
}
fn key(app: &mut App, event: KeyEvent) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .push_key(event, Platform::Mac);
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys.clear();
    input.key = None;
}
fn k(n: NamedKey) -> KeyEvent {
    KeyEvent::named(n, Modifiers::new())
}

#[test]
fn mixed_press_and_release_and_at_share_canonical_route() {
    let (mut app, modern, legacy) = boot();
    assert_eq!(app.world().resource::<Actions>().unwrap().bindings.len(), 3);
    point(&mut app, modern, true, false);
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
    point(&mut app, modern, false, true);
    apply_access(app.world_mut(), AccessCommand::Activate(modern));
    assert_eq!(
        app.world().resource::<ActionIntents>().unwrap().coalesced,
        1
    );
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    point(&mut app, legacy, true, false);
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("legacy")
    );
    assert!(take_action_intents(app.world_mut()).is_empty());
    key(&mut app, k(NamedKey::Enter));
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("legacy")
    );
}
#[test]
fn pointer_cancel_outside_escape_blur_capture_disable_and_destroy() {
    for case in 0..7 {
        let (mut app, modern, legacy) = boot();
        point(&mut app, modern, true, false);
        match case {
            0 => {
                let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
                p.position = [390.0, 230.0];
                p.released = true;
                p.down = false;
                app.update();
            }
            1 => {
                key(&mut app, k(NamedKey::Escape));
                point(&mut app, modern, false, true);
            }
            2 => {
                send(app.world_mut(), WindowFocus::Blurred);
                app.update();
                point(&mut app, modern, false, true);
            }
            3 => {
                cancel_activation(app.world_mut());
                point(&mut app, modern, false, true);
            }
            4 => {
                app.world_mut().get_mut::<Button>(modern).unwrap().enabled = false;
                point(&mut app, modern, false, true);
            }
            5 => {
                despawn(app.world_mut(), modern);
                point(&mut app, legacy, false, true);
            }
            _ => {
                request_focus(app.world_mut(), legacy);
                point(&mut app, modern, false, true);
            }
        }
        assert!(
            take_action_intents(app.world_mut()).is_empty(),
            "case {case}"
        );
    }
}
#[test]
fn space_release_enter_down_repeat_synthetic_and_focus_loss() {
    let (mut app, modern, legacy) = boot();
    request_focus(app.world_mut(), modern);
    key(&mut app, k(NamedKey::Space));
    assert!(take_action_intents(app.world_mut()).is_empty());
    key(&mut app, k(NamedKey::Space).repeating());
    key(&mut app, k(NamedKey::Space).released());
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    key(&mut app, k(NamedKey::Enter));
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    key(&mut app, k(NamedKey::Enter).repeating());
    key(&mut app, k(NamedKey::Enter).released());
    assert!(take_action_intents(app.world_mut()).is_empty());
    key(&mut app, k(NamedKey::Space));
    request_focus(app.world_mut(), legacy);
    key(&mut app, k(NamedKey::Space).released());
    assert!(take_action_intents(app.world_mut()).is_empty());
    request_focus(app.world_mut(), modern);
    let mut synthetic = k(NamedKey::Enter);
    synthetic.synthetic = true;
    key(&mut app, synthetic);
    assert!(take_action_intents(app.world_mut()).is_empty());
}
#[test]
fn cardinality_order_overflow_revalidation_and_generation() {
    let (mut app, modern, _) = boot();
    let mut targets = vec![modern];
    for i in 1..65 {
        targets.push(
            app.world_mut()
                .spawn((Button::new(format!("{i}")), ReleaseActivation)),
        );
    }
    for &e in &targets[..64] {
        assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Accepted);
    }
    assert_eq!(
        try_activate(app.world_mut(), modern),
        ActivationResult::AlreadyPending
    );
    assert_eq!(
        try_activate(app.world_mut(), targets[64]),
        ActivationResult::Overflow
    );
    assert_eq!(app.world().resource::<ActionIntents>().unwrap().overflow, 1);
    app.world_mut().insert(targets[1], Hidden);
    app.world_mut()
        .get_mut::<Button>(targets[2])
        .unwrap()
        .enabled = false;
    app.world_mut().insert(
        targets[3],
        Semantics {
            readonly: true,
            ..Semantics::default()
        },
    );
    despawn(app.world_mut(), targets[4]);
    let replacement = app
        .world_mut()
        .spawn((Button::new("reused"), ReleaseActivation));
    assert_ne!(replacement, targets[4]);
    assert_eq!(
        try_activate(app.world_mut(), targets[4]),
        ActivationResult::Rejected
    );
    let received = take_action_intents(app.world_mut());
    assert_eq!(received.len(), 60);
    assert_eq!(received[0].entity, modern);
    assert_eq!(received[1].entity, targets[5]);
    assert_eq!(
        try_activate(app.world_mut(), targets[64]),
        ActivationResult::Accepted
    );
}
#[test]
fn generic_tab_focus_no_shell_side_effects_editor_keeps_keys() {
    let (mut app, modern, legacy) = boot();
    key(&mut app, k(NamedKey::Tab));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(modern)
    );
    key(&mut app, k(NamedKey::Tab));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(legacy)
    );
    key(&mut app, k(NamedKey::Escape));
    assert!(!app.world().resource::<Input>().unwrap().quit);
    assert!(!app.world().has_resource::<Selection>());
    app.world_mut().insert(legacy, CapturesKeys);
    key(&mut app, k(NamedKey::Tab));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(legacy)
    );
}
#[test]
fn allowed_matrix_names_readonly_hidden_unsupported_and_safe_setvalue() {
    let (mut app, modern, _) = boot();
    app.world_mut().insert(
        modern,
        Semantics {
            name: Some("Persistent".into()),
            description: "Help".into(),
            readonly: true,
        },
    );
    let details = access_details(app.world(), modern);
    assert!(details.focus);
    assert!(!details.activate);
    assert!(details.readonly);
    assert_eq!(details.description, "Help");
    assert_eq!(
        access_tree(app.world())
            .nodes
            .iter()
            .find(|n| n.id == access_id(modern))
            .unwrap()
            .name,
        "Persistent"
    );
    apply_access(app.world_mut(), AccessCommand::Activate(modern));
    assert!(take_action_intents(app.world_mut()).is_empty());
    let text = app.world_mut().spawn((TextField, Label::new("original")));
    assert!(!access_details(app.world(), text).set_value);
    apply_access(
        app.world_mut(),
        AccessCommand::SetValue {
            entity: text,
            value: "unsafe".into(),
        },
    );
    assert_eq!(app.world().get::<Label>(text).unwrap().text, "original");
    app.world_mut().insert(modern, Hidden);
    assert!(!request_focus(app.world_mut(), modern));
    let plain = app.world_mut().spawn((Label::new("prose"),));
    assert!(!request_focus(app.world_mut(), plain));
}
#[test]
fn real_panel_relations_manual_focus_and_inactive_exclusion() {
    let (mut app, _, _) = boot();
    let group = app.world_mut().spawn((
        Tabs { selected: 1 },
        Layout::row(),
        Semantics {
            name: Some("Document mode".into()),
            ..Semantics::default()
        },
    ));
    let mut tabs = vec![];
    let mut panels = vec![];
    for id in 1..=2 {
        let tab = app.world_mut().spawn((Tab { id }, Layout::column()));
        set_parent(app.world_mut(), tab, group);
        let panel = app.world_mut().spawn((TabPanel, Layout::column()));
        link_tab_panel(app.world_mut(), tab, group, panel).unwrap();
        tabs.push(tab);
        panels.push(panel);
    }
    app.update();
    assert!(!interaction_visible(app.world(), panels[1]));
    assert_eq!(
        access_details(app.world(), tabs[0]).controls,
        Some(access_id(panels[0]))
    );
    assert_eq!(
        access_details(app.world(), panels[0]).labelled_by,
        Some(access_id(tabs[0]))
    );
    assert!(move_tab_focus(app.world_mut(), group, TabFocusMove::Last));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(tabs[1])
    );
    assert_eq!(app.world().get::<Tabs>(group).unwrap().selected, 1);
    apply_access(app.world_mut(), AccessCommand::Activate(tabs[1]));
    assert_eq!(app.world().get::<Tabs>(group).unwrap().selected, 2);
    assert!(!interaction_visible(app.world(), panels[0]));
    assert!(interaction_visible(app.world(), panels[1]));
    let tree = access_tree(app.world());
    assert!(!tree.nodes.iter().any(|n| n.id == access_id(panels[0])));
    assert!(tree.nodes.iter().any(|n| n.id == access_id(panels[1])));
    assert!(
        tree.nodes
            .iter()
            .find(|n| n.id == access_id(tabs[1]))
            .unwrap()
            .selected
    );
}
