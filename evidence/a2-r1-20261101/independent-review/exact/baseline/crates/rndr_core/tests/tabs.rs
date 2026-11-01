use rndr_core::{
    hit_test, set_parent, App, Axis, Chrome, Color, Hidden, Input, Label, Layout, Rect, Size, Tab,
    TabPanel, Tabs, Viewport,
};

fn px(w: f32, h: f32) -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(w),
        height: Size::Px(h),
        min_width: 0.0,
        min_height: 0.0,
    }
}

fn hug_column() -> Layout {
    Layout {
        axis: Axis::Column,
        gap: 0.0,
        padding: 0.0,
        width: Size::Px(120.0),
        height: Size::Hug,
        min_width: 0.0,
        min_height: 0.0,
    }
}

struct Panes {
    tab_b: rndr_core::Entity,
    panel_a: rndr_core::Entity,
    panel_b: rndr_core::Entity,
}

fn boot() -> (App, Panes) {
    let mut app = App::new();
    app.add_plugin(Chrome);
    app.world_mut().insert_resource(Viewport::new(200.0, 200.0));
    let panes = {
        let world = app.world_mut();
        let tabs = world.spawn((
            hug_column(),
            Color::rgb(0.1, 0.1, 0.1),
            Tabs { selected: 0 },
        ));
        let tab_a = world.spawn((hug_column(), Color::rgb(0.2, 0.2, 0.2), Tab { id: 0 }));
        set_parent(world, tab_a, tabs);
        let header_a = world.spawn((px(120.0, 24.0), Color::rgb(0.3, 0.3, 0.3), Label::new("A")));
        set_parent(world, header_a, tab_a);
        let panel_a = world.spawn((
            px(120.0, 40.0),
            Color::rgb(0.4, 0.2, 0.2),
            Label::new("Panel A"),
            TabPanel,
        ));
        set_parent(world, panel_a, tab_a);

        let tab_b = world.spawn((hug_column(), Color::rgb(0.2, 0.2, 0.25), Tab { id: 1 }));
        set_parent(world, tab_b, tabs);
        let header_b = world.spawn((px(120.0, 24.0), Color::rgb(0.3, 0.3, 0.35), Label::new("B")));
        set_parent(world, header_b, tab_b);
        let panel_b = world.spawn((
            px(120.0, 60.0),
            Color::rgb(0.2, 0.4, 0.2),
            Label::new("Panel B"),
            TabPanel,
        ));
        set_parent(world, panel_b, tab_b);
        Panes {
            tab_b,
            panel_a,
            panel_b,
        }
    };
    app.update();
    (app, panes)
}

fn click(app: &mut App, x: f32, y: f32) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = [x, y];
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
}

#[test]
fn click_switches_the_visible_panel() {
    let (mut app, panes) = boot();
    assert!(!app.world().has::<Hidden>(panes.panel_a));
    assert!(app.world().has::<Hidden>(panes.panel_b));
    assert_eq!(app.world().get::<Rect>(panes.panel_a).unwrap().h, 40.0);
    assert_eq!(app.world().get::<Rect>(panes.panel_b).unwrap().h, 0.0);

    let header = {
        let world = app.world();
        let tab = world.get::<Rect>(panes.tab_b).unwrap();
        (tab.x + 4.0, tab.y + 4.0)
    };
    click(&mut app, header.0, header.1);

    assert!(app.world().has::<Hidden>(panes.panel_a));
    assert!(!app.world().has::<Hidden>(panes.panel_b));
    assert_eq!(app.world().get::<Rect>(panes.panel_a).unwrap().h, 0.0);
    assert_eq!(app.world().get::<Rect>(panes.panel_b).unwrap().h, 60.0);
}

#[test]
fn hidden_panel_is_not_the_hit_target() {
    let (app, panes) = boot();
    let panel_a = *app.world().get::<Rect>(panes.panel_a).unwrap();
    let hit = hit_test(app.world(), panel_a.x + 4.0, panel_a.y + 4.0);
    assert_ne!(hit, Some(panes.panel_b));
    assert_eq!(hit, Some(panes.panel_a));

    let hidden = *app.world().get::<Rect>(panes.panel_b).unwrap();
    assert_eq!(hidden.h, 0.0);
    let missed = hit_test(app.world(), 4.0, panel_a.y + panel_a.h + 30.0);
    assert_ne!(missed, Some(panes.panel_b));
}
