use rndr_core::*;
fn setup(axis: Axis) -> (World, Entity, Entity) {
    let mut w = World::new();
    w.insert_resource(Input::default());
    w.insert_resource(Viewport::new(800., 600.));
    w.insert_resource(Focus::default());
    let port = w.spawn((
        Layout {
            width: Size::Px(100.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let content = w.spawn((Layout {
        width: Size::Px(500.),
        height: Size::Px(500.),
        ..Layout::column()
    },));
    set_parent(&mut w, content, port);
    let bar = w.spawn((
        Layout {
            width: Size::Px(24.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Focusable,
        Layer(1),
        Scrollbar { target: port, axis },
    ));
    // Separate roots overlap by design, later track paints/hits above the port.
    layout_propagate(&mut w);
    (w, port, bar)
}
fn point(w: &mut World, x: f32, y: f32, pressed: bool, down: bool, released: bool) {
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [x, y],
        press_at: pressed.then_some([x, y]),
        pressed,
        down,
        released,
        ..Pointer::default()
    };
    apply_scrollbars(w);
}
#[test]
fn editor_wheel_without_track_consumes_vertical_then_chains_bounded_remainder() {
    let mut app = App::new();
    app.add_plugin(Chrome);
    let world = app.world_mut();
    let parent = world.spawn((
        Rect {
            x: 0.0,
            y: 0.0,
            w: 200.0,
            h: 100.0,
        },
        Scroll {
            content: [1000.0, 1000.0],
            offset: [0.0, 0.0],
        },
    ));
    let editor = world.spawn((
        Rect {
            x: 0.0,
            y: 0.0,
            w: 180.0,
            h: 100.0,
        },
        Buffer::from_str(&"line\n".repeat(30)),
        Editor::default(),
        Label::new(""),
    ));
    set_parent(world, editor, parent);
    app.update();
    let world = app.world_mut();
    let input = world.resource_mut::<Input>().unwrap();
    input.pointer.position = [50.0, 20.0];
    input.scroll = [-20.0, -32.0];
    apply_scroll(world);
    assert!(world.get::<Editor>(editor).unwrap().offset > 0.0);
    assert_eq!(world.get::<Scroll>(parent).unwrap().offset, [-20.0, 0.0]);
    world.resource_mut::<Input>().unwrap().scroll = [0.0, -100_000.0];
    apply_scroll(world);
    assert_eq!(world.get::<Scroll>(parent).unwrap().offset[1], -900.0);
    assert_eq!(world.resource::<Input>().unwrap().scroll, [0.0, 0.0]);
}
#[test]
fn geometry_and_programmatic_value_follow_canonical_offset_and_clamp() {
    let (mut w, port, bar) = setup(Axis::Column);
    let g = scrollbar_geometry(&w, bar).unwrap();
    assert_eq!(g.max, 400.);
    assert_eq!(g.thumb.h, 24.);
    assert!(set_scrollbar_value(&mut w, bar, 200.));
    assert_eq!(w.get::<Scroll>(port).unwrap().offset, [0., -200.]);
    assert_eq!(scrollbar_geometry(&w, bar).unwrap().thumb.y, 38.);
    assert!(set_scrollbar_value(&mut w, bar, 9999.));
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
    assert!(!set_scrollbar_value(&mut w, bar, f32::NAN));
}
#[test]
fn thumb_capture_tracks_outside_and_release_ends_it() {
    let (mut w, port, bar) = setup(Axis::Column);
    point(&mut w, 12., 10., true, true, false);
    assert_eq!(w.resource::<Focus>().unwrap().entity, None);
    assert!(scrollbar_dragging(&w, bar));
    assert!(
        w.resource::<Input>().unwrap().pointer.down,
        "physical held state must survive consumption"
    );
    point(&mut w, 900., 48., false, true, false);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -200.);
    point(&mut w, 900., 86., false, false, true);
    assert!(!scrollbar_dragging(&w, bar));
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
    point(&mut w, 900., 10., false, true, false);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
}
#[test]
fn track_pages_and_does_not_begin_parent_drag_or_propagate_wheel() {
    let (mut w, port, bar) = setup(Axis::Column);
    let parent = w.spawn((
        Layout {
            width: Size::Px(200.),
            height: Size::Px(100.),
            ..Layout::row()
        },
        Scroll::default(),
    ));
    set_parent(&mut w, port, parent);
    set_parent(&mut w, bar, parent);
    let overflow = w.spawn((Layout {
        width: Size::Px(100.),
        height: Size::Px(500.),
        ..Layout::column()
    },));
    set_parent(&mut w, overflow, parent);
    layout_propagate(&mut w);
    assert!(w.get::<Scroll>(parent).unwrap().content[1] > 100.);
    // Bar follows port in this column, so use current geometry.
    let track = scrollbar_geometry(&w, bar).unwrap().track;
    point(&mut w, track.x + 12., track.y + 80., true, true, false);
    apply_scroll(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -100.);
    assert!(w
        .resource::<ScrollDrag>()
        .is_none_or(|d| d.entity.is_none()));
    let before = w.get::<Scroll>(parent).unwrap().offset;
    let i = w.resource_mut::<Input>().unwrap();
    i.pointer.down = false;
    i.scroll = [0., -10000.];
    apply_scrollbars(&mut w);
    apply_scroll(&mut w);
    assert_eq!(w.get::<Scroll>(parent).unwrap().offset, before);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
}
#[test]
fn keyboard_and_accessibility_set_value_share_bounds() {
    let (mut w, port, bar) = setup(Axis::Column);
    w.insert_resource(Focus { entity: Some(bar) });
    w.resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::End, Modifiers::new())];
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -400.);
    w.resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(NamedKey::PageUp, Modifiers::new())];
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -300.);
    assert_eq!(
        try_apply_access(
            &mut w,
            AccessCommand::SetValue {
                entity: bar,
                value: "20".into()
            }
        ),
        AccessCommandResult::Applied
    );
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -20.);
    assert_eq!(
        try_apply_access(
            &mut w,
            AccessCommand::SetValue {
                entity: bar,
                value: "NaN".into()
            }
        ),
        AccessCommandResult::Rejected
    );
    assert_eq!(
        access_tree(&w)
            .nodes
            .iter()
            .find(|n| n.id == access_id(bar))
            .unwrap()
            .role,
        AccessRole::ScrollBar
    );
}
#[test]
fn hidden_disabled_clipped_stale_targets_and_focus_changes_cancel_capture() {
    let (mut w, port, bar) = setup(Axis::Column);
    point(&mut w, 12., 10., true, true, false);
    w.insert_resource(Focus { entity: Some(bar) });
    point(&mut w, 12., 80., false, true, false);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], 0.);
    w.insert(bar, ControlDisabled);
    assert!(scrollbar_geometry(&w, bar).is_none());
    w.remove::<ControlDisabled>(bar);
    w.insert(port, Hidden);
    assert!(scrollbar_geometry(&w, bar).is_none());
    w.remove::<Hidden>(port);
    w.insert(
        bar,
        Clip {
            x: 0.,
            y: 0.,
            w: 0.,
            h: 0.,
        },
    );
    assert!(scrollbar_geometry(&w, bar).is_none());
    w.remove::<Clip>(bar);
    w.despawn_entity(port);
    assert!(scrollbar_geometry(&w, bar).is_none());
    assert!(!set_scrollbar_value(&mut w, bar, 10.));
}
#[test]
fn horizontal_scroll_uses_x_and_short_content_has_full_thumb() {
    let (mut w, port, bar) = setup(Axis::Row);
    assert!(set_scrollbar_value(&mut w, bar, 50.));
    assert_eq!(w.get::<Scroll>(port).unwrap().offset, [-50., 0.]);
    w.get_mut::<Scroll>(port).unwrap().content = [20., 20.];
    let g = scrollbar_geometry(&w, bar).unwrap();
    assert_eq!(g.max, 0.);
    assert_eq!(g.thumb, g.track);
}
#[test]
fn capture_uses_press_origin_and_cancels_after_retarget() {
    let (mut w, port, bar) = setup(Axis::Column);
    w.resource_mut::<Input>().unwrap().pointer = Pointer {
        position: [900., 48.],
        press_at: Some([12., 10.]),
        pressed: true,
        down: true,
        ..Default::default()
    };
    apply_scrollbars(&mut w);
    assert_eq!(w.resource::<Focus>().unwrap().entity, None);
    assert!(scrollbar_dragging(&w, bar));
    point(&mut w, 900., 48., false, true, false);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -200.);
    let other = w.spawn((
        Rect {
            x: 0.,
            y: 0.,
            w: 100.,
            h: 100.,
        },
        Scroll {
            offset: [0., 0.],
            content: [500., 500.],
        },
    ));
    w.get_mut::<Scrollbar>(bar).unwrap().target = other;
    point(&mut w, 900., 86., false, true, false);
    assert_eq!(w.get::<Scroll>(other).unwrap().offset[1], 0.);
    w.get_mut::<Scrollbar>(bar).unwrap().target = port;
    point(&mut w, 12., 48., true, true, false);
    point(&mut w, 12., 48., false, false, true);
}
#[test]
fn unrelated_keys_survive_scrollbar_navigation_in_same_queue() {
    let (mut w, port, bar) = setup(Axis::Column);
    w.insert_resource(Focus { entity: Some(bar) });
    let i = w.resource_mut::<Input>().unwrap();
    i.keys = vec![
        KeyEvent::named(NamedKey::ArrowDown, Modifiers::new()),
        KeyEvent::named(NamedKey::Tab, Modifiers::new()),
    ];
    apply_scrollbars(&mut w);
    assert_eq!(w.get::<Scroll>(port).unwrap().offset[1], -40.);
    let i = w.resource::<Input>().unwrap();
    assert_eq!(i.keys.len(), 1);
    assert_eq!(i.key, Some(Key::Tab));
}
#[test]
fn window_blur_cancels_capture_even_with_stale_held_state() {
    let mut app = App::new();
    app.add_system(Schedule::Input, apply_scrollbars);
    let w = app.world_mut();
    w.insert_resource(Viewport::new(400., 300.));
    let port = w.spawn((
        Layout {
            width: Size::Px(100.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let content = w.spawn((Layout {
        width: Size::Px(100.),
        height: Size::Px(500.),
        ..Layout::column()
    },));
    set_parent(w, content, port);
    let bar = w.spawn((
        Layout {
            width: Size::Px(24.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Focusable,
        Layer(1),
        Scrollbar {
            target: port,
            axis: Axis::Column,
        },
    ));
    layout_propagate(w);
    point(w, 12., 10., true, true, false);
    assert_eq!(w.resource::<Focus>().and_then(|f| f.entity), None);
    assert!(scrollbar_dragging(w, bar));
    send(w, WindowFocus::Blurred);
    w.resource_mut::<Input>().unwrap().pointer.position = [900., 86.];
    app.update();
    assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], 0.);
    point(app.world_mut(), 900., 86., false, true, false);
    assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], 0.);
}
#[test]
fn readonly_editor_scrolls_without_editing_selection_or_history() {
    let mut app = App::new();
    app.add_plugin(Shell);
    app.add_plugin(Chrome);
    let w = app.world_mut();
    let editor = w.spawn((
        Layout {
            width: Size::Px(100.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Buffer::from_str(&"hello world\n".repeat(100)),
        Editor::default(),
        Label::new(""),
        Focusable,
        Semantics {
            readonly: true,
            ..Default::default()
        },
    ));
    let bar = w.spawn((
        Layout {
            width: Size::Px(24.),
            height: Size::Px(100.),
            ..Layout::column()
        },
        Focusable,
        Scrollbar {
            target: editor,
            axis: Axis::Column,
        },
    ));
    app.update();
    let text = app.world().get::<Buffer>(editor).unwrap().text();
    assert!(set_scrollbar_value(app.world_mut(), bar, 400.));
    app.update();
    assert!(app.world().get::<Editor>(editor).unwrap().offset > 0.);
    assert_eq!(app.world().get::<Buffer>(editor).unwrap().text(), text);
    app.world_mut().get_mut::<Scrollbar>(bar).unwrap().axis = Axis::Row;
    assert!(scrollbar_geometry(app.world(), bar).is_none());
}
