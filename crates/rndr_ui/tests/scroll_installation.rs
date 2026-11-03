use rndr_core::*;
use rndr_ui::UiPlugin;

// Exercise both the minimal host and registrations retained by older hosts.
// Input-front on either side of plugin installation and Shell's Update path
// must not multiply wheel deltas or drag movement.
fn host(mode: u8) -> (App, Entity, Entity) {
    let mut app = App::new();
    if mode == 1 {
        app.add_system_front(Schedule::Input, apply_scroll);
    }
    if mode == 3 {
        app.add_plugin(Shell);
    }
    app.add_plugin(UiPlugin);
    if mode == 2 {
        app.add_system_front(Schedule::Input, apply_scroll);
    }
    if mode == 4 {
        app.add_plugin(Shell);
    }
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(200.0, 100.0));
    let port = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Fill,
            ..Layout::column()
        },
        Scroll::default(),
    ));
    let content = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Px(500.0),
        ..Layout::column()
    },));
    set_parent(app.world_mut(), content, port);
    app.update();
    (app, port, content)
}

#[test]
fn minimal_and_legacy_hosts_scroll_wheel_once() {
    for mode in 0..5 {
        let (mut app, port, _) = host(mode);
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [20.0, 50.0];
        input.scroll = [0.0, -20.0];
        app.update();
        assert_eq!(
            app.world().get::<Scroll>(port).unwrap().offset[1],
            -20.0,
            "host {mode}"
        );
        assert_eq!(app.world().resource::<Input>().unwrap().scroll, [0.0, 0.0]);
        app.update();
        assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], -20.0);
    }
}

#[test]
fn minimal_and_legacy_hosts_apply_drag_once_and_release() {
    for mode in 0..5 {
        let (mut app, port, _) = host(mode);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 60.0],
            pressed: true,
            down: true,
            ..Pointer::default()
        };
        app.update();
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 40.0],
            down: true,
            ..Pointer::default()
        };
        app.update();
        assert_eq!(
            app.world().get::<Scroll>(port).unwrap().offset[1],
            -20.0,
            "host {mode}"
        );
        app.update(); // same position is not another delta
        assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], -20.0);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 10.0],
            released: true,
            ..Pointer::default()
        };
        app.update();
        assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], -20.0);
    }
}

// Fixed-height release targets make offset/geometry chronology observable without
// depending on font metrics. All five supported registration orders are exercised.
fn targets(mode: u8) -> (App, Entity, Vec<Entity>) {
    let (mut app, port, content) = host(mode);
    let rows = (0..10)
        .map(|i| {
            let e = app.world_mut().spawn((
                Layout {
                    width: Size::Fill,
                    height: Size::Px(50.0),
                    ..Layout::column()
                },
                Button::new(format!("row-{i}")),
                Focusable,
                ReleaseActivation,
            ));
            set_parent(app.world_mut(), e, content);
            e
        })
        .collect();
    app.update();
    (app, port, rows)
}

#[test]
fn wheel_then_pointer_press_and_release_use_scrolled_geometry() {
    for mode in 0..5 {
        let (mut app, port, rows) = targets(mode);
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.scroll = [0.0, -50.0];
        input.pointer = Pointer {
            position: [20.0, 25.0],
            press_at: Some([20.0, 25.0]),
            pressed: true,
            down: true,
            ..Pointer::default()
        };
        app.update();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(rows[1]),
            "host {mode}: pointer must use post-wheel rects"
        );
        assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], -50.0);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 25.0],
            released: true,
            ..Pointer::default()
        };
        app.update();
        assert_eq!(
            take_action_intents(app.world_mut())
                .iter()
                .map(|i| i.entity)
                .collect::<Vec<_>>(),
            vec![rows[1]],
            "host {mode}"
        );
    }
}

#[test]
fn wheel_then_keyboard_reveal_uses_post_wheel_rects() {
    for mode in 0..5 {
        let (mut app, port, rows) = targets(mode);
        request_focus(app.world_mut(), rows[0]);
        let input = app.world_mut().resource_mut::<Input>().unwrap();
        input.pointer.position = [20.0, 25.0];
        input.scroll = [0.0, -100.0];
        input.keys = vec![KeyEvent::named(NamedKey::Tab, Modifiers::default())];
        app.update();
        assert_eq!(
            app.world().resource::<Focus>().unwrap().entity,
            Some(rows[1])
        );
        let r = app.world().get::<Rect>(rows[1]).unwrap();
        assert!(
            r.y >= 0.0 && r.y + r.h <= 100.0,
            "host {mode}: focused row must be revealed, rect={r:?}, offset={:?}",
            app.world().get::<Scroll>(port).unwrap().offset
        );
    }
}

#[test]
fn drag_cancels_release_activation_after_scrolling() {
    for mode in 0..5 {
        let (mut app, port, _) = targets(mode);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 75.0],
            press_at: Some([20.0, 75.0]),
            pressed: true,
            down: true,
            ..Pointer::default()
        };
        app.update();
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 25.0],
            down: true,
            ..Pointer::default()
        };
        app.update();
        assert_eq!(app.world().get::<Scroll>(port).unwrap().offset[1], -50.0);
        app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
            position: [20.0, 25.0],
            released: true,
            ..Pointer::default()
        };
        app.update();
        assert!(
            take_action_intents(app.world_mut()).is_empty(),
            "host {mode}: drag must not click"
        );
    }
}

#[test]
fn focused_row_does_not_snap_back_after_wheel_or_drag() {
    for mode in 0..5 {
        for drag in [false, true] {
            let (mut app, port, rows) = targets(mode);
            request_focus(app.world_mut(), rows[0]);
            app.update(); // settle the explicit focus request before scrolling
            if drag {
                app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
                    position: [20.0, 25.0],
                    press_at: Some([20.0, 25.0]),
                    pressed: true,
                    down: true,
                    ..Pointer::default()
                };
                app.update();
                app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
                    position: [20.0, -25.0],
                    down: true,
                    ..Pointer::default()
                };
            } else {
                let input = app.world_mut().resource_mut::<Input>().unwrap();
                input.pointer.position = [20.0, 25.0];
                input.scroll = [0.0, -50.0];
            }
            app.update();
            assert_eq!(
                app.world().get::<Scroll>(port).unwrap().offset[1],
                -50.0,
                "host {mode}, drag={drag}"
            );
            app.world_mut().resource_mut::<Input>().unwrap().pointer = Pointer {
                position: [20.0, -25.0],
                released: drag,
                ..Pointer::default()
            };
            app.update();
            app.update();
            assert_eq!(
                app.world().get::<Scroll>(port).unwrap().offset[1],
                -50.0,
                "host {mode}, drag={drag}: idle must not undo scrolling"
            );
            assert!(take_action_intents(app.world_mut()).is_empty());
            // An explicit focus request still reveals even the same target.
            request_focus(app.world_mut(), rows[0]);
            app.update();
            assert_eq!(
                app.world().get::<Scroll>(port).unwrap().offset[1],
                0.0,
                "host {mode}: explicit focus must reveal"
            );
        }
    }
}

#[test]
fn wheel_is_applied_before_later_input_systems() {
    fn observe(world: &mut World) {
        let port = *world.resource::<Entity>().unwrap();
        assert_eq!(world.get::<Scroll>(port).unwrap().offset[1], -20.0);
    }
    let (mut app, port, _) = host(0);
    app.world_mut().insert_resource(port);
    app.add_system(Schedule::Input, observe);
    app.world_mut().resource_mut::<Input>().unwrap().scroll = [0.0, -20.0];
    app.update();
}
