use std::time::Duration;

use rndr_core::{
    resize_edge_at, take_window_actions, App, Button, Color, CursorStyle, Entity, Fired,
    GenericInteraction, Input, Layout, Platform, PointerButton, Rect, ResizeEdge, Size, Titlebar,
    Viewport, WindowAction, WindowButton, WindowChrome, WindowConfig, WindowDrag,
};

struct Chrome {
    app: App,
    clock: std::sync::Arc<rndr_core::ManualClock>,
    band: Entity,
    title: Entity,
    tab: Entity,
    close: Entity,
    body: Entity,
}

fn boot(platform: Platform) -> Chrome {
    let mut app = App::new();
    app.world_mut().insert_resource(WindowConfig {
        titlebar: Some(Titlebar::new(40.0)),
        ..WindowConfig::default()
    });
    app.world_mut().insert_resource(platform);
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Viewport::new(400.0, 300.0));
    let clock = rndr_core::ManualClock::handle();
    app.set_clock(clock.clone());
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let band = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.0),
            ..Layout::row()
        },
        Color::rgb(0.1, 0.1, 0.1),
    ));
    app.world_mut().insert(band, WindowDrag);
    rndr_core::set_parent(app.world_mut(), band, root);
    let title = app.world_mut().spawn((Layout {
        width: Size::Px(100.0),
        height: Size::Fill,
        ..Layout::row()
    },));
    rndr_core::set_parent(app.world_mut(), title, band);
    let tab = app.world_mut().spawn((
        Layout {
            width: Size::Px(80.0),
            height: Size::Fill,
            ..Layout::row()
        },
        Button::new("tab"),
    ));
    rndr_core::set_parent(app.world_mut(), tab, band);
    let close = app.world_mut().spawn((Layout {
        width: Size::Px(46.0),
        height: Size::Fill,
        ..Layout::row()
    },));
    app.world_mut().insert(close, WindowButton::Close);
    rndr_core::set_parent(app.world_mut(), close, band);
    let body = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    rndr_core::set_parent(app.world_mut(), body, root);
    app.update();
    Chrome {
        app,
        clock,
        band,
        title,
        tab,
        close,
        body,
    }
}

fn center(app: &App, e: Entity) -> [f32; 2] {
    let r = *app.world().get::<Rect>(e).unwrap();
    [r.x + r.w / 2.0, r.y + r.h / 2.0]
}

/// Press and release at `at`; the actions the press queued and what fired.
fn press(
    app: &mut App,
    at: [f32; 2],
    button: PointerButton,
) -> (Vec<WindowAction>, Option<String>) {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.button = button;
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    let actions = take_window_actions(app.world_mut());
    let fired = app.world().resource::<Fired>().and_then(|f| f.name.clone());
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.down = false;
    input.pointer.released = true;
    app.update();
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .released = false;
    (actions, fired)
}

fn hover(app: &mut App, at: [f32; 2]) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = at;
    app.update();
}

#[test]
fn drag_region_moves_the_window_but_buttons_inside_keep_their_presses() {
    let mut c = boot(Platform::X11);
    let at = center(&c.app, c.title);
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::Drag]
    );

    let at = center(&c.app, c.tab);
    let (actions, fired) = press(&mut c.app, at, PointerButton::Left);
    assert!(actions.is_empty());
    assert_eq!(fired.as_deref(), Some("tab"));

    let at = center(&c.app, c.body);
    assert!(press(&mut c.app, at, PointerButton::Left).0.is_empty());
    let _ = c.band;
}

#[test]
fn double_click_on_a_drag_region_maximizes_and_a_slow_one_drags_again() {
    let mut c = boot(Platform::X11);
    let at = center(&c.app, c.title);
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::Drag]
    );
    c.clock.advance(Duration::from_millis(200));
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::TitlebarDoubleClick]
    );
    c.clock.advance(Duration::from_millis(200));
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::Drag]
    );
    c.clock.advance(Duration::from_millis(900));
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::Drag]
    );
}

#[test]
fn right_click_on_a_drag_region_opens_the_window_menu() {
    let mut c = boot(Platform::Windows);
    let at = center(&c.app, c.title);
    assert_eq!(
        press(&mut c.app, at, PointerButton::Right).0,
        [WindowAction::WindowMenu(at)]
    );
}

#[test]
fn caption_buttons_queue_their_action_and_report_hover() {
    let mut c = boot(Platform::Windows);
    let at = center(&c.app, c.close);
    hover(&mut c.app, at);
    let chrome = *c.app.world().resource::<WindowChrome>().unwrap();
    assert_eq!(chrome.hovered, Some(c.close));
    assert!(chrome.draws_controls && chrome.resize_edges);
    assert_eq!(
        press(&mut c.app, at, PointerButton::Left).0,
        [WindowAction::Close]
    );
    let at = center(&c.app, c.body);
    hover(&mut c.app, at);
    assert_eq!(
        c.app.world().resource::<WindowChrome>().unwrap().hovered,
        None
    );
}

#[test]
fn frameless_edges_resize_with_matching_cursors_and_take_the_press() {
    let mut c = boot(Platform::X11);
    hover(&mut c.app, [399.0, 150.0]);
    assert_eq!(
        c.app.world().resource::<CursorStyle>().copied(),
        Some(CursorStyle::ResizeEast)
    );
    hover(&mut c.app, [200.0, 150.0]);
    assert_eq!(
        c.app.world().resource::<CursorStyle>().copied(),
        Some(CursorStyle::Arrow)
    );
    // The top edge lies over the drag band; the resize wins.
    assert_eq!(
        press(&mut c.app, [2.0, 2.0], PointerButton::Left).0,
        [WindowAction::Resize(ResizeEdge::NorthWest)]
    );
    assert_eq!(
        press(&mut c.app, [200.0, 2.0], PointerButton::Left).0,
        [WindowAction::Resize(ResizeEdge::North)]
    );

    c.app
        .world_mut()
        .resource_mut::<WindowChrome>()
        .unwrap()
        .maximized = true;
    assert_eq!(
        press(&mut c.app, [50.0, 2.0], PointerButton::Left).0,
        [WindowAction::Drag]
    );
}

#[test]
fn macos_keeps_its_native_frame_and_controls() {
    let c = boot(Platform::Mac);
    let chrome = *c.app.world().resource::<WindowChrome>().unwrap();
    assert!(!chrome.draws_controls && !chrome.resize_edges);
    let frameless = WindowConfig {
        decorations: false,
        ..WindowConfig::default()
    };
    assert!(WindowChrome::expected(&frameless, Platform::Mac).resize_edges);
    let fixed = WindowConfig {
        resizable: false,
        ..frameless
    };
    assert!(!WindowChrome::expected(&fixed, Platform::Wayland).resize_edges);
}

#[test]
fn edge_hit_testing_prefers_corners_near_the_ends() {
    let edge = |x, y| resize_edge_at([x, y], 400.0, 300.0);
    assert_eq!(edge(0.0, 0.0), Some(ResizeEdge::NorthWest));
    assert_eq!(edge(10.0, 1.0), Some(ResizeEdge::NorthWest));
    assert_eq!(edge(20.0, 1.0), Some(ResizeEdge::North));
    assert_eq!(edge(399.0, 299.0), Some(ResizeEdge::SouthEast));
    assert_eq!(edge(1.0, 150.0), Some(ResizeEdge::West));
    assert_eq!(edge(398.0, 5.0), Some(ResizeEdge::NorthEast));
    assert_eq!(edge(200.0, 297.0), Some(ResizeEdge::South));
    assert_eq!(edge(200.0, 150.0), None);
    assert_eq!(edge(-1.0, 150.0), None);
}
