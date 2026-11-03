use rndr_core::*;
#[derive(Clone)]
struct Probe {
    a: Entity,
    b: Entity,
    host: Entity,
    trace: Vec<Entity>,
}
fn record_and_reenter(w: &mut World, e: Entity) {
    w.resource_mut::<Probe>().unwrap().trace.push(e);
    let p = w.resource::<Probe>().unwrap().clone();
    if e == p.a {
        assert_eq!(try_activate(w, p.b), ActivationResult::Accepted);
    }
    assert_eq!(try_activate(w, p.host), ActivationResult::Accepted);
    // Callback may invalidate its own target, with no live core queue borrow.
    w.insert(e, Hidden);
}
fn button(w: &mut World, name: &str, handled: bool) -> Entity {
    let e = w.spawn((
        Button {
            action: name.into(),
            enabled: true,
        },
        ReleaseActivation,
    ));
    if handled {
        w.insert(e, ActivationHandler(record_and_reenter));
    }
    e
}
#[test]
fn nested_handlers_self_invalidation_and_host_fifo() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let w = app.world_mut();
    let a = button(w, "A", true);
    let b = button(w, "B", true);
    let host = button(w, "host", false);
    w.insert_resource(Probe {
        a,
        b,
        host,
        trace: vec![],
    });
    assert_eq!(try_activate(w, host), ActivationResult::Accepted);
    assert_eq!(try_activate(w, a), ActivationResult::Accepted);
    assert_eq!(w.resource::<Probe>().unwrap().trace, vec![a, b]);
    let q = take_action_intents(w);
    assert_eq!(
        q.iter().map(|i| i.entity).collect::<Vec<_>>(),
        vec![host, host, host]
    );
    assert_eq!(try_activate(w, a), ActivationResult::Rejected);
}
#[test]
fn nested_handlers_at_last_fifo_slot_consume_only_their_own_admission() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let w = app.world_mut();
    let a = button(w, "A", true);
    let b = button(w, "B", true);
    let host = button(w, "host", false);
    w.insert_resource(Probe {
        a,
        b,
        host,
        trace: vec![],
    });
    for _ in 0..63 {
        assert_eq!(try_activate(w, host), ActivationResult::Accepted);
    }
    // Both handlers consume their own admission before re-entry. B fills slot64;
    // A's attempt then overflows, without evicting the earlier host requests.
    // Use a dedicated callback because overflow is expected for the outer host.
    fn outer(w: &mut World, e: Entity) {
        w.resource_mut::<Probe>().unwrap().trace.push(e);
        let p = w.resource::<Probe>().unwrap().clone();
        assert_eq!(try_activate(w, p.b), ActivationResult::Accepted);
        assert_eq!(try_activate(w, p.host), ActivationResult::Overflow);
    }
    w.insert(a, ActivationHandler(outer));
    assert_eq!(try_activate(w, a), ActivationResult::Accepted);
    assert_eq!(w.resource::<Probe>().unwrap().trace, vec![a, b]);
    assert_eq!(w.resource::<ActionIntents>().unwrap().overflow, 1);
    let q = take_action_intents(w);
    assert_eq!(q.len(), 64);
    assert!(q.iter().all(|i| i.entity == host));
}
