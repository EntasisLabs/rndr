use rndr_core::*;
use rndr_ui::collections::Item;
use rndr_ui::{collections::*, controls, *};
fn setup() -> (App, Entity, ViewHandle, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.add_system_front(Schedule::Input, apply_scroll);
    app.world_mut().insert_resource(Viewport::new(500.0, 500.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let nodes = [
        TreeNode {
            item: Item::new(1, "Branch"),
            parent: None,
        },
        TreeNode {
            item: Item::new(2, "Leaf"),
            parent: Some(ItemId(1)),
        },
    ];
    let h = TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Tree"),
        &nodes,
        |_, _, _| {},
    )
    .unwrap();
    let after = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(99),
        controls::ButtonProps::new("After", controls::HostIntent::Command(99)),
    )
    .unwrap()
    .root;
    rndr_paint::frame(&mut app);
    let row = h.row(app.world(), ItemId(1)).unwrap().root;
    let toggle = children_of(app.world(), children_of(app.world(), row)[0])[0];
    (app, root, h, toggle, after)
}
#[test]
fn disclosure_enter_then_right_in_one_batch_respects_chronology() {
    let (mut app, _, h, toggle, _) = setup();
    assert!(request_focus(app.world_mut(), toggle));
    app.world_mut().resource_mut::<Input>().unwrap().keys = [NamedKey::Enter, NamedKey::ArrowRight]
        .into_iter()
        .map(|k| KeyEvent::named(k, Modifiers::default()))
        .collect();
    rndr_paint::frame(&mut app);
    // Enter expands first; Right then moves to the now-visible child.
    assert!(
        h.expanded(app.world(), ItemId(1)),
        "queued Enter must not undo later Right"
    );
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
}
#[test]
fn at_disclosure_activate_then_tab_does_not_steal_focus_back() {
    let (mut app, _, h, toggle, after) = setup();
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Focus(toggle)),
        AccessCommandResult::Applied
    );
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Activate(toggle)),
        AccessCommandResult::Activation(ActivationResult::Accepted)
    );
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(after),
        "late disclosure drain must not override chronological Tab"
    );
}
#[test]
fn pointer_disclosure_release_then_tab_does_not_steal_focus_back() {
    let (mut app, _, h, toggle, after) = setup();
    let r = *app.world().get::<Rect>(toggle).unwrap();
    let p = [r.x + r.w / 2.0, r.y + r.h / 2.0];
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: p,
        press_at: Some(p),
        pressed: true,
        down: true,
        ..Pointer::default()
    };
    rndr_paint::frame(&mut app);
    app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
        position: p,
        released: true,
        ..Pointer::default()
    };
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(after),
        "deferred pointer disclosure must not override later Tab"
    );
}
#[test]
fn programmatic_expansion_does_not_cancel_unrelated_armed_host_button() {
    let (mut app, _, h, _, after) = setup();
    request_focus(app.world_mut(), after);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(interaction_pressed(app.world(), after));
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after));
    let mut release = KeyEvent::named(NamedKey::Space, Modifiers::default());
    release.released = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![release];
    rndr_paint::frame(&mut app);
    let requests = take_action_intents(app.world_mut());
    assert!(
        requests.iter().any(|i| i.entity == after),
        "unrelated expansion must not swallow armed host release"
    );
}
#[test]
fn focused_disclosure_becoming_leaf_normalizes_to_surviving_row() {
    let (mut app, root, h, toggle, _) = setup();
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    request_focus(app.world_mut(), toggle);
    let nodes = [TreeNode {
        item: Item::new(1, "Now leaf"),
        parent: None,
    }];
    let updated = TreeView::mount(
        app.world_mut(),
        root,
        UiId(20),
        ViewProps::new("Tree"),
        &nodes,
        |_, _, _| {},
    )
    .unwrap();
    assert_eq!(h, updated);
    assert!(!h.expanded(app.world(), ItemId(1)));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(h.row(app.world(), ItemId(1)).unwrap().root)
    );
    rndr_paint::frame(&mut app);
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
}
#[test]
fn disclosure_enter_then_right_in_separate_frames_is_control() {
    let (mut app, _, h, toggle, _) = setup();
    request_focus(app.world_mut(), toggle);
    for k in [NamedKey::Enter, NamedKey::ArrowRight] {
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(k, Modifiers::default())];
        rndr_paint::frame(&mut app);
    }
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(h.focused(app.world()), Some(ItemId(2)));
}

#[test]
fn disclosure_admission_preserves_capacity_and_unrelated_fifo() {
    let (mut app, root, h, toggle, after) = setup();
    let other = controls::Button::mount(
        app.world_mut(),
        root,
        UiId(100),
        controls::ButtonProps::new("Other", controls::HostIntent::Command(100)),
    )
    .unwrap()
    .root;
    for i in 0..ActionIntents::CAPACITY {
        assert_eq!(
            try_activate(app.world_mut(), if i % 2 == 0 { after } else { other }),
            ActivationResult::Accepted
        );
    }
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Activate(toggle)),
        AccessCommandResult::Activation(ActivationResult::Overflow)
    );
    assert!(
        !h.expanded(app.world(), ItemId(1)),
        "overflow must not mutate semantic state"
    );
    assert_eq!(app.world().resource::<ActionIntents>().unwrap().overflow, 1);
    let intents = take_action_intents(app.world_mut());
    assert_eq!(intents.len(), ActionIntents::CAPACITY);
    for (i, intent) in intents.iter().enumerate() {
        assert_eq!(intent.entity, if i % 2 == 0 { after } else { other });
    }
    assert_eq!(
        try_activate(app.world_mut(), after),
        ActivationResult::Accepted
    );
    assert_eq!(
        try_apply_access(app.world_mut(), AccessCommand::Activate(toggle)),
        AccessCommandResult::Activation(ActivationResult::Accepted)
    );
    assert!(
        h.expanded(app.world(), ItemId(1)),
        "accepted AT effect is synchronous"
    );
    let intents = take_action_intents(app.world_mut());
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].entity, after);
}

#[test]
fn disclosure_rejects_repeat_synthetic_readonly_disabled_and_hidden() {
    let (mut app, _, h, toggle, _) = setup();
    request_focus(app.world_mut(), toggle);
    let mut synthetic = KeyEvent::named(NamedKey::Enter, Modifiers::default());
    synthetic.synthetic = true;
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Enter, Modifiers::default()).repeating(),
        KeyEvent::named(NamedKey::Enter, Modifiers::default()).released(),
        synthetic,
    ];
    rndr_paint::frame(&mut app);
    assert!(!h.expanded(app.world(), ItemId(1)));
    app.world_mut()
        .get_mut::<Semantics>(toggle)
        .unwrap()
        .readonly = true;
    assert_eq!(
        try_activate(app.world_mut(), toggle),
        ActivationResult::Rejected
    );
    app.world_mut()
        .get_mut::<Semantics>(toggle)
        .unwrap()
        .readonly = false;
    app.world_mut().get_mut::<Button>(toggle).unwrap().enabled = false;
    assert_eq!(
        try_activate(app.world_mut(), toggle),
        ActivationResult::Rejected
    );
    app.world_mut().get_mut::<Button>(toggle).unwrap().enabled = true;
    app.world_mut().insert(toggle, Hidden);
    assert_eq!(
        try_activate(app.world_mut(), toggle),
        ActivationResult::Rejected
    );
    assert!(!h.expanded(app.world(), ItemId(1)));
}

#[test]
fn collapse_cancels_hidden_owned_space_even_after_reopen() {
    let (mut app, _, h, _, _) = setup();
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    h.focus(app.world_mut(), ItemId(2)).unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    let leaf = h.row(app.world(), ItemId(2)).unwrap().root;
    assert!(interaction_pressed(app.world(), leaf));
    h.set_expanded(app.world_mut(), ItemId(1), false).unwrap();
    assert!(!interaction_pressed(app.world(), leaf));
    assert_eq!(h.focused(app.world()), Some(ItemId(1)));
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    h.focus(app.world_mut(), ItemId(2)).unwrap();
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default()).released()];
    rndr_paint::frame(&mut app);
    assert_eq!(
        h.selected(app.world()),
        None,
        "hidden arm cannot revive on reopen"
    );
}

#[test]
fn collapse_preserves_unrelated_host_space_release() {
    let (mut app, _, h, _, after) = setup();
    h.set_expanded(app.world_mut(), ItemId(1), true).unwrap();
    request_focus(app.world_mut(), after);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(interaction_pressed(app.world(), after));
    h.set_expanded(app.world_mut(), ItemId(1), false).unwrap();
    assert!(interaction_pressed(app.world(), after));
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default()).released()];
    rndr_paint::frame(&mut app);
    assert_eq!(
        take_action_intents(app.world_mut())
            .iter()
            .map(|i| i.entity)
            .collect::<Vec<_>>(),
        vec![after]
    );
}

#[test]
fn disclosure_space_release_is_chronological_and_escape_cancels() {
    let (mut app, _, h, toggle, after) = setup();
    request_focus(app.world_mut(), toggle);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    assert!(!h.expanded(app.world(), ItemId(1)));
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
        KeyEvent::named(NamedKey::Tab, Modifiers::default()),
    ];
    rndr_paint::frame(&mut app);
    assert!(h.expanded(app.world(), ItemId(1)));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(after));
    request_focus(app.world_mut(), toggle);
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    rndr_paint::frame(&mut app);
    app.world_mut().resource_mut::<Input>().unwrap().keys = vec![
        KeyEvent::named(NamedKey::Escape, Modifiers::default()),
        KeyEvent::named(NamedKey::Space, Modifiers::default()).released(),
    ];
    rndr_paint::frame(&mut app);
    assert!(
        h.expanded(app.world(), ItemId(1)),
        "Escape must prevent pending collapse"
    );
}

#[test]
fn unrelated_branch_changes_preserve_surviving_armed_disclosure() {
    for open in [true, false] {
        let (mut app, root, h, toggle, _) = setup();
        let nodes = [
            TreeNode {
                item: Item::new(1, "Branch"),
                parent: None,
            },
            TreeNode {
                item: Item::new(2, "Leaf"),
                parent: Some(ItemId(1)),
            },
            TreeNode {
                item: Item::new(3, "Other branch"),
                parent: None,
            },
            TreeNode {
                item: Item::new(4, "Other leaf"),
                parent: Some(ItemId(3)),
            },
        ];
        TreeView::mount(
            app.world_mut(),
            root,
            UiId(20),
            ViewProps::new("Tree"),
            &nodes,
            |_, _, _| {},
        )
        .unwrap();
        h.set_expanded(app.world_mut(), ItemId(3), !open).unwrap();
        request_focus(app.world_mut(), toggle);
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
        rndr_paint::frame(&mut app);
        assert!(interaction_pressed(app.world(), toggle));
        h.set_expanded(app.world_mut(), ItemId(3), open).unwrap();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(toggle)
        );
        assert!(
            interaction_pressed(app.world(), toggle),
            "unrelated branch must preserve armed disclosure"
        );
        app.world_mut().resource_mut::<Input>().unwrap().keys =
            vec![KeyEvent::named(NamedKey::Space, Modifiers::default()).released()];
        rndr_paint::frame(&mut app);
        assert!(h.expanded(app.world(), ItemId(1)));
        assert_eq!(h.expanded(app.world(), ItemId(3)), open);
    }
}
