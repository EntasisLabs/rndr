use rndr_core::*;

#[derive(Default)]
struct Probe {
    calls: usize,
    target: Option<Entity>,
    transactions: usize,
    widths: Vec<f32>,
}
fn delegate(w: &mut World, e: Entity) -> Entity {
    let p = w.resource_mut::<Probe>().unwrap();
    p.calls += 1;
    p.target.unwrap_or(e)
}
fn hook(w: &mut World, _: Entity, _: &KeyEvent) -> bool {
    w.resource_mut::<Probe>().unwrap().calls += 1;
    false
}
fn transaction(w: &mut World, _: Entity, _: FieldTransaction) {
    w.resource_mut::<Probe>().unwrap().transactions += 1;
}
fn boot() -> App {
    let mut a = App::new();
    a.add_plugin(GenericInteraction);
    a.world_mut().insert_resource(Probe::default());
    a.world_mut().insert_resource(Viewport::new(400., 240.));
    a
}
#[test]
fn focus_delegation_admits_original_revalidates_return_and_preserves_rejected_arms() {
    let mut a = boot();
    let old = a
        .world_mut()
        .spawn((Layout::column(), Button::new("old"), ReleaseActivation));
    let row = a
        .world_mut()
        .spawn((Layout::column(), Focusable, FocusTarget(delegate)));
    let target = a.world_mut().spawn((Layout::column(), Focusable));
    a.update();
    assert!(request_focus(a.world_mut(), old));
    a.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::Space, Modifiers::default())];
    a.update();
    assert!(interaction_pressed(a.world(), old));
    a.world_mut().resource_mut::<Probe>().unwrap().target = Some(target);
    a.world_mut().insert(row, ControlDisabled);
    assert!(!request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Probe>().unwrap().calls, 0);
    a.world_mut().remove::<ControlDisabled>(row);
    a.world_mut().insert(target, ControlDisabled);
    assert!(!request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(old));
    assert!(interaction_pressed(a.world(), old));
    a.world_mut().remove::<ControlDisabled>(target);
    despawn(a.world_mut(), target);
    assert!(!request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(old));
    a.world_mut().resource_mut::<Probe>().unwrap().target = Some(row);
    assert!(request_focus(a.world_mut(), row));
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(row));
    assert!(!interaction_pressed(a.world(), old));
    // Delegation is not inherited by an independently focusable child or AT.
    let child = a.world_mut().spawn((Layout::column(), Focusable));
    set_parent(a.world_mut(), child, row);
    let calls = a.world().resource::<Probe>().unwrap().calls;
    assert_eq!(
        try_apply_access(a.world_mut(), AccessCommand::Focus(child)),
        AccessCommandResult::Applied
    );
    assert_eq!(a.world().resource::<Probe>().unwrap().calls, calls);
    assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(child));
}

fn reconcile(w: &mut World, e: Entity) -> bool {
    let width = w.get::<Rect>(e).map(|r| r.w).unwrap_or(0.);
    let p = w.resource_mut::<Probe>().unwrap();
    p.widths.push(width);
    let changed = p.widths.len() == 1 || p.widths[p.widths.len() - 2] != width;
    // Ordinary topology mutation is permitted; no recursive layout call.
    if children_of(w, e).is_empty() {
        let child = w.spawn((Layout {
            width: Size::Fill,
            height: Size::Px(20.),
            ..Layout::column()
        },));
        set_parent(w, child, e);
    }
    changed
}
fn remove_self(w: &mut World, e: Entity) -> bool {
    w.resource_mut::<Probe>().unwrap().calls += 1;
    w.remove::<ViewportRows>(e);
    despawn(w, e);
    true
}
#[test]
fn viewport_hooks_are_bounded_and_topology_mutation_is_observed_before_placement() {
    let mut a = boot();
    let root = a.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(100.),
            ..Layout::column()
        },
        ViewportRows(reconcile),
    ));
    layout_propagate(a.world_mut());
    assert_eq!(
        a.world().resource::<Probe>().unwrap().widths,
        vec![0., 400.]
    );
    let child = children_of(a.world(), root)[0];
    assert_eq!(a.world().get::<Rect>(child).unwrap().w, 400.);
    a.world_mut()
        .resource_mut::<Probe>()
        .unwrap()
        .widths
        .clear();
    a.world_mut().insert_resource(Viewport::new(600., 240.));
    layout_propagate(a.world_mut());
    assert_eq!(
        a.world().resource::<Probe>().unwrap().widths,
        vec![400., 600.]
    );
    assert_eq!(a.world().get::<Rect>(child).unwrap().w, 600.);
    let retired = a
        .world_mut()
        .spawn((Layout::column(), ViewportRows(remove_self)));
    layout_propagate(a.world_mut());
    assert!(!a.world().is_alive(retired));
    assert_eq!(a.world().resource::<Probe>().unwrap().calls, 1);
}

#[test]
fn explicit_editor_hook_falls_through_space_enter_and_transactional_ownership_runs_first() {
    for transactional in [false, true] {
        let mut a = boot();
        let field = a.world_mut().spawn((
            Layout::column(),
            Label::new(""),
            TextField,
            CapturesKeys,
            Focusable,
            KeyNavigation(hook),
        ));
        if transactional {
            a.world_mut().insert(field, FieldTransactions(transaction));
        }
        a.update();
        request_focus(a.world_mut(), field);
        a.world_mut().resource_mut::<Input>().unwrap().keys = vec![
            KeyEvent::character('a', Modifiers::default()),
            KeyEvent::character(' ', Modifiers::default()),
            KeyEvent::named(NamedKey::Enter, Modifiers::default()),
        ];
        a.update();
        assert_eq!(a.world().get::<Label>(field).unwrap().text, "a ");
        assert!(events::<Submitted>(a.world())
            .iter()
            .any(|s| s.entity == field && s.text == "a "));
        assert_eq!(
            a.world().resource::<Probe>().unwrap().calls,
            if transactional { 0 } else { 3 }
        );
        assert_eq!(a.world().resource::<Focus>().unwrap().entity, Some(field));
        assert!(take_action_intents(a.world_mut()).is_empty());
    }
}
