use rndr_core::*;

fn boot() -> (World, Entity, Entity) {
    let mut w = World::new();
    w.insert_resource(Input::default());
    w.insert_resource(Focus::default());
    let port = w.spawn((
        Rect {
            x: 40.,
            y: 0.,
            w: 100.,
            h: 100.,
        },
        Scroll {
            content: [100., 500.],
            offset: [0., 0.],
        },
    ));
    let bar = w.spawn((
        Rect {
            x: 0.,
            y: 0.,
            w: 24.,
            h: 100.,
        },
        Scrollbar {
            target: port,
            axis: Axis::Column,
        },
        Focusable,
    ));
    (w, port, bar)
}
#[test]
fn coalesced_press_uses_origin_not_hover_and_applies_current_motion() {
    let (mut w, port, bar) = boot();
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        press_at: Some([60., 10.]),
        position: [12., 80.],
        pressed: true,
        down: true,
        ..Default::default()
    };
    apply_scrollbars(&mut w);
    assert!(
        w.resource::<Input>().unwrap().pointer.pressed,
        "editor-origin press must survive hover over track"
    );
    assert_eq!(w.resource::<Focus>().unwrap().entity, None);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], 0.);
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        press_at: Some([12., 10.]),
        position: [900., 48.],
        pressed: true,
        down: true,
        ..Default::default()
    };
    apply_scrollbars(&mut w);
    assert_eq!(w.resource::<Focus>().unwrap().entity, None);
    assert!(scrollbar_dragging(&w, bar));
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -200.);
    assert!(w.resource::<Input>().unwrap().pointer.down);
}
#[test]
fn coalesced_thumb_drag_release_applies_endpoint_without_sticky_capture() {
    let (mut w, port, _) = boot();
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        press_at: Some([12., 10.]),
        position: [900., 86.],
        pressed: true,
        released: true,
        ..Default::default()
    };
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [900., 10.],
        down: true,
        ..Default::default()
    };
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
}
#[test]
fn synthetic_scrollbar_navigation_does_not_mutate_viewport() {
    let (mut w, port, bar) = boot();
    w.resource_mut::<Focus>().unwrap().entity = Some(bar);
    let mut event = KeyEvent::named(NamedKey::End, Modifiers::new());
    event.synthetic = true;
    w.resource_mut::<Input>().unwrap().keys.push(event);
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], 0.);
    assert_eq!(w.resource::<Input>().unwrap().keys.len(), 1);
}
#[test]
fn chrome_only_installs_canonical_scrollbar_keys() {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    let (w, port, bar) = boot();
    // This isolated World fixture has no app systems; create equivalent geometry
    // in the actual Chrome-only app instead of invoking the controller directly.
    let target = app.world_mut().spawn((
        *w.get::<Rect>(port).unwrap(),
        *w.get::<Scroll>(port).unwrap(),
    ));
    let track = app.world_mut().spawn((
        *w.get::<Rect>(bar).unwrap(),
        Scrollbar {
            target,
            axis: Axis::Column,
        },
        Focusable,
    ));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(track);
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .keys
        .push(KeyEvent::named(NamedKey::End, Modifiers::new()));
    app.update();
    assert_eq!(app.world().get::<Scroll>(target).unwrap().offset[1], -400.);
}
