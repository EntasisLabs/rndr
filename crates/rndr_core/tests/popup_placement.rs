use rndr_core::{
    open_popup, set_parent, AnchoredPopup, App, Color, Entity, Focus, Focusable,
    GenericInteraction, Hidden, Input, KeyEvent, Layout, Modifiers, NamedKey, PopupPlacement, Rect,
    Size, Viewport,
};

fn boot() -> (App, Entity) {
    let mut app = App::new();
    app.add_plugin(GenericInteraction);
    app.world_mut().insert_resource(Viewport::new(400.0, 300.0));
    let anchor = app.world_mut().spawn((
        Layout {
            width: Size::Px(100.0),
            height: Size::Px(30.0),
            ..Layout::column()
        },
        Color::rgb(0.2, 0.2, 0.2),
        Focusable,
    ));
    app.update();
    (app, anchor)
}

/// A hidden 120-point-wide popup with one focusable 40-point row.
fn popup(app: &mut App, anchor: Entity, placement: PopupPlacement) -> (Entity, Entity) {
    let root = app.world_mut().spawn((
        Layout {
            width: Size::Px(120.0),
            ..Layout::column()
        },
        Color::rgb(0.1, 0.1, 0.1),
        Hidden,
        placement,
        AnchoredPopup {
            anchor,
            max_height: 200.0,
            closed: |_, _| {},
        },
    ));
    let row = app.world_mut().spawn((
        Layout {
            width: Size::Fill,
            height: Size::Px(40.0),
            ..Layout::column()
        },
        Color::rgb(0.3, 0.3, 0.3),
        Focusable,
    ));
    set_parent(app.world_mut(), row, root);
    (root, row)
}

fn show(app: &mut App, root: Entity, focus: Entity) {
    assert!(open_popup(app.world_mut(), root));
    app.world_mut().resource_mut::<Focus>().unwrap().entity = Some(focus);
    app.update();
}

fn rect(app: &App, e: Entity) -> Rect {
    *app.world().get::<Rect>(e).unwrap()
}

fn visible(app: &App, e: Entity) -> bool {
    !app.world().has::<Hidden>(e)
}

fn key(app: &mut App, named: NamedKey) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    app.update();
}

#[test]
fn point_popups_open_at_their_own_width_and_stay_inside_the_window() {
    let (mut app, anchor) = boot();
    let (root, row) = popup(&mut app, anchor, PopupPlacement::At([50.0, 60.0]));
    show(&mut app, root, row);
    let r = rect(&app, root);
    assert_eq!((r.x, r.y, r.w, r.h), (50.0, 64.0, 120.0, 40.0));

    let (corner, corner_row) = popup(&mut app, anchor, PopupPlacement::At([350.0, 280.0]));
    show(&mut app, corner, corner_row);
    assert!(!visible(&app, root));
    let r = rect(&app, corner);
    assert_eq!((r.x, r.y, r.w, r.h), (280.0, 236.0, 120.0, 40.0));
}

#[test]
fn submenus_open_beside_their_row_and_close_level_by_level() {
    let (mut app, anchor) = boot();
    let (menu, row) = popup(&mut app, anchor, PopupPlacement::Below);
    let (sub, sub_row) = popup(&mut app, row, PopupPlacement::Beside);
    show(&mut app, menu, row);
    show(&mut app, sub, sub_row);
    assert!(visible(&app, menu) && visible(&app, sub));
    let (m, r, s) = (rect(&app, menu), rect(&app, row), rect(&app, sub));
    assert_eq!(s.x, r.x + r.w);
    assert_eq!(s.y, r.y - 4.0);
    assert!(m.w >= 240.0);

    key(&mut app, NamedKey::Escape);
    assert!(visible(&app, menu));
    assert!(!visible(&app, sub));
    assert_eq!(app.world().resource::<Focus>().unwrap().entity, Some(row));

    // A press inside the parent menu keeps both levels; outside closes all.
    show(&mut app, sub, sub_row);
    let at = [s.x + s.w + 20.0, 290.0];
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    assert!(!visible(&app, menu) && !visible(&app, sub));
    assert_eq!(
        app.world().resource::<Focus>().unwrap().entity,
        Some(anchor)
    );
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

    show(&mut app, menu, row);
    show(&mut app, sub, sub_row);
    key(&mut app, NamedKey::Tab);
    assert!(!visible(&app, menu) && !visible(&app, sub));
}

#[test]
fn opening_a_sibling_submenu_closes_the_other_but_keeps_the_parent() {
    let (mut app, anchor) = boot();
    let (menu, row) = popup(&mut app, anchor, PopupPlacement::Below);
    let (first, first_row) = popup(&mut app, row, PopupPlacement::Beside);
    let (second, second_row) = popup(&mut app, row, PopupPlacement::Beside);
    show(&mut app, menu, row);
    show(&mut app, first, first_row);
    show(&mut app, second, second_row);
    assert!(visible(&app, menu));
    assert!(!visible(&app, first));
    assert!(visible(&app, second));
}

#[test]
fn below_popups_near_the_right_edge_align_to_the_anchor_end() {
    use rndr_core::popup_rect;
    let viewport = Viewport::new(400.0, 300.0);
    let at = |x: f32| Rect {
        x,
        y: 100.0,
        w: 60.0,
        h: 30.0,
    };
    // Room to the right: the leading edges align, as before.
    assert_eq!(popup_rect(at(20.0), viewport, 120.0, 40.0).x, 20.0);
    // A trigger at the trailing edge keeps its menu under it, not flush
    // against the window edge.
    let end = popup_rect(at(330.0), viewport, 120.0, 40.0);
    assert_eq!(end.x + end.w, 390.0);
    // Wider than the space left of the anchor end: still inside the window.
    let wide = popup_rect(at(20.0), viewport, 120.0, 40.0);
    assert!(wide.x >= 0.0 && wide.x + wide.w <= 400.0);
    let squeezed = popup_rect(at(10.0), Viewport::new(100.0, 300.0), 120.0, 40.0);
    assert_eq!((squeezed.x, squeezed.w), (0.0, 100.0));
}

mod reveal {
    use rndr_core::motion::MotionSettings;
    use rndr_core::{
        open_popup, set_parent, AnchoredPopup, Color, Entity, Focus, Focusable, GenericInteraction,
        Hidden, Layout, Opacity, PopupPlacement, Rect, Sim, Size, Transform, Viewport,
    };
    use std::time::Duration;

    fn boot() -> (Sim, Entity, Entity) {
        let mut sim = Sim::new();
        sim.app.add_plugin(GenericInteraction);
        sim.world().insert_resource(Viewport::new(400.0, 300.0));
        let anchor = sim.world().spawn((
            Layout {
                width: Size::Px(100.0),
                height: Size::Px(30.0),
                ..Layout::column()
            },
            Color::rgb(0.2, 0.2, 0.2),
            Focusable,
        ));
        let root = sim.world().spawn((
            Layout {
                width: Size::Px(120.0),
                ..Layout::column()
            },
            Color::rgb(0.1, 0.1, 0.1),
            Hidden,
            PopupPlacement::Below,
            AnchoredPopup {
                anchor,
                max_height: 200.0,
                closed: |_, _| {},
            },
        ));
        let row = sim.world().spawn((
            Layout {
                width: Size::Fill,
                height: Size::Px(40.0),
                ..Layout::column()
            },
            Focusable,
        ));
        set_parent(sim.world(), row, root);
        sim.update();
        (sim, anchor, root)
    }

    /// Open with focus inside, as a real trigger does.
    fn open(sim: &mut Sim, root: Entity) {
        assert!(open_popup(sim.world(), root));
        let row = rndr_core::children_of(sim.world(), root)[0];
        sim.world().resource_mut::<Focus>().unwrap().entity = Some(row);
    }

    fn step(sim: &mut Sim, ms: u64) {
        sim.advance(Duration::from_millis(ms));
        sim.update();
    }

    fn opacity(sim: &mut Sim, e: Entity) -> f32 {
        sim.world().get::<Opacity>(e).map_or(1.0, |o| o.0)
    }

    fn dy(sim: &mut Sim, e: Entity) -> f32 {
        sim.world()
            .get::<Transform>(e)
            .map_or(0.0, |t| t.translate[1])
    }

    #[test]
    fn opening_fades_and_slides_from_the_anchor_then_settles() {
        let (mut sim, anchor, root) = boot();
        step(&mut sim, 16);
        open(&mut sim, root);
        step(&mut sim, 16);
        // The opening frame presents the hidden end; the next is in flight.
        assert_eq!(opacity(&mut sim, root), 0.0);
        step(&mut sim, 40);
        let popup = *sim.world().get::<Rect>(root).unwrap();
        let below = popup.y >= sim.world().get::<Rect>(anchor).unwrap().y;
        assert!(below);
        let (a, y) = (opacity(&mut sim, root), dy(&mut sim, root));
        assert!(a > 0.0 && a < 1.0, "fading in: {a}");
        assert!(
            y < 0.0 && y > -4.01,
            "slides down from the anchor side: {y}"
        );
        // Layout and hit geometry never move; only the presented layer does.
        assert_eq!(*sim.world().get::<Rect>(root).unwrap(), popup);
        step(&mut sim, 400);
        assert_eq!(opacity(&mut sim, root), 1.0);
        assert_eq!(dy(&mut sim, root), 0.0);
    }

    #[test]
    fn reduced_motion_keeps_position_and_only_fades() {
        let (mut sim, _, root) = boot();
        sim.world().insert_resource(MotionSettings {
            reduced_motion: true,
            ..Default::default()
        });
        step(&mut sim, 16);
        open(&mut sim, root);
        step(&mut sim, 16);
        step(&mut sim, 40);
        assert_eq!(dy(&mut sim, root), 0.0);
        assert!(opacity(&mut sim, root) < 1.0);
    }

    #[test]
    fn frames_without_a_running_clock_show_popups_settled() {
        let (mut sim, _, root) = boot();
        open(&mut sim, root);
        sim.update();
        assert_eq!(opacity(&mut sim, root), 1.0);
        assert_eq!(dy(&mut sim, root), 0.0);
    }

    #[test]
    fn reopening_replays_the_entrance() {
        let (mut sim, _, root) = boot();
        step(&mut sim, 16);
        open(&mut sim, root);
        step(&mut sim, 16);
        step(&mut sim, 400);
        assert_eq!(opacity(&mut sim, root), 1.0);
        rndr_core::close_popup(sim.world(), root, false);
        step(&mut sim, 16);
        assert!(sim.world().has::<Hidden>(root), "closing is immediate");
        open(&mut sim, root);
        step(&mut sim, 16);
        step(&mut sim, 40);
        assert!(opacity(&mut sim, root) < 1.0);
    }
}

#[test]
fn a_dropdown_takes_a_wider_authored_width_within_the_window() {
    let (mut app, anchor) = boot();
    let (root, row) = popup(&mut app, anchor, PopupPlacement::Below);
    app.world_mut().get_mut::<Layout>(root).unwrap().width = Size::Px(320.0);
    show(&mut app, root, row);
    assert_eq!(rect(&app, root).w, 320.0);
    app.world_mut().get_mut::<Layout>(root).unwrap().width = Size::Px(900.0);
    app.update();
    assert!(rect(&app, root).w <= 400.0, "clamped to the viewport");
}
