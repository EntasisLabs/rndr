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
