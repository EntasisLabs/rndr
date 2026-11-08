use rndr_core::*;
use rndr_ui::menu::{
    open_menu, MenuAt, MenuDismissed, MenuEntry, MenuHandle, MenuItem, MenuPicked,
};
use rndr_ui::{UiError, UiPlugin};

fn boot() -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugin(UiPlugin);
    app.world_mut().insert_resource(Viewport::new(500.0, 400.0));
    let root = app.world_mut().spawn((Layout {
        width: Size::Fill,
        height: Size::Fill,
        ..Layout::column()
    },));
    let card = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(60.0),
            ..Layout::column()
        },
        Color::rgb(0.2, 0.2, 0.2),
        ContextTarget,
        Focusable,
    ));
    set_parent(app.world_mut(), card, root);
    let other = app.world_mut().spawn((
        Layout {
            width: Size::Px(200.0),
            height: Size::Px(60.0),
            ..Layout::column()
        },
        Color::rgb(0.3, 0.3, 0.3),
        Button::new("other"),
    ));
    set_parent(app.world_mut(), other, root);
    app.update();
    (app, card, other)
}

fn entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::Group("Edit".into()),
        MenuItem::new(1, "Cut").shortcut("Ctrl+X").into(),
        MenuItem::new(2, "Copy").shortcut("Ctrl+C").into(),
        MenuItem::new(3, "Paste").enabled(false).into(),
        MenuEntry::Separator,
        MenuEntry::submenu(
            "Sort by",
            vec![
                MenuItem::new(10, "Name").checked(true).into(),
                MenuItem::new(11, "Date").checked(false).into(),
            ],
        ),
        MenuItem::new(4, "Delete").danger().into(),
    ]
}

fn open(app: &mut App, card: Entity, at: MenuAt) -> MenuHandle {
    let h = open_menu(app.world_mut(), card, at, &entries()).unwrap();
    app.update();
    h
}

fn focus(app: &App) -> Option<Entity> {
    app.world().resource::<Focus>().and_then(|f| f.entity)
}

fn key(app: &mut App, named: NamedKey) {
    app.world_mut().resource_mut::<Input>().unwrap().keys =
        vec![KeyEvent::named(named, Modifiers::default())];
    app.update();
}

fn picked(app: &App) -> Vec<MenuPicked> {
    events::<MenuPicked>(app.world()).to_vec()
}

fn dismissed(app: &App) -> Vec<MenuDismissed> {
    events::<MenuDismissed>(app.world()).to_vec()
}

fn rect(app: &App, e: Entity) -> Rect {
    *app.world().get::<Rect>(e).unwrap()
}

fn move_to(app: &mut App, at: [f32; 2]) {
    app.world_mut()
        .resource_mut::<Input>()
        .unwrap()
        .pointer
        .position = at;
    app.update();
}

fn click(app: &mut App, at: [f32; 2]) -> Option<String> {
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.button = PointerButton::Left;
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
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
    fired
}

fn center(r: Rect) -> [f32; 2] {
    [r.x + r.w / 2.0, r.y + r.h / 2.0]
}

#[test]
fn keyboard_walks_enabled_items_and_picks_one() {
    let (mut app, card, _) = boot();
    let h = open(&mut app, card, MenuAt::Point([40.0, 30.0]));
    let r = rect(&app, h.root);
    assert_eq!((r.x, r.y), (40.0, 34.0));
    assert!(r.w >= 180.0);
    let cut = h.item(app.world(), 1).unwrap();
    let copy = h.item(app.world(), 2).unwrap();
    let delete = h.item(app.world(), 4).unwrap();
    let sort = h.submenu_row(app.world(), "Sort by").unwrap();
    assert_eq!(focus(&app), Some(cut));
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(focus(&app), Some(copy));
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(focus(&app), Some(sort), "disabled Paste is skipped");
    key(&mut app, NamedKey::End);
    assert_eq!(focus(&app), Some(delete));
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(focus(&app), Some(cut), "wraps");
    key(&mut app, NamedKey::ArrowUp);
    assert_eq!(focus(&app), Some(delete));
    key(&mut app, NamedKey::Home);
    key(&mut app, NamedKey::ArrowDown);
    key(&mut app, NamedKey::Enter);
    assert_eq!(
        picked(&app),
        vec![MenuPicked {
            menu: h.root,
            item: 2
        }]
    );
    assert!(dismissed(&app).is_empty());
    assert!(!h.is_open(app.world()));
    assert_eq!(focus(&app), Some(card));
    app.update();
    assert!(!app.world().is_alive(h.root), "closed menus are despawned");
}

#[test]
fn submenus_open_with_right_close_with_left_and_pick_for_the_top_menu() {
    let (mut app, card, _) = boot();
    let h = open(&mut app, card, MenuAt::Below);
    let sort = h.submenu_row(app.world(), "Sort by").unwrap();
    let name = h.item(app.world(), 10).unwrap();
    let date = h.item(app.world(), 11).unwrap();
    request_focus(app.world_mut(), sort);
    app.update();
    key(&mut app, NamedKey::ArrowRight);
    assert_eq!(focus(&app), Some(name));
    let (row, sub) = (rect(&app, sort), rect(&app, name));
    assert!(sub.x >= row.x + row.w, "submenu opens beside its row");
    key(&mut app, NamedKey::ArrowLeft);
    assert_eq!(focus(&app), Some(sort));
    assert!(h.is_open(app.world()));
    key(&mut app, NamedKey::Enter);
    assert_eq!(focus(&app), Some(name), "Enter on a submenu row opens it");
    key(&mut app, NamedKey::Escape);
    assert_eq!(focus(&app), Some(sort), "Escape closes one level");
    assert!(dismissed(&app).is_empty());
    key(&mut app, NamedKey::ArrowRight);
    key(&mut app, NamedKey::ArrowDown);
    assert_eq!(focus(&app), Some(date));
    key(&mut app, NamedKey::Space);
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    let mut up = KeyEvent::named(NamedKey::Space, Modifiers::default());
    up.released = true;
    input.keys = vec![up];
    app.update();
    assert_eq!(
        picked(&app),
        vec![MenuPicked {
            menu: h.root,
            item: 11
        }]
    );
    assert!(!h.is_open(app.world()));
}

#[test]
fn escape_and_outside_clicks_dismiss_without_clicking_through() {
    let (mut app, card, other) = boot();
    let h = open(&mut app, card, MenuAt::Point([300.0, 20.0]));
    key(&mut app, NamedKey::Escape);
    assert_eq!(dismissed(&app), vec![MenuDismissed { menu: h.root }]);
    assert_eq!(focus(&app), Some(card));

    let h = open(&mut app, card, MenuAt::Point([300.0, 20.0]));
    let at = center(rect(&app, other));
    assert!(!rect(&app, h.root).contains(at[0], at[1]));
    assert_eq!(click(&mut app, at), None, "the outside press is swallowed");
    assert!(!h.is_open(app.world()));
    assert_eq!(click(&mut app, at).as_deref(), Some("other"));
}

#[test]
fn clicking_and_hovering_rows() {
    let (mut app, card, _) = boot();
    let h = open(&mut app, card, MenuAt::Point([40.0, 30.0]));
    let copy = h.item(app.world(), 2).unwrap();
    let paste = h.item(app.world(), 3).unwrap();
    let sort = h.submenu_row(app.world(), "Sort by").unwrap();
    let name = h.item(app.world(), 10).unwrap();
    let at = center(rect(&app, copy));
    move_to(&mut app, at);
    assert_eq!(focus(&app), Some(copy));
    let at = center(rect(&app, paste));
    move_to(&mut app, at);
    assert_eq!(
        focus(&app),
        Some(copy),
        "disabled rows don't take the highlight"
    );
    let at = center(rect(&app, sort));
    move_to(&mut app, at);
    assert_eq!(focus(&app), Some(sort));
    app.update();
    assert!(!app
        .world()
        .has::<Hidden>(app.world().get::<Parent>(name).unwrap().entity));
    let at = center(rect(&app, copy));
    move_to(&mut app, at);
    app.update();
    assert!(app
        .world()
        .has::<Hidden>(app.world().get::<Parent>(name).unwrap().entity));

    let at = center(rect(&app, paste));
    assert_eq!(click(&mut app, at), None);
    assert!(picked(&app).is_empty());
    assert!(h.is_open(app.world()));
    let at = center(rect(&app, copy));
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.pressed = true;
    input.pointer.down = true;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.down = false;
    input.pointer.released = true;
    app.update();
    assert_eq!(
        picked(&app),
        vec![MenuPicked {
            menu: h.root,
            item: 2
        }]
    );
}

#[test]
fn opening_a_menu_replaces_the_open_one_and_rejects_bad_entries() {
    let (mut app, card, _) = boot();
    let first = open(&mut app, card, MenuAt::Point([10.0, 10.0]));
    let second = open(&mut app, card, MenuAt::Point([60.0, 60.0]));
    assert!(!first.is_open(app.world()));
    assert!(second.is_open(app.world()));
    assert_eq!(dismissed(&app), vec![MenuDismissed { menu: first.root }]);
    let world = app.world_mut();
    assert_eq!(
        open_menu(world, card, MenuAt::Below, &[]).unwrap_err(),
        UiError::InvalidProps
    );
    assert_eq!(
        open_menu(world, card, MenuAt::Below, &[MenuItem::new(1, " ").into()]).unwrap_err(),
        UiError::InvalidProps
    );
    assert_eq!(
        open_menu(
            world,
            card,
            MenuAt::Below,
            &[MenuEntry::submenu("Empty", vec![])]
        )
        .unwrap_err(),
        UiError::InvalidProps
    );
}

fn open_below(world: &mut World, button: Entity) {
    let _ = open_menu(world, button, MenuAt::Below, &entries());
}

#[test]
fn a_button_click_opens_a_menu_below_it() {
    let (mut app, _card, other) = boot();
    app.world_mut().insert(other, ReleaseActivation);
    app.world_mut().insert(other, Focusable);
    app.world_mut().insert(other, ActivationHandler(open_below));
    let text = app.world_mut().spawn((Layout::row(), Label::new("File")));
    set_parent(app.world_mut(), text, other);
    app.update();
    let at = center(rect(&app, text));
    // A fast click can press and release inside one native frame.
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.position = at;
    input.pointer.press_at = Some(at);
    input.pointer.button = PointerButton::Left;
    input.pointer.pressed = true;
    input.pointer.released = true;
    input.pointer.down = false;
    app.update();
    let input = app.world_mut().resource_mut::<Input>().unwrap();
    input.pointer.pressed = false;
    input.pointer.released = false;
    app.update();
    app.update();
    let f = focus(&app).expect("focus");
    assert_ne!(f, other, "focus moved into the menu");
    let root = app.world().get::<Parent>(f).map(|p| p.entity);
    let popup = root.and_then(|r| app.world().get::<AnchoredPopup>(r).copied());
    assert!(popup.is_some(), "focus is on a menu row");
    assert!(
        dismissed(&app).is_empty(),
        "menu stays open after the click"
    );
}
