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
