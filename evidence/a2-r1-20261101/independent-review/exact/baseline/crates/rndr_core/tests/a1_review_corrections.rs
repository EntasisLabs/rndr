use rndr_core::*;

fn app(modern: bool) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let b = app.world_mut().spawn((Layout::column(), Button::new("go")));
    if modern {
        app.world_mut().insert(b, ReleaseActivation);
    }
    app.update();
    (app, b)
}
fn k(key: NamedKey) -> KeyEvent {
    KeyEvent::named(key, Modifiers::new())
}
fn batch(app: &mut App, keys: Vec<KeyEvent>) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.keys.clear();
    input.key = None;
    input.chars.clear();
    for key in keys {
        input.push_key(key, Platform::Mac);
    }
    app.update();
}

// Promoted from /tmp/rndr-review.kCZS9d/probe.rs, now asserting both edges.
#[test]
fn traversal_space_pair_then_unmatched_release() {
    let (mut app, b) = app(true);
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::Space),
            k(NamedKey::Space).released(),
        ],
    );
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(b));
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    batch(&mut app, vec![k(NamedKey::Space).released()]);
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

// Promoted from mixed.rs. Traversal must rebuild the first-press legacy shim.
#[test]
fn traversal_then_legacy_enter() {
    let (mut app, b) = app(false);
    batch(&mut app, vec![k(NamedKey::Tab), k(NamedKey::Enter)]);
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(b));
    assert_eq!(
        app.world().resource::<Fired>().unwrap().name.as_deref(),
        Some("go")
    );
    assert!(take_action_intents(app.world_mut()).is_empty());
}

#[test]
fn modified_release_resolves_arm_without_later_activation() {
    let (mut app, b) = app(true);
    request_focus(app.world_mut(), b);
    batch(&mut app, vec![k(NamedKey::Space)]);
    let mut release = k(NamedKey::Space).released();
    release.modifiers.ctrl = true;
    batch(&mut app, vec![release]);
    batch(&mut app, vec![k(NamedKey::Space).released()]);
    assert!(take_action_intents(app.world_mut()).is_empty());
}

#[test]
fn repeated_enter_and_at_are_distinct_before_drain() {
    let (mut app, b) = app(true);
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::Enter),
            k(NamedKey::Enter).released(),
            k(NamedKey::Enter),
        ],
    );
    for _ in 0..2 {
        assert_eq!(
            try_apply_access(app.world_mut(), AccessCommand::Activate(b)),
            AccessCommandResult::Activation(ActivationResult::Accepted)
        );
    }
    assert_eq!(take_action_intents(app.world_mut()).len(), 4);
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

#[test]
fn subsequent_unmatched_release_cannot_commit_stale_traversal_arm() {
    let (mut app, _) = app(true);
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::Space),
            k(NamedKey::Space).released(),
        ],
    );
    take_action_intents(app.world_mut());
    batch(&mut app, vec![k(NamedKey::Space).released()]);
    assert!(take_action_intents(app.world_mut()).is_empty());
}

#[test]
fn legacy_to_modern_traversal_preserves_space_pair() {
    let (mut app, modern) = app(true);
    let legacy = app
        .world_mut()
        .spawn((Layout::column(), Button::new("legacy")));
    request_focus(app.world_mut(), legacy);
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::Space),
            k(NamedKey::Space).released(),
        ],
    );
    let intents = take_action_intents(app.world_mut());
    assert_eq!(intents.len(), 1);
    assert_eq!(intents[0].entity, modern);
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

#[test]
fn pointer_focus_then_keyboard_and_pointer_release_are_distinct() {
    let (mut app, modern) = app(true);
    app.world_mut().insert(
        modern,
        Layout {
            width: Size::Px(100.0),
            height: Size::Px(40.0),
            ..Layout::column()
        },
    );
    let legacy = app
        .world_mut()
        .spawn((Layout::column(), Button::new("legacy")));
    app.update();
    request_focus(app.world_mut(), legacy);
    let rect = *app.world().get::<Rect>(modern).unwrap();
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.position = [rect.x + 2.0, rect.y + 2.0];
    p.press_at = Some(p.position);
    p.pressed = true;
    p.down = true;
    batch(
        &mut app,
        vec![k(NamedKey::Space), k(NamedKey::Space).released()],
    );
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.pressed = false;
    p.down = false;
    p.released = true;
    batch(&mut app, vec![]);
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    let p = &mut app.world_mut().resource_mut::<Input>().unwrap().pointer;
    p.released = false;
    batch(&mut app, vec![]);
    assert!(take_action_intents(app.world_mut()).is_empty());
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

#[test]
fn space_cancel_focus_blur_synthetic_escape_and_modifiers() {
    for case in 0..7 {
        let (mut app, modern) = app(true);
        let other = app
            .world_mut()
            .spawn((Layout::column(), Button::new("other")));
        request_focus(app.world_mut(), modern);
        batch(&mut app, vec![k(NamedKey::Space)]);
        match case {
            0 => {
                request_focus(app.world_mut(), other);
                request_focus(app.world_mut(), modern);
            }
            1 => cancel_activation(app.world_mut()),
            2 => batch(&mut app, vec![k(NamedKey::Space).synthetic()]),
            3 => batch(&mut app, vec![k(NamedKey::Escape)]),
            4 => {
                send(app.world_mut(), WindowFocus::Blurred);
                batch(&mut app, vec![k(NamedKey::Enter)]);
            }
            5 => {
                let mut e = k(NamedKey::Space).released();
                e.modifiers.alt = true;
                batch(&mut app, vec![e]);
            }
            _ => {
                let mut e = k(NamedKey::Space).released();
                e.modifiers.super_key = true;
                batch(&mut app, vec![e]);
            }
        }
        batch(&mut app, vec![k(NamedKey::Space).released()]);
        assert!(
            take_action_intents(app.world_mut()).is_empty(),
            "case {case}"
        );
        assert!(app.world().resource::<Input>().unwrap().keys.is_empty());
    }
}

#[test]
fn modern_edges_do_not_leak_to_legacy_and_repeat_is_not_new_activation() {
    let (mut app, modern) = app(true);
    let legacy = app
        .world_mut()
        .spawn((Layout::column(), Button::new("legacy")));
    request_focus(app.world_mut(), modern);
    batch(
        &mut app,
        vec![
            k(NamedKey::Enter),
            k(NamedKey::Enter).repeating(),
            k(NamedKey::Enter).released(),
            k(NamedKey::Enter).synthetic(),
        ],
    );
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
    request_focus(app.world_mut(), legacy);
    batch(
        &mut app,
        vec![
            k(NamedKey::Enter).released(),
            k(NamedKey::Enter).synthetic(),
        ],
    );
    let input = app.world().resource::<Input>().unwrap();
    assert!(input.keys.is_empty());
    assert!(input.key.is_none());
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
}

#[test]
fn traversal_into_field_keeps_remaining_edit_order_and_no_dual_shim() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let field =
        app.world_mut()
            .spawn((Layout::column(), TextField, CapturesKeys, Label::new("ab")));
    app.update();
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::ArrowLeft),
            k(NamedKey::Backspace),
        ],
    );
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(field));
    assert_eq!(app.world().get::<Label>(field).unwrap().text, "b");
    assert!(app.world().resource::<Input>().unwrap().keys.is_empty());
    assert!(app.world().resource::<Input>().unwrap().key.is_none());
    assert!(take_action_intents(app.world_mut()).is_empty());
}

#[test]
fn traversal_into_editor_keeps_remaining_edit_order() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let editor = app.world_mut().spawn((
        Layout::column(),
        Editor::default(),
        CapturesKeys,
        Buffer::from_str("ab"),
        Label::new(""),
    ));
    app.update();
    // Buffer starts at end. ArrowLeft then Backspace must run once, in order.
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::ArrowLeft),
            k(NamedKey::Backspace),
        ],
    );
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(editor)
    );
    assert_eq!(app.world().get::<Buffer>(editor).unwrap().text(), "b");
    assert!(take_action_intents(app.world_mut()).is_empty());
}

#[test]
fn rejected_requests_and_drain_validation_preserve_survivor_order() {
    let (mut app, a) = app(true);
    let b = app.world_mut().spawn((Button::new("b"), ReleaseActivation));
    let c = app.world_mut().spawn((Button::new("c"), ReleaseActivation));
    for target in [a, b, a, c, b, a] {
        assert_eq!(
            try_activate(app.world_mut(), target),
            ActivationResult::Accepted
        );
    }
    app.world_mut().insert(b, Hidden);
    assert_eq!(try_activate(app.world_mut(), b), ActivationResult::Rejected);
    app.world_mut().get_mut::<Button>(c).unwrap().action = "changed".into();
    assert_eq!(
        take_action_intents(app.world_mut())
            .iter()
            .map(|i| i.entity)
            .collect::<Vec<_>>(),
        vec![a, a, a]
    );
    for case in 0..4 {
        let e = app
            .world_mut()
            .spawn((Button::new("denied"), ReleaseActivation));
        assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Accepted);
        match case {
            0 => {
                app.world_mut().insert(e, Hidden);
            }
            1 => app.world_mut().get_mut::<Button>(e).unwrap().enabled = false,
            2 => {
                app.world_mut().insert(
                    e,
                    Semantics {
                        readonly: true,
                        ..Semantics::default()
                    },
                );
            }
            _ => despawn(app.world_mut(), e),
        }
        assert_eq!(try_activate(app.world_mut(), e), ActivationResult::Rejected);
        assert!(take_action_intents(app.world_mut()).is_empty());
    }
}

struct Hotkeys;
impl hotkey::HotkeyBackend for Hotkeys {
    fn register(
        &mut self,
        _: hotkey::HotkeyId,
        _: &hotkey::PlatformAccel,
    ) -> Result<(), hotkey::HotkeyFailure> {
        Ok(())
    }
    fn unregister(&mut self, _: hotkey::HotkeyId) {}
    fn poll(&mut self) -> Vec<hotkey::HotkeyId> {
        vec![]
    }
}

#[test]
fn registered_hotkey_cancels_arm_in_order_and_never_leaks_to_legacy() {
    let (mut app, modern) = app(true);
    hotkey::install_backend(app.world_mut(), Hotkeys);
    request_focus(app.world_mut(), modern);
    batch(&mut app, vec![k(NamedKey::Space)]);
    let binding = hotkey::register(app.world_mut(), "Alt+Space").unwrap();
    let mut chord = k(NamedKey::Space);
    chord.modifiers.alt = true;
    batch(&mut app, vec![chord, k(NamedKey::Space).released()]);
    drop(binding);
    batch(&mut app, vec![k(NamedKey::Space).released()]);
    assert!(take_action_intents(app.world_mut()).is_empty());
    let binding = hotkey::register(app.world_mut(), "Alt+Enter").unwrap();
    let mut chord = k(NamedKey::Enter);
    chord.modifiers.alt = true;
    let legacy = app
        .world_mut()
        .spawn((Layout::column(), Button::new("legacy")));
    request_focus(app.world_mut(), legacy);
    batch(
        &mut app,
        vec![
            chord.clone(),
            chord.repeating(),
            k(NamedKey::Enter).released(),
        ],
    );
    assert!(app.world().resource::<Fired>().unwrap().name.is_none());
    assert!(app.world().resource::<Input>().unwrap().keys.is_empty());
    drop(binding);
    batch(
        &mut app,
        vec![
            k(NamedKey::Tab),
            k(NamedKey::Space),
            k(NamedKey::Space).released(),
        ],
    );
    assert_eq!(take_action_intents(app.world_mut()).len(), 1);
}

#[test]
fn same_target_capacity_is_64_requests_not_targets() {
    let (mut app, b) = app(true);
    for _ in 0..64 {
        assert_eq!(try_activate(app.world_mut(), b), ActivationResult::Accepted);
    }
    assert_eq!(try_activate(app.world_mut(), b), ActivationResult::Overflow);
    assert_eq!(app.world().resource::<ActionIntents>().unwrap().overflow, 1);
    assert_eq!(take_action_intents(app.world_mut()).len(), 64);
}
